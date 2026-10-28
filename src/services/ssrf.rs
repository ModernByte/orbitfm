//! Синтаксическая политика сетевой цели (ADR-0025, ред. 2).
//!
//! Ярус 1 — чистая функция без DNS: вызывается с главного потока (`sync_media`)
//! и изнутри `runtime::block_on` в парке, где резолв остановил бы UI. Разрешение
//! имени и проверка адресов — ярус 2 в [`super::artwork`].

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use url::{Host, Url};

/// Вердикт яруса 1 с причиной (для `tracing`, новых строк i18n нет).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PolicyReject {
    /// Схема не `http`/`https`.
    UnsupportedScheme,
    /// Ярлык хоста отсутствует.
    MissingHost,
    /// Одноярлыковый хост: резолвится search-domains во внутреннюю сеть.
    SingleLabel,
    /// `localhost`, mDNS и служебные домены.
    LocalDomain,
    /// `userinfo` в URL (`http://localhost@good.example`).
    HasUserinfo,
    /// Явный порт 0.
    ZeroPort,
    /// Адрес входит в приватный или служебный диапазон.
    PrivateAddress,
}

impl std::fmt::Display for PolicyReject {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let label = match self {
            Self::UnsupportedScheme => "схема не http/https",
            Self::MissingHost => "хост не указан",
            Self::SingleLabel => "одноярлыковый хост",
            Self::LocalDomain => "локальный или служебный домен",
            Self::HasUserinfo => "userinfo в ссылке",
            Self::ZeroPort => "порт 0",
            Self::PrivateAddress => "приватный или служебный адрес",
        };
        f.write_str(label)
    }
}

/// Разбирает ссылку и проверяет её ярусом 1; `Some(Url)` — цель допустима.
pub(crate) fn parse_publishable(raw: &str) -> Option<Url> {
    let parsed = Url::parse(raw).ok()?;
    classify(&parsed).ok()
}

/// Проверяет уже разобранную ссылку и возвращает её же при успехе.
pub(crate) fn classify(url: &Url) -> Result<Url, PolicyReject> {
    if url.scheme() != "http" && url.scheme() != "https" {
        return Err(PolicyReject::UnsupportedScheme);
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(PolicyReject::HasUserinfo);
    }
    if url.port() == Some(0) {
        return Err(PolicyReject::ZeroPort);
    }
    match url.host() {
        Some(Host::Ipv4(address)) => {
            if !is_public_destination(IpAddr::V4(address)) {
                return Err(PolicyReject::PrivateAddress);
            }
        }
        Some(Host::Ipv6(address)) => {
            if !is_public_destination(IpAddr::V6(address)) {
                return Err(PolicyReject::PrivateAddress);
            }
        }
        Some(Host::Domain(name)) => {
            if let Some(reject) = domain_verdict(name) {
                return Err(reject);
            }
        }
        None => return Err(PolicyReject::MissingHost),
    }
    Ok(url.clone())
}

/// deny-by-default классификация адресного назначения.
///
/// `IpAddr::is_global` и семейство `is_*` (`is_reserved`, `is_shared`,
/// `is_benchmarking`, `Ipv6Addr::is_unicast_global`) на стабильном toolchain
/// недоступны, а `Ipv4Addr::is_private` не покрывает CGNAT `100.64/10` —
/// поэтому диапазоны перечислены явно и табличнотестируемо.
pub(crate) fn is_public_destination(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(address) => is_public_ipv4(address),
        IpAddr::V6(address) => is_public_ipv6(address),
    }
}

fn is_public_ipv4(address: Ipv4Addr) -> bool {
    let [first, second, third, _] = address.octets();
    !(address.is_unspecified()
        || address.is_loopback()
        || address.is_link_local()
        || address.is_private()
        || address.is_multicast()
        // 0.0.0.0/8 — «этот» сегмент, метаданные и недостижимые узлы.
        || first == 0
        // 100.64.0.0/10 — CGNAT, в `is_private` не входит.
        || (first == 100 && (second & 0b1100_0000) == 0b0100_0000)
        // 192.0.0.0/24 (IETF protocol assignments) и 192.0.2.0/24 (TEST-NET-1):
        // ровно /24, шире — легальные CDN-префиксы вида 192.0.66.0/24.
        || (first == 192 && second == 0 && matches!(third, 0 | 2))
        // 198.18.0.0/15 — бенчмарк-диапазон, классический пробник обхода.
        || (first == 198 && (second == 18 || second == 19))
        // 198.51.100.0/24 (TEST-NET-2) и 203.0.113.0/24 (TEST-NET-3).
        || (first == 198 && second == 51 && third == 100)
        || (first == 203 && second == 0 && third == 113)
        // 240.0.0.0/4 — зарезервировано и broadcast.
        || first >= 240)
}

fn is_public_ipv6(address: Ipv6Addr) -> bool {
    if address.is_unspecified() || address.is_loopback() || address.is_multicast() {
        return false;
    }
    let segments = address.segments();
    let local_ula = segments[0] & 0xfe00 == 0xfc00;
    let link_local = segments[0] & 0xffc0 == 0xfe80;
    let teredo = segments[0] == 0x2001 && segments[1] == 0x0000;
    let documentation = segments[0] == 0x2001 && segments[1] == 0x0db8;
    if local_ula || link_local || teredo || documentation {
        return false;
    }
    // 100::/64 — discard-only префикс.
    if segments[0] == 0x0100 {
        return false;
    }
    if let Some(inner) = v4_inner(address) {
        return is_public_ipv4(inner);
    }
    // 0000::/96 — устаревшие IPv4-compatible и прочие нулевые префиксы.
    !address.octets()[..10].iter().all(|byte| *byte == 0)
}

/// Вложенный IPv4: `::ffff:0:0/96` (v4-mapped) и `2002::/16` (6to4).
/// Рекурсия — ровно один уровень: вложенный адрес классифицируется как v4.
fn v4_inner(address: Ipv6Addr) -> Option<Ipv4Addr> {
    if let Some(mapped) = address.to_ipv4_mapped() {
        return Some(mapped);
    }
    let octets = address.octets();
    if octets[0] == 0x20 && octets[1] == 0x02 {
        return Some(Ipv4Addr::new(octets[2], octets[3], octets[4], octets[5]));
    }
    None
}

/// Вердикт по ярлыку хоста: `None` — допустим.
///
/// Не-ASCII здесь не проверяется: парсер WHATWG в `url` применяет IDNA-маппинг
/// до того, как мы получим ярлык, и возвращает punycode (`exämple.com` →
/// `xn--exmple-cua.com`). Голиф-обход поэтому не уходит в резолвер в виде
/// UTF-8, а решающим остаётся класс разрешённого адреса — ярус 2.
fn domain_verdict(name: &str) -> Option<PolicyReject> {
    // Корневая точка в DNS-записи легальна (`router.`, `LOCALHOST.`) и не должна
    // превращать запрет в пропуск: снимаем ровно один хвостовой разделитель.
    let trimmed = name.strip_suffix('.').unwrap_or(name);
    if trimmed.is_empty() {
        return Some(PolicyReject::MissingHost);
    }
    let lower = trimmed.to_ascii_lowercase();
    if lower == "localhost" || lower.ends_with(".localhost") {
        return Some(PolicyReject::LocalDomain);
    }
    // Граница ярлыка обязательна: `artwork-local.example` — не mDNS-домен.
    for suffix in ["local", "internal", "home.arpa", "localdomain"] {
        if lower == suffix || lower.ends_with(&format!(".{suffix}")) {
            return Some(PolicyReject::LocalDomain);
        }
    }
    if !lower.contains('.') {
        return Some(PolicyReject::SingleLabel);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ip(text: &str) -> IpAddr {
        text.parse().expect("тестовый адрес должен парситься")
    }

    fn verdict(reference: &str) -> Result<Url, PolicyReject> {
        classify(&Url::parse(reference).expect("тестовый URL должен парситься"))
    }

    #[test]
    fn public_ipv4_is_allowed() {
        for text in [
            "93.184.216.34",
            "8.8.8.8",
            "172.32.0.1",
            "193.168.255.255",
            "101.0.0.1",
            // Границы отрицанных диапазонов: за /10, за /24, вне 192.0.0/16-ловушки.
            "100.128.0.1",
            "192.0.66.1",
            "192.1.0.1",
            "198.17.255.255",
            "198.20.0.1",
        ] {
            assert!(
                is_public_destination(ip(text)),
                "{text} обязан быть публичным"
            );
        }
        assert!(verdict("https://93.184.216.34/a.png").is_ok());
    }

    #[test]
    fn private_ipv4_ranges_are_denied() {
        for text in [
            "0.0.0.0",
            "0.1.2.3",
            "127.0.0.1",
            "127.255.255.255",
            "10.0.0.1",
            "172.16.0.1",
            "192.168.1.1",
            "169.254.169.254",
            "169.254.1.1",
            "100.64.0.1",
            "100.127.255.255",
            "192.0.0.1",
            "192.0.2.1",
            "198.18.0.1",
            "198.19.255.255",
            "198.51.100.1",
            "203.0.113.1",
            "224.0.0.1",
            "239.255.255.255",
            "240.0.0.1",
            "255.255.255.255",
        ] {
            assert!(
                !is_public_destination(ip(text)),
                "{text} обязан быть приватным"
            );
        }
        // Тот же набор как цель URL: ярус 1 отбивает literal без резолва.
        for text in ["127.0.0.1", "169.254.169.254", "100.64.0.1", "10.0.0.5"] {
            assert_eq!(
                verdict(&format!("https://{text}/a.png")),
                Err(PolicyReject::PrivateAddress),
                "{text}"
            );
        }
    }

    #[test]
    fn cgnat_is_not_covered_by_is_private() {
        let cgnat = Ipv4Addr::new(100, 64, 0, 1);
        assert!(
            !cgnat.is_private(),
            "предпосылка таблицы: `is_private` не покрывает CGNAT"
        );
        assert!(!is_public_destination(IpAddr::V4(cgnat)));
    }

    #[test]
    fn private_ipv6_ranges_are_denied() {
        for text in [
            "::",
            "::1",
            "::ffff:127.0.0.1",
            "::ffff:10.0.0.1",
            "::ffff:8.8.8.8",
            "ff02::1",
            "fe80::1",
            "fc00::1",
            "fd12:3456::1",
            "2001:db8::1",
            "2001::1",
            "100::1",
            "2002:7f00:1::",
            "2002:0808:0808::",
        ] {
            let verdict = is_public_destination(ip(text));
            let expected = matches!(text, "::ffff:8.8.8.8" | "2002:0808:0808::");
            assert_eq!(
                verdict, expected,
                "{text}: публичность {verdict} не {expected}"
            );
        }
    }

    #[test]
    fn numeric_host_tricks_are_canonicalised_by_the_parser() {
        for reference in [
            "http://127.1/",
            "http://0177.0.0.1/",
            "http://0x7f.0.0.1/",
            "http://2130706433/",
            "http://[::1]/",
            "http://[0:0:0:0:0:ffff:127.0.0.1]/",
        ] {
            assert_eq!(
                verdict(reference),
                Err(PolicyReject::PrivateAddress),
                "{reference} обязана отбиваться как приватная цель"
            );
        }
    }

    #[test]
    fn local_and_single_label_domains_are_denied() {
        for reference in [
            "http://localhost/icon.png",
            "http://LOCALHOST./icon.png",
            "http://orbitfm.localhost/icon.png",
            "http://router/icon.png",
            "http://nas/icon.png",
            "http://metadata.google.internal/",
            "http://x.local/",
            "http://x.home.arpa/",
            "http://ROUTER./icon.png",
            "http://good.local./icon.png",
        ] {
            assert!(
                verdict(reference).is_err(),
                "{reference} обязана быть отклонена"
            );
        }
        assert_eq!(
            verdict("http://router/icon.png"),
            Err(PolicyReject::SingleLabel)
        );
        assert_eq!(
            verdict("http://metadata.google.internal/"),
            Err(PolicyReject::LocalDomain)
        );
        // Граница ярлыка: дефисное «local»/«internal» в середине имени — не mDNS.
        assert!(verdict("https://artwork-local.example/").is_ok());
        assert!(verdict("https://img-cdn.internal.example/").is_ok());
    }

    #[test]
    fn userinfo_and_port_zero_are_denied() {
        assert_eq!(
            verdict("http://localhost@good.example/"),
            Err(PolicyReject::HasUserinfo)
        );
        assert_eq!(
            verdict("https://good.example:0/"),
            Err(PolicyReject::ZeroPort)
        );
        assert_eq!(
            verdict("https://127.0.0.1@93.184.216.34/"),
            Err(PolicyReject::HasUserinfo)
        );
    }

    /// IDN-ловушки доходят до политики уже punycode'ом: парсер `url` применяет
    /// IDNA-маппинг, поэтому отдельное правило «не-ASCII» не нужно и не может
    /// сработать. Проверка фиксирует канонизацию, а не отклонение.
    #[test]
    fn unicode_hosts_arrive_canonicalised_to_punycode() {
        assert_eq!(
            verdict("https://exämple.com/a.png").map(|url| url.host_str().unwrap().to_owned()),
            Ok("xn--exmple-cua.com".to_owned())
        );
        // Полная ширина точки — не разделитель, а часть IDNA-маппинга: здесь
        // она разворачивается в обычный IPv4-literal и отбивается адресом.
        assert_eq!(
            verdict("https://127。0。0。1/"),
            Err(PolicyReject::PrivateAddress)
        );
    }

    #[test]
    fn non_http_schemes_are_denied() {
        for reference in [
            "file:///etc/passwd",
            "data:image/png;base64,AAAA",
            "gopher://127.0.0.1:1111",
            "ftp://good.example/a.png",
            "ws://good.example/a",
        ] {
            assert_eq!(
                verdict(reference),
                Err(PolicyReject::UnsupportedScheme),
                "{reference}"
            );
        }
        // Строка без схемы не разбирается: до политики не доходит.
        assert!(Url::parse("..not a url..").is_err());
        assert_eq!(parse_publishable("..not a url.."), None);
    }

    #[test]
    fn non_default_ports_are_allowed_but_zero_denied() {
        assert!(verdict("https://good.example:8443/icon.png").is_ok());
        assert!(
            verdict("http://93.184.216.34:9/")
                .is_ok_and(|url| url.port_or_known_default() == Some(9))
        );
    }

    #[test]
    fn publishable_reference_roundtrip() {
        assert_eq!(
            parse_publishable("https://cdn.example.com/i.png").map(|url| url.to_string()),
            Some("https://cdn.example.com/i.png".to_owned())
        );
        assert_eq!(
            parse_publishable("http://169.254.169.254/latest/meta-data/"),
            None
        );
        assert_eq!(parse_publishable("не url"), None);
    }
}
