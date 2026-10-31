//! Сетевой контур артворка: политика цели и перепроверка каждого хоппа
//! (ADR-0025, ред. 2).
//!
//! Модуль отдаёт **ограниченные байты** валидированного ответа: allowlist
//! форматов и декодирование живут в presentation-слое
//! (`crate::ui::images::decode_favicon`), поэтому `services` не знает ни `egui`,
//! ни пикселей. [`ArtworkFetcher::fetch`] исполняется только из фоновой задачи —
//! здесь сеть и резолв имён.

use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use reqwest::header::{CONTENT_LENGTH, CONTENT_TYPE, LOCATION};
use reqwest::{Client, ClientBuilder, Response, redirect};
use tracing::{debug, warn};
use url::{Host, Url};

use super::USER_AGENT;
use super::ssrf::{self, PolicyReject};

/// Максимальный принятый ответ, байт (ADR-0025, п. 2).
const MAX_ARTWORK_BYTES: usize = 512 * 1024;
/// Максимум редиректов на одной ссылке (ADR-0025, п. 10).
const MAX_ARTWORK_REDIRECTS: usize = 3;
/// Бюджет одного HTTP-обмена.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(6);
/// Бюджет резолва имени.
const DNS_TIMEOUT: Duration = Duration::from_secs(3);
/// Wall-clock бюджет всего цикла хоппов (ADR-0025, п. 4).
const TOTAL_TIMEOUT: Duration = Duration::from_secs(6);
/// Потолок припинованных клиентов (ADR-0025, п. 10).
const MAX_PINNED_CLIENTS: usize = 16;
/// Срок жизни припинованного клиента: по истечении хост резолвится заново,
/// поэтому rebinding-хост нельзя припинить навсегда.
const CLIENT_TTL: Duration = Duration::from_secs(60);

/// Допустимые `Content-Type` (ADR-0025, п. 4). Отсутствие заголовка — не отказ:
/// решает сигнатура в декодере.
const ALLOWED_MIME: [&str; 4] = [
    "image/png",
    "image/jpeg",
    "image/webp",
    "application/octet-stream",
];

/// Причина отказа — для `tracing` и диагностики; новых строк i18n нет.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FetchCause {
    /// Ссылка не парсится.
    Unparseable,
    /// Ярус 1 или адресная проверка яруса 2 отклонили цель.
    Policy(PolicyReject),
    /// Имя не разрешилось ни в один адрес.
    NoAddresses,
    /// Резолв не уложился в бюджет.
    DnsTimeout,
    /// Транспортная ошибка HTTP-клиента.
    Transport,
    /// Не 2xx, либо редирект без `Location`.
    Status,
    /// Бюджет редиректов исчерпан.
    RedirectLimit,
    /// `Content-Type` вне allowlist.
    Mime,
    /// Ответ больше [`MAX_ARTWORK_BYTES`].
    TooLarge,
    /// Цикл хоппов не уложился в wall-clock бюджет.
    Timeout,
}

impl std::fmt::Display for FetchCause {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let label = match self {
            Self::Unparseable => "ссылка не разобрана",
            Self::Policy(reject) => return write!(f, "политика цели: {reject}"),
            Self::NoAddresses => "имя не разрешилось",
            Self::DnsTimeout => "резолв превысил бюджет",
            Self::Transport => "транспортная ошибка",
            Self::Status => "не 2xx",
            Self::RedirectLimit => "слишком много редиректов",
            Self::Mime => "недопустимый Content-Type",
            Self::TooLarge => "ответ превышает лимит",
            Self::Timeout => "превышен общий бюджет",
        };
        f.write_str(label)
    }
}

/// Настройки политики цели.
#[derive(Debug, Default)]
pub(crate) struct Policy {
    /// Разрешить `http` как конечную цель, включая понижающий редирект.
    /// По умолчанию — только `https` (ADR-0025, п. 11); пользовательский opt-in
    /// `artwork_allow_http` переключает флаг на лету.
    allow_http: AtomicBool,
    /// Тестовый шов: разрешить приватные адреса, чтобы прогонять хопп-цикл по
    /// loopback-макету. Поле существует только в тестовой сборке — в релизном
    /// бинаре обойти политику из кода приложения нельзя.
    #[cfg(test)]
    allow_private_for_test: bool,
}

impl Policy {
    fn permits(&self, address: IpAddr) -> bool {
        #[cfg(test)]
        if self.allow_private_for_test {
            return true;
        }
        ssrf::is_public_destination(address)
    }

    fn allows_http(&self) -> bool {
        self.allow_http.load(Ordering::Relaxed)
    }
}

/// Фетчер артворка: политика + кэш припинованных клиентов. `Clone` дешёвый
/// (только `Arc`), чтобы фоновые задачи не одалживали оркестратор.
#[derive(Clone)]
pub(crate) struct ArtworkFetcher {
    policy: Arc<Policy>,
    clients: Arc<Mutex<HashMap<String, PinnedClient>>>,
}

struct PinnedClient {
    client: Client,
    pinned_at: Instant,
    used_at: Instant,
}

/// Результат одного хоппа.
enum Hop {
    Bytes(Vec<u8>),
    Redirect(String),
}

impl Default for ArtworkFetcher {
    fn default() -> Self {
        Self::new()
    }
}

impl ArtworkFetcher {
    /// Продакшн-фетчер: HTTPS-only, приватные цели отвергаются.
    pub(crate) fn new() -> Self {
        Self::with_policy(Policy::default())
    }

    /// Фетчер с пользовательским opt-in HTTP-обложек (ADR-0025, п. 11).
    pub(crate) fn with_allow_http(allow_http: bool) -> Self {
        Self::with_policy(Policy {
            allow_http: AtomicBool::new(allow_http),
            #[cfg(test)]
            allow_private_for_test: false,
        })
    }

    fn with_policy(policy: Policy) -> Self {
        Self {
            policy: Arc::new(policy),
            clients: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Переключает opt-in на лету: фоновые задачи делят `Arc<Policy>`, поэтому
    /// новые хоппы видят новое решение без пересоздания фетчера.
    pub(crate) fn set_allow_http(&self, allow_http: bool) {
        self.policy.allow_http.store(allow_http, Ordering::Relaxed);
    }

    /// Загружает байты favicon по ссылке станции, перепроверяя цель на каждом
    /// хоппе. Наружу — байты валидированного ответа или причина отказа; каждый
    /// отказ стоит одной строкой в логе (ADR-0025, п. 7) — иначе «инициалы вместо
    /// иконки» неразличимы между политикой, таймаутом и мусорным ответом.
    pub(crate) async fn fetch(&self, reference: &str) -> Result<Vec<u8>, FetchCause> {
        let parsed = Url::parse(reference);
        let outcome = match &parsed {
            Ok(url) => self.attempt(url).await,
            Err(_) => Err(FetchCause::Unparseable),
        };
        if let Err(cause) = &outcome {
            let host = parsed
                .as_ref()
                .ok()
                .and_then(|url| url.host_str())
                .unwrap_or("-");
            match cause {
                FetchCause::Transport | FetchCause::DnsTimeout | FetchCause::Timeout => {
                    warn!(host, %cause, "artwork: загрузка не состоялась");
                }
                _ => debug!(host, %cause, "artwork: загрузка отклонена"),
            }
        }
        outcome
    }

    /// Хопп-цикл: ярус 1 на точке входа и на каждом `Location`, адресная
    /// проверка яруса 2 на каждом резолве.
    async fn attempt(&self, url: &Url) -> Result<Vec<u8>, FetchCause> {
        let mut current = self.entry_point(url)?;
        let started = Instant::now();
        let mut hops = 0;

        loop {
            let Some(remaining) = TOTAL_TIMEOUT.checked_sub(started.elapsed()) else {
                return Err(FetchCause::Timeout);
            };
            match self.step(&current, remaining).await? {
                Hop::Bytes(bytes) => return Ok(bytes),
                Hop::Redirect(location) => {
                    if hops >= MAX_ARTWORK_REDIRECTS {
                        return Err(FetchCause::RedirectLimit);
                    }
                    hops += 1;
                    current = self.follow(&current, &location)?;
                }
            }
        }
    }

    /// Ярус 1 плюс беззвучный `http → https` при HTTPS-only политике
    /// (семантика `ClientBuilder::https_only(true)`); с opt-in схема
    /// сохраняется.
    fn entry_point(&self, url: &Url) -> Result<Url, FetchCause> {
        let mut url = self.validate(url)?;
        if url.scheme() == "http" && !self.policy.allows_http() {
            url.set_scheme("https")
                .map_err(|_| FetchCause::Policy(PolicyReject::UnsupportedScheme))?;
        }
        Ok(url)
    }

    /// Ярус 1 для хоппа. Тестовый шов расслабляет здесь ровно один предикат —
    /// адрес literal'а, чтобы макет жил на loopback; схемные, доменные и
    /// userinfo-правила продолжают действовать на каждом хоппе, а в релизной
    /// сборке поля шва не существует.
    fn validate(&self, url: &Url) -> Result<Url, FetchCause> {
        match ssrf::classify(url) {
            Ok(checked) => Ok(checked),
            Err(reject) => {
                #[cfg(test)]
                if self.policy.allow_private_for_test
                    && matches!(reject, PolicyReject::PrivateAddress)
                {
                    return Ok(url.clone());
                }
                Err(FetchCause::Policy(reject))
            }
        }
    }

    /// Один хопп: хопповая валидация → резолв → адресная проверка → запрос.
    /// Таймаут общий на весь обмен: медленное тело не должно пережить бюджет
    /// (ADR-0025, п. 2).
    async fn step(&self, url: &Url, remaining: Duration) -> Result<Hop, FetchCause> {
        let addresses = self.resolve(url, remaining).await?;
        let client = self.client_for(url, &addresses)?;
        let timeout = REQUEST_TIMEOUT.min(remaining);
        tokio::time::timeout(timeout, async {
            let response = client
                .get(url.clone())
                .send()
                .await
                .map_err(|_| FetchCause::Transport)?;
            inspect(response).await
        })
        .await
        .map_err(|_| FetchCause::Timeout)?
    }

    /// Редирект принимается только если новый URL проходит ярус 1; при
    /// HTTPS-only политике понижение протокола отклоняется, а с opt-in
    /// `http` разрешён наравне с точкой входа.
    fn follow(&self, from: &Url, location: &str) -> Result<Url, FetchCause> {
        let joined = from.join(location).map_err(|_| FetchCause::Unparseable)?;
        let url = self.validate(&joined)?;
        if url.scheme() == "http" && !self.policy.allows_http() {
            return Err(FetchCause::Policy(PolicyReject::UnsupportedScheme));
        }
        Ok(url)
    }

    /// Резолвит цель и проверяет **все** полученные адреса: приватный хоть в
    /// одном ответе — отказ (all-or-nothing).
    async fn resolve(&self, url: &Url, remaining: Duration) -> Result<Vec<SocketAddr>, FetchCause> {
        let addresses = resolve_addresses(url, remaining).await?;
        gate(&self.policy, &addresses)?;
        Ok(addresses)
    }

    /// Клиент для цели: IP-literal обслуживается общим клиентом, домен —
    /// припинованным по всему проверенному набору адресов (ADR-0025, п. 10).
    fn client_for(&self, url: &Url, addresses: &[SocketAddr]) -> Result<Client, FetchCause> {
        let Some(Host::Domain(name)) = url.host() else {
            return shared_client();
        };
        let authority = format!("{name}:{}", addresses[0].port());
        let mut pool = lock(&self.clients);
        if let Some(entry) = pool.get_mut(&authority)
            && entry.pinned_at.elapsed() < CLIENT_TTL
        {
            entry.used_at = Instant::now();
            return Ok(entry.client.clone());
        }
        let client = builder()?
            .resolve_to_addrs(name, addresses)
            .build()
            .map_err(|_| FetchCause::Transport)?;
        if pool.len() >= MAX_PINNED_CLIENTS {
            evict(&mut pool);
        }
        pool.insert(
            authority,
            PinnedClient {
                client: client.clone(),
                pinned_at: Instant::now(),
                used_at: Instant::now(),
            },
        );
        Ok(client)
    }
}

/// All-or-nothing по всему набору резолва: приватный адрес хоть в одном ответе
/// отклоняет цель целиком (ADR-0025, п. 10). Отдельная функция потому, что
/// решение надо тестировать без DNS: `evil.com` отдает `[93.184.216.34, ::1]`.
fn gate(policy: &Policy, addresses: &[SocketAddr]) -> Result<(), FetchCause> {
    if addresses.iter().all(|address| policy.permits(address.ip())) {
        Ok(())
    } else {
        Err(FetchCause::Policy(PolicyReject::PrivateAddress))
    }
}

/// Резолвит цель в набор адресов с бюджетом `remaining`; IP-literal не ходит
/// в DNS. Общий шаг яруса 2 для фетчера и вердикта публикации.
async fn resolve_addresses(url: &Url, remaining: Duration) -> Result<Vec<SocketAddr>, FetchCause> {
    let port = url
        .port_or_known_default()
        .ok_or(FetchCause::Policy(PolicyReject::ZeroPort))?;
    match url.host() {
        Some(Host::Ipv4(ip)) => Ok(vec![SocketAddr::new(IpAddr::V4(ip), port)]),
        Some(Host::Ipv6(ip)) => Ok(vec![SocketAddr::new(IpAddr::V6(ip), port)]),
        Some(Host::Domain(name)) => {
            let authority = format!("{name}:{port}");
            let lookup = tokio::time::timeout(
                DNS_TIMEOUT.min(remaining),
                tokio::net::lookup_host(&authority),
            )
            .await
            .map_err(|_| FetchCause::DnsTimeout)?
            .map_err(|_| FetchCause::NoAddresses)?;
            let collected: Vec<SocketAddr> = lookup.collect();
            if collected.is_empty() {
                return Err(FetchCause::NoAddresses);
            }
            Ok(collected)
        }
        None => Err(FetchCause::Policy(PolicyReject::MissingHost)),
    }
}

/// Общий клиент для целей-литералов: редиректы не доверяем, UA — по ADR-0004.
fn shared_client() -> Result<Client, FetchCause> {
    builder()?.build().map_err(|_| FetchCause::Transport)
}

/// Системные прокси отключены: иначе хост-прокси резолвил бы имя сам и
/// обходил привязку адресов и SSRF-политику (ADR-0025).
fn builder() -> Result<ClientBuilder, FetchCause> {
    Ok(Client::builder()
        .user_agent(USER_AGENT)
        .no_proxy()
        .redirect(redirect::Policy::none()))
}

/// `Content-Type` из allowlist; отсутствие заголовка — не отказ, решает
/// сигнатура в декодере (ADR-0025, п. 4).
fn mime_allowed(value: Option<&str>) -> bool {
    let Some(value) = value else {
        return true;
    };
    let mime = value.split(';').next().unwrap_or_default().trim();
    ALLOWED_MIME
        .iter()
        .any(|allowed| mime.eq_ignore_ascii_case(allowed))
}

/// Разбирает ответ: редирект → `Location`, 2xx → байты в границах.
async fn inspect(response: Response) -> Result<Hop, FetchCause> {
    let status = response.status();
    if status.is_redirection() {
        let location = response
            .headers()
            .get(LOCATION)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned)
            .ok_or(FetchCause::Status)?;
        return Ok(Hop::Redirect(location));
    }
    if !status.is_success() {
        return Err(FetchCause::Status);
    }
    if !mime_allowed(
        response
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
    ) {
        return Err(FetchCause::Mime);
    }
    if response
        .headers()
        .get(CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<usize>().ok())
        .is_some_and(|length| length > MAX_ARTWORK_BYTES)
    {
        return Err(FetchCause::TooLarge);
    }
    read_bounded(response).await
}

/// Читает тело потоково и обрывает при превышении лимита (ADR-0025, п. 2):
/// враньё в `Content-Length` не должно приводить к неограниченной аллокации.
async fn read_bounded(mut response: Response) -> Result<Hop, FetchCause> {
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| FetchCause::Transport)? {
        if bytes.len() + chunk.len() > MAX_ARTWORK_BYTES {
            return Err(FetchCause::TooLarge);
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(Hop::Bytes(bytes))
}

fn lock<T>(value: &Mutex<T>) -> MutexGuard<'_, T> {
    value.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Вытеснение: сначала протухшие, потом давно не использовавшийся (LRU).
fn evict(pool: &mut HashMap<String, PinnedClient>) {
    if let Some(expired) = pool
        .iter()
        .find(|(_, entry)| entry.pinned_at.elapsed() >= CLIENT_TTL)
        .map(|(authority, _)| authority.clone())
    {
        pool.remove(&expired);
        return;
    }
    let victim = pool
        .iter()
        .min_by_key(|(_, entry)| entry.used_at)
        .map(|(authority, _)| authority.clone());
    if let Some(authority) = victim {
        pool.remove(&authority);
    }
}

/// Гейт публикации метаданных (ADR-0025, п. 12): в MPRIS/SMTC/Now Playing
/// уходит только ссылка, прошедшая ярус 1. Приватный literal не публикуется
/// никогда — оболочка сама ходит по `artUrl`. Строку не переписываем: нормализация
/// меняла бы то, что видит DE-фетчер.
pub(crate) fn publishable(reference: &str) -> Option<&str> {
    ssrf::parse_publishable(reference).map(|_| reference)
}

/// Асинхронный вердикт яруса 2 для строки, публикуемой наружу (ADR-0025,
/// п. 12): ярус 1, затем резолв и та же all-or-nothing проверка адресов, что
/// у фетчера. `false` — ссылку нельзя отдавать DE-фетчеру: он пошёл бы по
/// приватному адресу, который мы сами никогда не запросим. Байты не читаются,
/// соединение не открывается.
pub(crate) async fn public_verdict(reference: &str) -> bool {
    let Ok(url) = Url::parse(reference) else {
        return false;
    };
    if ssrf::classify(&url).is_err() {
        return false;
    }
    match resolve_addresses(&url, DNS_TIMEOUT).await {
        Ok(addresses) => addresses
            .iter()
            .all(|address| ssrf::is_public_destination(address.ip())),
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    async fn respond(listener: tokio::net::TcpListener, replies: Vec<String>) {
        for reply in replies {
            let Ok((mut socket, _)) = listener.accept().await else {
                return;
            };
            let mut probe = [0u8; 1024];
            let _ = socket.read(&mut probe).await;
            let _ = socket.write_all(reply.as_bytes()).await;
            let _ = socket.shutdown().await;
        }
    }

    fn ok_reply(content_type: Option<&str>, body: &str) -> String {
        let typed = content_type
            .map(|value| format!("Content-Type: {value}\r\n"))
            .unwrap_or_default();
        format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n{typed}Connection: close\r\n\r\n{body}",
            body.len()
        )
    }

    fn redirect_reply(location: &str) -> String {
        format!(
            "HTTP/1.1 302 Found\r\nLocation: {location}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
        )
    }

    /// Макет HTTP-сервера: отдаёт заготовленные ответы по одному соединению на
    /// хопп. Собственный `TcpListener` вместо зависимости от `hyper`.
    /// Бюджет нужен на случай раннего отказа фетчера: без него неиспользованный
    /// `accept()` вешал бы тест, а не ронял бы его.
    async fn mock(replies: Vec<String>) -> (String, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let handle = tokio::spawn(async move {
            let budget = Duration::from_secs(10);
            let _ = tokio::time::timeout(budget, respond(listener, replies)).await;
        });
        (format!("http://{address}/icon.png"), handle)
    }

    fn test_policy() -> Policy {
        Policy {
            allow_http: AtomicBool::new(true),
            allow_private_for_test: true,
        }
    }

    #[tokio::test]
    async fn fetch_returns_bounded_body_from_allowed_mime() {
        let (url, server) = mock(vec![ok_reply(Some("image/png"), "--body--")]).await;
        let outcome = ArtworkFetcher::with_policy(test_policy()).fetch(&url).await;
        server.await.unwrap();
        assert_eq!(outcome.as_deref(), Ok("--body--".as_bytes()));
    }

    #[tokio::test]
    async fn production_policy_refuses_private_literal_without_connecting() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let fetcher = ArtworkFetcher::new();
        let cause = fetcher
            .fetch(&format!("http://{address}/icon.png"))
            .await
            .expect_err("приватный literal отклоняется");
        assert_eq!(cause, FetchCause::Policy(PolicyReject::PrivateAddress));
        // Ни одного соединения: отказ случился до транспорта.
        assert!(
            tokio::time::timeout(Duration::from_millis(50), listener.accept())
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn fetch_revalidates_every_redirect_hop() {
        let (first, server) = mock(vec![
            redirect_reply("/next.png"),
            ok_reply(Some("image/png"), "--body--"),
        ])
        .await;
        let fetcher = ArtworkFetcher::with_policy(test_policy());
        let bytes = fetcher.fetch(&first).await.expect("хоппы валидны");
        server.await.unwrap();
        assert_eq!(bytes, b"--body--");
    }

    #[tokio::test]
    async fn fetch_stops_at_redirect_budget() {
        let (url, server) = mock(vec![
            redirect_reply("/a"),
            redirect_reply("/b"),
            redirect_reply("/c"),
            redirect_reply("/d"),
        ])
        .await;
        let fetcher = ArtworkFetcher::with_policy(test_policy());
        assert_eq!(
            fetcher.fetch(&url).await.unwrap_err(),
            FetchCause::RedirectLimit
        );
        server.await.unwrap();
    }

    #[tokio::test]
    async fn fetch_rejects_non_image_mime_and_accepts_absent_header() {
        let (url, server) = mock(vec![ok_reply(
            Some("text/html; charset=utf-8"),
            "<html><script>alert(1)</script></html>",
        )])
        .await;
        let fetcher = ArtworkFetcher::with_policy(test_policy());
        assert_eq!(fetcher.fetch(&url).await.unwrap_err(), FetchCause::Mime);
        server.await.unwrap();

        let (url, server) = mock(vec![ok_reply(None, "--body--")]).await;
        let fetcher = ArtworkFetcher::with_policy(test_policy());
        assert_eq!(
            fetcher.fetch(&url).await.as_deref(),
            Ok("--body--".as_bytes())
        );
        server.await.unwrap();
    }

    #[tokio::test]
    async fn fetch_truncates_body_beyond_content_length_claim() {
        let body = "A".repeat(MAX_ARTWORK_BYTES + 1024);
        let reply = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: image/png\r\nConnection: close\r\n\r\n{body}"
        );
        let (url, server) = mock(vec![reply]).await;
        let fetcher = ArtworkFetcher::with_policy(test_policy());
        assert_eq!(fetcher.fetch(&url).await.unwrap_err(), FetchCause::TooLarge);
        server.await.unwrap();
    }

    #[tokio::test]
    async fn redirect_without_location_is_status_error() {
        let (url, server) = mock(vec![
            "HTTP/1.1 301 Moved\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_owned(),
        ])
        .await;
        let fetcher = ArtworkFetcher::with_policy(test_policy());
        assert_eq!(fetcher.fetch(&url).await.unwrap_err(), FetchCause::Status);
        server.await.unwrap();
    }

    #[tokio::test]
    async fn unparseable_reference_never_reaches_transport() {
        assert_eq!(
            ArtworkFetcher::with_policy(test_policy())
                .fetch("не url")
                .await
                .unwrap_err(),
            FetchCause::Unparseable
        );
    }

    /// Класс dns-rebinding: имя отдаёт публичный адрес **и** приватный — отказ
    /// всему набору, даже если первый ответ выглядит невинно. Тестируется без
    /// DNS, потому что решение — чистая функция по уже разрешённым адресам.
    #[test]
    fn resolved_address_set_is_all_or_nothing() {
        let production = Policy::default();
        let public: SocketAddr = "93.184.216.34:443".parse().unwrap();
        let hostile: Vec<SocketAddr> = [
            "127.0.0.1:443",
            "169.254.169.254:80",
            "100.64.0.1:80",
            "10.0.0.5:9999",
            "192.168.0.1:8080",
            "[::1]:443",
            "[fd00::5]:443",
            "[fe80::1]:443",
            "[::ffff:127.0.0.1]:443",
        ]
        .iter()
        .map(|literal| literal.parse().expect("адрес-литерал корректен"))
        .collect();
        assert!(gate(&production, &[public]).is_ok());
        for address in hostile {
            let cause = gate(&production, &[public, address]).unwrap_err();
            assert_eq!(
                cause,
                FetchCause::Policy(PolicyReject::PrivateAddress),
                "{address}"
            );
        }
    }

    /// Тестовый шов расслабляет ровно адресный предикат — иначе макет на
    /// loopback не проходит ярус 1 и хопп-цикл вообще не проверяется. Все
    /// остальные правила хоппа остаются в силе.
    #[test]
    fn test_seam_relaxes_only_the_address_predicate() {
        let fetcher = ArtworkFetcher::with_policy(test_policy());
        let from = Url::parse("https://cdn.example.com/i.png").unwrap();
        assert!(fetcher.follow(&from, "https://127.0.0.1:9/x").is_ok());
        assert!(fetcher.follow(&from, "file:///etc/passwd").is_err());
        assert!(fetcher.follow(&from, "//router/private").is_err());
        assert!(fetcher.follow(&from, "https://localhost/x").is_err());
        assert!(fetcher.follow(&from, "https://x.internal/l").is_err());
        assert!(
            fetcher
                .follow(&from, "https://127.0.0.1@cdn.example/x")
                .is_err()
        );
        // Без шва приватный literal отклоняется и в хоппе.
        let strict = ArtworkFetcher::new();
        assert_eq!(
            strict.follow(&from, "https://127.0.0.1:9/x").unwrap_err(),
            FetchCause::Policy(PolicyReject::PrivateAddress)
        );
    }

    #[test]
    fn https_only_policy_upgrades_plaintext_entry_point() {
        let fetcher = ArtworkFetcher::new();
        let upgraded = fetcher
            .entry_point(&Url::parse("http://cdn.example.com/i.png").unwrap())
            .expect("публичный http поднимается до https");
        assert_eq!(upgraded.scheme(), "https");
        assert_eq!(upgraded.host_str(), Some("cdn.example.com"));

        let strict = ArtworkFetcher::with_policy(Policy {
            allow_http: AtomicBool::new(true),
            ..Policy::default()
        });
        let kept = strict
            .entry_point(&Url::parse("http://cdn.example.com/i.png").unwrap())
            .unwrap();
        assert_eq!(kept.scheme(), "http");
    }

    /// Пользовательский opt-in переключается без пересоздания фетчера: фоновые
    /// задачи делят `Arc<Policy>`, поэтому новый хопп видит новый режим.
    #[test]
    fn opt_in_http_switches_at_runtime() {
        let fetcher = ArtworkFetcher::new();
        let endpoint = Url::parse("http://cdn.example.com/i.png").unwrap();
        assert_eq!(fetcher.entry_point(&endpoint).unwrap().scheme(), "https");

        fetcher.set_allow_http(true);
        assert_eq!(fetcher.entry_point(&endpoint).unwrap().scheme(), "http");
        let from = Url::parse("https://cdn.example.com/i.png").unwrap();
        assert!(
            fetcher
                .follow(&from, "http://cdn.example.com/j.png")
                .is_ok(),
            "с opt-in понижающий редирект разрешён"
        );

        fetcher.set_allow_http(false);
        assert_eq!(fetcher.entry_point(&endpoint).unwrap().scheme(), "https");
        assert_eq!(
            fetcher
                .follow(&from, "http://cdn.example.com/j.png")
                .unwrap_err(),
            FetchCause::Policy(PolicyReject::UnsupportedScheme)
        );
    }

    /// Вердикт публикации не открывает соединений: публичный literal решается
    /// локально, приватный и синтаксически негодный отклоняются ярусом 1.
    #[tokio::test]
    async fn public_verdict_decides_without_touching_the_network() {
        assert!(public_verdict("https://93.184.216.34/icon.png").await);
        assert!(public_verdict("http://93.184.216.34:8080/icon.png").await);
        assert!(!public_verdict("http://127.0.0.1/icon.png").await);
        assert!(!public_verdict("https://10.0.0.5/icon.png").await);
        assert!(!public_verdict("https://169.254.169.254/latest/meta-data/").await);
        assert!(!public_verdict("https://localhost/icon.png").await);
        assert!(!public_verdict("file:///etc/passwd").await);
        assert!(!public_verdict("не url").await);
        assert!(!public_verdict("").await);
    }

    #[test]
    fn downgrade_redirect_is_refused_even_when_host_is_public() {
        let fetcher = ArtworkFetcher::new();
        let from = Url::parse("https://cdn.example.com/i.png").unwrap();
        assert_eq!(
            fetcher
                .follow(&from, "http://cdn.example.com/j.png")
                .unwrap_err(),
            FetchCause::Policy(PolicyReject::UnsupportedScheme)
        );
        assert!(fetcher.follow(&from, "/k.png").is_ok());
        assert_eq!(
            fetcher
                .follow(&from, "https://169.254.169.254/")
                .unwrap_err(),
            FetchCause::Policy(PolicyReject::PrivateAddress)
        );
        assert!(fetcher.follow(&from, "//router/private").is_err());
        assert!(fetcher.follow(&from, "file:///etc/passwd").is_err());
        assert_eq!(
            fetcher.follow(&from, "http://").unwrap_err(),
            FetchCause::Unparseable
        );
    }

    #[test]
    fn pinned_client_pool_stays_capped() {
        let mut pool: HashMap<String, PinnedClient> = HashMap::new();
        let client = shared_client().unwrap();
        for index in 0..MAX_PINNED_CLIENTS {
            pool.insert(
                format!("h{index}:443"),
                PinnedClient {
                    client: client.clone(),
                    pinned_at: Instant::now(),
                    used_at: Instant::now(),
                },
            );
        }
        assert_eq!(pool.len(), MAX_PINNED_CLIENTS);
        evict(&mut pool);
        assert_eq!(pool.len(), MAX_PINNED_CLIENTS - 1);
        assert!(!pool.contains_key("h0:443") || !pool.contains_key("h15:443"));
    }

    #[test]
    fn mime_allowlist_rejects_active_content() {
        assert!(mime_allowed(Some("image/png")));
        assert!(mime_allowed(Some("IMAGE/JPEG; charset=binary")));
        assert!(mime_allowed(Some("image/webp")));
        assert!(mime_allowed(Some("application/octet-stream")));
        assert!(mime_allowed(None), "отсутствие заголовка решает сигнатуру");
        assert!(!mime_allowed(Some("image/svg+xml")));
        assert!(!mime_allowed(Some("image/gif")));
        assert!(!mime_allowed(Some("image/x-icon")));
        assert!(!mime_allowed(Some("text/html")));
        assert!(!mime_allowed(Some("")));
    }

    #[test]
    fn publishable_gate_passes_public_and_drops_private() {
        assert_eq!(
            publishable("https://cdn.example.com/i.png"),
            Some("https://cdn.example.com/i.png")
        );
        assert_eq!(publishable("http://127.1/x"), None);
        assert_eq!(
            publishable("http://169.254.169.254/latest/meta-data/"),
            None
        );
        assert_eq!(publishable("http://localhost:8080/x"), None);
        assert_eq!(publishable("не url"), None);
        assert_eq!(publishable(""), None);
    }

    /// Замер покрытия обложек на живом каталоге перед релизом (S7): политика
    /// HTTPS-only может оставить заметную долю станций с инициалами, и это
    /// решение про UX, а не про код. Печатает раскладку по причинам отказа,
    /// ничего не утверждает по значению — сеть здесь не детерминирована.
    #[tokio::test]
    #[ignore = "требует сеть и живой Radio Browser; запускайте с --ignored --nocapture"]
    async fn measures_live_artwork_coverage() {
        use super::super::radio_browser::{RadioBrowserClient, SearchParams};
        use std::collections::BTreeMap;

        let client = RadioBrowserClient::new();
        let stations = client
            .search(SearchParams {
                limit: 300,
                hide_broken: true,
                ..SearchParams::default()
            })
            .await
            .expect("Radio Browser недоступен");
        assert!(!stations.is_empty(), "каталог вернул пустую выборку");

        let fetcher = ArtworkFetcher::new();
        let mut causes: BTreeMap<String, usize> = BTreeMap::new();
        let mut http_origin: BTreeMap<String, usize> = BTreeMap::new();
        // По 8 загрузок параллельно: столько же, сколько in_flight в UI-кэше,
        // чтобы замер не отличался от боевого профиля.
        for batch in stations.chunks(8) {
            let mut tasks = tokio::task::JoinSet::new();
            for station in batch {
                let fetcher = fetcher.clone();
                let reference = station.favicon.clone().unwrap_or_default();
                tasks.spawn(async move {
                    let was_http = reference.starts_with("http://");
                    let cause = if reference.trim().is_empty() {
                        "нет ссылки".to_owned()
                    } else {
                        match fetcher.fetch(&reference).await {
                            Ok(bytes) if bytes.is_empty() => "пустой ответ".to_owned(),
                            Ok(_) => "загружено".to_owned(),
                            Err(cause) => cause.to_string(),
                        }
                    };
                    (was_http, cause)
                });
            }
            while let Some(outcome) = tasks.join_next().await {
                let (was_http, cause) = outcome.expect("задача не должна паниковать");
                *causes.entry(cause.clone()).or_default() += 1;
                if was_http {
                    *http_origin.entry(cause).or_default() += 1;
                }
            }
        }

        let served = causes.get("загружено").copied().unwrap_or_default();
        println!("выборка: {} станций", stations.len());
        for (cause, count) in &causes {
            println!("  {cause}: {count}");
        }
        println!(
            "покрытие обложек: {served}/{} ({:.1}%)",
            stations.len(),
            100.0 * served as f64 / stations.len() as f64
        );
        // Отдельно: ссылки, которые политика подняла с http на https. Это и
        // есть цена HTTPS-only; opt-in `artwork_allow_http` возвращает ровно
        // эту долю покрытия ценой доверия байтам, пришедшим по 80 порту.
        if !http_origin.is_empty() {
            println!("изначально http (подняты до https):");
            for (cause, count) in &http_origin {
                println!("  {cause}: {count}");
            }
        }
    }
}
