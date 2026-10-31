//! Резолвер внешних потоков для `play-url` (ADR-0040).
//!
//! Пользователь вставляет адрес страницы, плейлиста или файла, а модуль
//! доводит его до воспроизводимого `http(s)`-адреса: разбирает `.m3u`/`.pls`,
//! вытаскивает `og:audio`/`<audio>`/`<source>` из HTML. HLS не поддерживается
//! mpv-конфигурацией OrbitFM и отклоняется до воспроизведения.
//!
//! LAN разрешён, потому что команду запускает явное действие пользователя:
//! если исходная цель локальная, строгая политика не включается. Но публичная
//! цель не может увести импорт во внутреннюю сеть редиректом, записью
//! плейлиста или `og:audio` — для неё каждый следующий хопп проверяется как
//! ярус 2 обложек (ADR-0040, ред. 2).

use std::collections::{HashMap, HashSet};
use std::net::{IpAddr, SocketAddr};
use std::time::{Duration, Instant};

use reqwest::header::{CONTENT_LENGTH, CONTENT_TYPE, LOCATION};
use reqwest::{Client, Response, redirect};
use url::{Host, Url};

use super::USER_AGENT;
use super::ssrf;

/// Общий бюджет резолва: сервер отвечает клиенту за 10 с, диалог — за 15 с.
pub(crate) const RESOLVE_BUDGET: Duration = Duration::from_secs(8);
/// Бюджет одного HTTP-обмена внутри общего.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);
/// Потолок тела плейлиста или HTML-страницы (ADR-0040).
const MAX_BODY_BYTES: usize = 1024 * 1024;
/// Максимум редиректов на одном адресе.
const MAX_REDIRECTS: usize = 5;
/// Уровни вложенности плейлистов: внешний и один вложенный.
const MAX_PLAYLIST_LEVELS: usize = 2;
/// HTML-страниц в цепочке `og:audio` → страница → поток.
const MAX_PAGE_HOPS: usize = 3;
/// Записей, которые просматриваются в плейлисте.
const MAX_PLAYLIST_ENTRIES: usize = 20;
/// Бюджет DNS-проверки хоппа в строгом режиме.
const DNS_TIMEOUT: Duration = Duration::from_secs(3);

/// Разрешённый внешний поток: воспроизводимый адрес и метаданные.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResolvedStream {
    /// Конечный адрес, который получит mpv.
    pub(crate) url: String,
    /// Название из `#EXTINF`, `TitleN` или `og:title`/`<title>`.
    pub(crate) title: Option<String>,
    /// Исходная страница/плейлист, указанные пользователем.
    pub(crate) homepage: Option<String>,
}

/// Ошибки резолва: технические подробности не переводятся, но сообщение
/// показывается пользователю в попапе и CLI.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub(crate) enum StreamError {
    #[error("некорректный URL")]
    InvalidUrl,
    #[error("поддерживаются только http:// и https://")]
    UnsupportedScheme,
    #[error("HLS-потоки (.m3u8) не поддерживаются")]
    Hls,
    #[error("страница или плейлист больше 1 МиБ")]
    TooLarge,
    #[error("аудиопоток не найден")]
    NotAudio,
    #[error("HTTP {0}")]
    Http(u16),
    #[error("сеть: {0}")]
    Network(String),
    #[error("превышен таймаут импорта")]
    Timeout,
    #[error("плейлист не содержит воспроизводимых ссылок")]
    EmptyPlaylist,
    #[error("слишком много редиректов")]
    RedirectLimit,
    #[error("слишком много уровней плейлиста")]
    PlaylistDepth,
    #[error("цикл в плейлисте или странице")]
    PlaylistCycle,
    #[error("ссылка ведёт во внутреннюю сеть")]
    PrivateTarget,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Hls,
    Playlist,
    Html,
    Audio,
    Unknown,
}

/// Резолвит адрес с общим бюджетом [`RESOLVE_BUDGET`].
pub(crate) async fn resolve(input: &str) -> Result<ResolvedStream, StreamError> {
    tokio::time::timeout(RESOLVE_BUDGET, resolve_inner(input))
        .await
        .map_err(|_| StreamError::Timeout)?
}

async fn resolve_inner(input: &str) -> Result<ResolvedStream, StreamError> {
    let homepage = input.trim();
    let mut current = Url::parse(homepage).map_err(|_| StreamError::InvalidUrl)?;
    validate(&current)?;

    let client = Client::builder()
        .user_agent(USER_AGENT)
        .no_proxy()
        .redirect(redirect::Policy::none())
        .timeout(REQUEST_TIMEOUT)
        .build()
        .map_err(|err| StreamError::Network(err.to_string()))?;
    // Публичная цель — строгий режим: все последующие хоппы обязаны остаться
    // публичными. Локальная цель (явный LAN-адрес) строгого режима не включает.
    let strict = ensure_public(&current).await.is_ok();
    let started = Instant::now();
    let mut title = None;
    let mut redirects = 0;
    let mut playlists = 0;
    let mut pages = 0;
    let mut visited = HashSet::new();

    loop {
        let remaining = RESOLVE_BUDGET
            .checked_sub(started.elapsed())
            .filter(|remaining| !remaining.is_zero())
            .ok_or(StreamError::Timeout)?;
        if remaining < Duration::from_millis(50) {
            return Err(StreamError::Timeout);
        }
        if !visited.insert(current.to_string()) {
            return Err(StreamError::PlaylistCycle);
        }

        let (final_url, response) =
            fetch_following(&client, current.clone(), &mut redirects, strict).await?;
        validate(&final_url)?;
        let content_type = header(&response, CONTENT_TYPE);
        let kind = classify(content_type.as_deref(), &final_url);

        match kind {
            Kind::Hls => return Err(StreamError::Hls),
            Kind::Playlist => {
                if playlists >= MAX_PLAYLIST_LEVELS {
                    return Err(StreamError::PlaylistDepth);
                }
                playlists += 1;
                let body = read_bounded(response).await?;
                let text = String::from_utf8_lossy(&body);
                if text.contains("#EXT-X-") {
                    return Err(StreamError::Hls);
                }
                let entries = if looks_like_pls(&text) {
                    parse_pls(&text)?
                } else {
                    parse_m3u(&text)?
                };
                let (entry_title, entry) = first_playable(&final_url, entries)?;
                if title.is_none() {
                    title = entry_title;
                }
                current = entry;
            }
            Kind::Html => {
                if pages >= MAX_PAGE_HOPS {
                    return Err(StreamError::NotAudio);
                }
                pages += 1;
                let body = read_bounded(response).await?;
                let text = String::from_utf8_lossy(&body);
                if text.contains("#EXT-X-") {
                    return Err(StreamError::Hls);
                }
                if has_hls_alternate(&text) {
                    return Err(StreamError::Hls);
                }
                if title.is_none() {
                    title = meta_content(&text, "og:title").or_else(|| html_title(&text));
                }
                let candidate = meta_content(&text, "og:audio")
                    .or_else(|| tag_attr(&text, "audio", "src"))
                    .or_else(|| tag_attr(&text, "source", "src"))
                    .ok_or(StreamError::NotAudio)?;
                current = join(&final_url, &candidate)?;
            }
            Kind::Audio => {
                return Ok(ResolvedStream {
                    url: final_url.to_string(),
                    title,
                    homepage: Some(homepage.to_owned()),
                });
            }
            Kind::Unknown => {
                let body = read_bounded(response).await?;
                let text = String::from_utf8_lossy(&body);
                if text.contains("#EXT-X-") {
                    return Err(StreamError::Hls);
                }
                if looks_like_playlist(&text) {
                    if playlists >= MAX_PLAYLIST_LEVELS {
                        return Err(StreamError::PlaylistDepth);
                    }
                    playlists += 1;
                    let entries = if looks_like_pls(&text) {
                        parse_pls(&text)?
                    } else {
                        parse_m3u(&text)?
                    };
                    let (entry_title, entry) = first_playable(&final_url, entries)?;
                    if title.is_none() {
                        title = entry_title;
                    }
                    current = entry;
                } else if looks_like_html(&text) {
                    if pages >= MAX_PAGE_HOPS {
                        return Err(StreamError::NotAudio);
                    }
                    pages += 1;
                    if has_hls_alternate(&text) {
                        return Err(StreamError::Hls);
                    }
                    if title.is_none() {
                        title = meta_content(&text, "og:title").or_else(|| html_title(&text));
                    }
                    let candidate = meta_content(&text, "og:audio")
                        .or_else(|| tag_attr(&text, "audio", "src"))
                        .or_else(|| tag_attr(&text, "source", "src"));
                    match candidate {
                        Some(candidate) => current = join(&final_url, &candidate)?,
                        None => return Err(StreamError::NotAudio),
                    }
                } else {
                    return Err(StreamError::NotAudio);
                }
            }
        }
    }
}

/// Схема, host и отсутствие userinfo проверяются на каждом хоппе: редирект не
/// должен уводить импорт в `file://` или прятать логин в URL (ADR-0040).
fn validate(url: &Url) -> Result<(), StreamError> {
    match url.scheme() {
        "http" | "https" => {}
        _ => return Err(StreamError::UnsupportedScheme),
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(StreamError::InvalidUrl);
    }
    if url.host_str().is_none_or(str::is_empty) {
        return Err(StreamError::InvalidUrl);
    }
    if extension(url).as_deref() == Some("m3u8") {
        return Err(StreamError::Hls);
    }
    Ok(())
}

/// Ручной цикл редиректов: `reqwest` не следует `Location` сам, чтобы каждый
/// новый адрес прошёл [`validate`]. В строгом режиме перед запросом
/// проверяется и адресное назначение — публичная страница не должна
/// редиректить импорт на loopback или в LAN (ADR-0040, ред. 2).
async fn fetch_following(
    client: &Client,
    mut url: Url,
    redirects: &mut usize,
    strict: bool,
) -> Result<(Url, Response), StreamError> {
    loop {
        if strict {
            ensure_public(&url).await?;
        }
        let response = client
            .get(url.clone())
            .send()
            .await
            .map_err(|err| request_error(&err))?;
        let status = response.status();
        if !status.is_redirection() {
            if status.is_success() {
                return Ok((url, response));
            }
            return Err(StreamError::Http(status.as_u16()));
        }
        let Some(location) = header(&response, LOCATION) else {
            return Err(StreamError::Http(status.as_u16()));
        };
        if *redirects >= MAX_REDIRECTS {
            return Err(StreamError::RedirectLimit);
        }
        *redirects += 1;
        let next = url.join(&location).map_err(|_| StreamError::InvalidUrl)?;
        validate(&next)?;
        url = next;
    }
}

/// Ярус 2 для строгого режима: резолвит хост и требует, чтобы **все** адреса
/// были публичными. IP-literal не ходит в DNS. Ошибка сети тоже закрывает
/// импорт: без резолва нельзя утверждать, что цель публична.
async fn ensure_public(url: &Url) -> Result<(), StreamError> {
    let port = url.port_or_known_default().ok_or(StreamError::InvalidUrl)?;
    let addresses: Vec<SocketAddr> = match url.host() {
        Some(Host::Ipv4(address)) => vec![SocketAddr::new(IpAddr::V4(address), port)],
        Some(Host::Ipv6(address)) => vec![SocketAddr::new(IpAddr::V6(address), port)],
        Some(Host::Domain(name)) => {
            let authority = format!("{name}:{port}");
            let lookup = tokio::time::timeout(DNS_TIMEOUT, tokio::net::lookup_host(&authority))
                .await
                .map_err(|_| StreamError::Timeout)?
                .map_err(|err| StreamError::Network(err.to_string()))?;
            lookup.collect()
        }
        None => return Err(StreamError::InvalidUrl),
    };
    if addresses.is_empty()
        || !addresses
            .iter()
            .all(|address| ssrf::is_public_destination(address.ip()))
    {
        return Err(StreamError::PrivateTarget);
    }
    Ok(())
}

fn request_error(err: &reqwest::Error) -> StreamError {
    if err.is_timeout() {
        StreamError::Timeout
    } else {
        StreamError::Network(err.to_string())
    }
}

fn header(response: &Response, name: reqwest::header::HeaderName) -> Option<String> {
    response
        .headers()
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned)
}

fn classify(content_type: Option<&str>, url: &Url) -> Kind {
    let mime = content_type.map(|value| {
        value
            .split(';')
            .next()
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase()
    });
    let ext = extension(url);

    if ext.as_deref() == Some("m3u8") || mime.as_deref() == Some("application/vnd.apple.mpegurl") {
        return Kind::Hls;
    }
    if matches!(ext.as_deref(), Some("m3u" | "pls")) {
        return Kind::Playlist;
    }
    if let Some(mime) = mime.as_deref() {
        match mime {
            "application/x-mpegurl"
            | "audio/mpegurl"
            | "audio/x-mpegurl"
            | "audio/m3u"
            | "audio/x-scpls" => {
                return Kind::Playlist;
            }
            "text/html" | "application/xhtml+xml" => return Kind::Html,
            "application/ogg" => return Kind::Audio,
            value if value.starts_with("audio/") => return Kind::Audio,
            _ => {}
        }
    }
    if ext.as_deref().is_some_and(is_audio_extension) {
        return Kind::Audio;
    }
    Kind::Unknown
}

fn extension(url: &Url) -> Option<String> {
    let segment = url.path_segments()?.next_back()?;
    let (name, ext) = segment.rsplit_once('.')?;
    if name.is_empty() || ext.is_empty() {
        return None;
    }
    Some(ext.to_ascii_lowercase())
}

fn is_audio_extension(ext: &str) -> bool {
    matches!(
        ext,
        "mp3" | "aac" | "ogg" | "oga" | "opus" | "flac" | "wav" | "m4a" | "m4b" | "webm"
    )
}

async fn read_bounded(mut response: Response) -> Result<Vec<u8>, StreamError> {
    if response
        .headers()
        .get(CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<usize>().ok())
        .is_some_and(|length| length > MAX_BODY_BYTES)
    {
        return Err(StreamError::TooLarge);
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|err| request_error(&err))? {
        if bytes.len() + chunk.len() > MAX_BODY_BYTES {
            return Err(StreamError::TooLarge);
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

fn join(base: &Url, reference: &str) -> Result<Url, StreamError> {
    let reference = reference.trim();
    if reference.is_empty() {
        return Err(StreamError::EmptyPlaylist);
    }
    let url = base.join(reference).map_err(|_| StreamError::InvalidUrl)?;
    validate(&url)?;
    Ok(url)
}

fn looks_like_pls(text: &str) -> bool {
    text.lines()
        .map(str::trim)
        .take(4)
        .any(|line| line.eq_ignore_ascii_case("[playlist]"))
}

fn looks_like_playlist(text: &str) -> bool {
    let trimmed = text.trim_start();
    trimmed.starts_with("#EXTM3U") || looks_like_pls(trimmed)
}

fn looks_like_html(text: &str) -> bool {
    let lower = text
        .chars()
        .take(512)
        .collect::<String>()
        .to_ascii_lowercase();
    [
        "<!doctype html",
        "<html",
        "<head",
        "<meta",
        "<audio",
        "<source",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
}

/// Разбирает `.m3u`: имя записи — из `#EXTINF` перед ней. Больше 20 записей
/// не смотрим, а первую ссылку выбирает [`first_playable`].
fn parse_m3u(text: &str) -> Result<Vec<(Option<String>, String)>, StreamError> {
    let mut entries = Vec::new();
    let mut pending = None;
    for line in text.lines().map(str::trim) {
        if line.is_empty() {
            continue;
        }
        if let Some(info) = line.strip_prefix("#EXTINF:") {
            pending = info
                .split_once(',')
                .map(|(_, name)| name.trim())
                .filter(|name| !name.is_empty())
                .map(str::to_owned);
            continue;
        }
        if line.starts_with('#') {
            continue;
        }
        entries.push((pending.take(), line.to_owned()));
        if entries.len() >= MAX_PLAYLIST_ENTRIES {
            break;
        }
    }
    if entries.is_empty() {
        return Err(StreamError::EmptyPlaylist);
    }
    Ok(entries)
}

/// Разбирает `.pls`: `FileN` и `TitleN` сопоставляются по номеру.
fn parse_pls(text: &str) -> Result<Vec<(Option<String>, String)>, StreamError> {
    let mut files: Vec<(usize, String)> = Vec::new();
    let mut titles: HashMap<usize, String> = HashMap::new();
    for line in text.lines().map(str::trim) {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        let value = value.trim();
        if value.is_empty() {
            continue;
        }
        let lower = key.to_ascii_lowercase();
        if let Some(number) = lower.strip_prefix("file").and_then(parse_index) {
            files.push((number, value.to_owned()));
        } else if let Some(number) = lower.strip_prefix("title").and_then(parse_index) {
            titles.insert(number, value.to_owned());
        }
    }
    files.sort_by_key(|(number, _)| *number);
    files.truncate(MAX_PLAYLIST_ENTRIES);
    if files.is_empty() {
        return Err(StreamError::EmptyPlaylist);
    }
    Ok(files
        .into_iter()
        .map(|(number, file)| {
            (
                titles.remove(&number).filter(|title| !title.is_empty()),
                file,
            )
        })
        .collect())
}

/// Первая запись плейлиста, которую можно запросить: не-http(s) строки
/// пропускаются, HLS-ссылка отклоняет плейлист целиком.
fn first_playable(
    base: &Url,
    entries: Vec<(Option<String>, String)>,
) -> Result<(Option<String>, Url), StreamError> {
    let mut last_error = None;
    for (title, reference) in entries {
        match join(base, &reference) {
            Ok(url) => return Ok((title, url)),
            Err(err @ StreamError::Hls) => return Err(err),
            Err(err) => last_error = Some(err),
        }
    }
    Err(last_error.unwrap_or(StreamError::EmptyPlaylist))
}

fn parse_index(raw: &str) -> Option<usize> {
    if raw.is_empty() || !raw.chars().all(|ch| ch.is_ascii_digit()) {
        return None;
    }
    raw.parse().ok()
}

/// `og:audio` и `og:title`: `<meta property=… content=…>` или `name=…`.
fn meta_content(html: &str, key: &str) -> Option<String> {
    for tag in tags(html) {
        if !tag_name(tag).eq_ignore_ascii_case("meta") {
            continue;
        }
        let attrs = attributes(tag);
        let property = attrs
            .iter()
            .find(|(name, _)| name == "property" || name == "name")
            .map(|(_, value)| value.as_str());
        if property.is_some_and(|value| value.eq_ignore_ascii_case(key)) {
            return attrs
                .iter()
                .find(|(name, _)| name == "content")
                .map(|(_, value)| unescape(value))
                .filter(|value| !value.is_empty());
        }
    }
    None
}

fn tag_attr(html: &str, wanted: &str, attr: &str) -> Option<String> {
    for tag in tags(html) {
        if !tag_name(tag).eq_ignore_ascii_case(wanted) {
            continue;
        }
        let attrs = attributes(tag);
        if let Some((_, value)) = attrs.iter().find(|(name, _)| name == attr)
            && !value.is_empty()
        {
            return Some(unescape(value));
        }
    }
    None
}

fn html_title(html: &str) -> Option<String> {
    let lower = html.to_ascii_lowercase();
    let start = lower.find("<title")?;
    let open_end = lower[start..].find('>')? + start + 1;
    let close = lower[open_end..].find("</title>")? + open_end;
    let title = collapse_ws(&unescape(&html[open_end..close]));
    (!title.is_empty()).then_some(title)
}

/// `<link rel="alternate" type="application/vnd.apple.mpegurl">` — страница
/// предлагает HLS-вариант; молча брать `og:audio` рядом нельзя.
fn has_hls_alternate(html: &str) -> bool {
    tags(html).into_iter().any(|tag| {
        if !tag_name(tag).eq_ignore_ascii_case("link") {
            return false;
        }
        let attrs = attributes(tag);
        let rel = attrs
            .iter()
            .find(|(name, _)| name == "rel")
            .map(|(_, value)| value.to_ascii_lowercase())
            .unwrap_or_default();
        let mime = attrs
            .iter()
            .find(|(name, _)| name == "type")
            .map(|(_, value)| value.to_ascii_lowercase())
            .unwrap_or_default();
        rel.split_whitespace().any(|value| value == "alternate")
            && (mime.contains("mpegurl") || mime.contains("m3u8"))
    })
}

fn tags(html: &str) -> Vec<&str> {
    let mut result = Vec::new();
    let mut offset = 0;
    while let Some(start) = html[offset..].find('<') {
        let start = offset + start;
        let Some(length) = html[start..].find('>') else {
            break;
        };
        let end = start + length;
        result.push(&html[start + 1..end]);
        offset = end + 1;
    }
    result
}

fn tag_name(tag: &str) -> &str {
    let tag = tag.trim_start_matches('/');
    let end = tag
        .find(|ch: char| ch.is_ascii_whitespace() || ch == '/')
        .unwrap_or(tag.len());
    &tag[..end]
}

fn attributes(tag: &str) -> Vec<(String, String)> {
    let mut attrs = Vec::new();
    let bytes = tag.as_bytes();
    let mut index = 0;
    while index < bytes.len() && !bytes[index].is_ascii_whitespace() {
        index += 1;
    }
    while index < bytes.len() {
        while index < bytes.len() && (bytes[index].is_ascii_whitespace() || bytes[index] == b'/') {
            index += 1;
        }
        let name_start = index;
        while index < bytes.len()
            && !bytes[index].is_ascii_whitespace()
            && bytes[index] != b'='
            && bytes[index] != b'/'
        {
            index += 1;
        }
        if name_start == index {
            index += 1;
            continue;
        }
        let name = tag[name_start..index].to_ascii_lowercase();
        while index < bytes.len() && bytes[index].is_ascii_whitespace() {
            index += 1;
        }
        if index < bytes.len() && bytes[index] == b'=' {
            index += 1;
            while index < bytes.len() && bytes[index].is_ascii_whitespace() {
                index += 1;
            }
            let value = if index < bytes.len() && (bytes[index] == b'"' || bytes[index] == b'\'') {
                let quote = bytes[index];
                index += 1;
                let value_start = index;
                while index < bytes.len() && bytes[index] != quote {
                    index += 1;
                }
                let value = &tag[value_start..index];
                if index < bytes.len() {
                    index += 1;
                }
                value.to_owned()
            } else {
                let value_start = index;
                while index < bytes.len()
                    && !bytes[index].is_ascii_whitespace()
                    && bytes[index] != b'>'
                {
                    index += 1;
                }
                tag[value_start..index].to_owned()
            };
            attrs.push((name, value));
        } else {
            attrs.push((name, String::new()));
        }
    }
    attrs
}

fn unescape(value: &str) -> String {
    value
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
}

fn collapse_ws(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
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
            let mut probe = [0u8; 4096];
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

    async fn mock(replies: Vec<String>) -> (String, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let handle = tokio::spawn(async move {
            let _ = tokio::time::timeout(Duration::from_secs(10), respond(listener, replies)).await;
        });
        (format!("http://{address}"), handle)
    }

    #[tokio::test]
    async fn resolves_relative_m3u_entry_and_title() {
        let m3u = "#EXTM3U\n#EXTINF:-1,My Stream\nstream.mp3\n#EXTINF:-1,Other\nother.mp3\n";
        let (base, server) = mock(vec![
            ok_reply(Some("text/plain"), m3u),
            ok_reply(Some("audio/mpeg"), "ID3"),
        ])
        .await;
        let resolved = resolve(&format!("{base}/list.m3u")).await.expect("m3u");
        server.await.unwrap();
        assert_eq!(resolved.url, format!("{base}/stream.mp3"));
        assert_eq!(resolved.title.as_deref(), Some("My Stream"));
        assert_eq!(resolved.homepage, Some(format!("{base}/list.m3u")));
    }

    #[tokio::test]
    async fn resolves_pls_file_and_title() {
        let pls = "[playlist]\nFile1=live.ogg\nTitle1=PL Title\nNumberOfEntries=1\n";
        let (base, server) = mock(vec![
            ok_reply(Some("audio/x-scpls"), pls),
            ok_reply(Some("audio/ogg"), "OggS"),
        ])
        .await;
        let resolved = resolve(&format!("{base}/radio.pls")).await.expect("pls");
        server.await.unwrap();
        assert_eq!(resolved.url, format!("{base}/live.ogg"));
        assert_eq!(resolved.title.as_deref(), Some("PL Title"));
    }

    #[tokio::test]
    async fn resolves_html_og_audio_and_title() {
        let html = "<!doctype html><html><head><meta property=\"og:title\" content=\"Jazz &amp; Soul\"><meta property=\"og:audio\" content=\"/audio.mp3\"></head><body><audio src=\"/fallback.mp3\"></audio></body></html>";
        let (base, server) = mock(vec![
            ok_reply(Some("text/html; charset=utf-8"), html),
            ok_reply(Some("audio/mpeg"), "ID3"),
        ])
        .await;
        let resolved = resolve(&format!("{base}/page.html")).await.expect("html");
        server.await.unwrap();
        assert_eq!(resolved.url, format!("{base}/audio.mp3"));
        assert_eq!(resolved.title.as_deref(), Some("Jazz & Soul"));
    }

    #[tokio::test]
    async fn resolves_audio_tag_and_document_title() {
        let html = "<html><head><title>  Page\n Title </title></head><body><source src=\"stream.opus\"></body></html>";
        let (base, server) = mock(vec![
            ok_reply(Some("text/html"), html),
            ok_reply(Some("audio/opus"), "OggS"),
        ])
        .await;
        let resolved = resolve(&format!("{base}/radio")).await.expect("html");
        server.await.unwrap();
        assert_eq!(resolved.url, format!("{base}/stream.opus"));
        assert_eq!(resolved.title.as_deref(), Some("Page Title"));
    }

    #[tokio::test]
    async fn rejects_hls_by_extension_mime_and_body() {
        assert_eq!(
            resolve("http://127.0.0.1:9/live.m3u8").await.unwrap_err(),
            StreamError::Hls
        );
        let (base, server) = mock(vec![ok_reply(
            Some("application/vnd.apple.mpegurl"),
            "#EXTM3U",
        )])
        .await;
        assert_eq!(
            resolve(&format!("{base}/live")).await.unwrap_err(),
            StreamError::Hls
        );
        server.await.unwrap();

        let (base, server) = mock(vec![ok_reply(
            Some("text/plain"),
            "#EXTM3U\n#EXT-X-STREAM-INF:BANDWIDTH=128000\nlow.m3u8\n",
        )])
        .await;
        assert_eq!(
            resolve(&format!("{base}/list.m3u")).await.unwrap_err(),
            StreamError::Hls
        );
        server.await.unwrap();
    }

    #[tokio::test]
    async fn rejects_hls_alternate_link_on_page() {
        let html = "<html><head><link rel=\"alternate\" type=\"application/vnd.apple.mpegurl\" href=\"/live.m3u8\"><meta property=\"og:audio\" content=\"/fallback.mp3\"></head></html>";
        let (base, server) = mock(vec![ok_reply(Some("text/html"), html)]).await;
        assert_eq!(
            resolve(&format!("{base}/page")).await.unwrap_err(),
            StreamError::Hls
        );
        server.await.unwrap();
    }

    #[tokio::test]
    async fn follows_redirects_and_rejects_loops() {
        let (base, server) = mock(vec![
            redirect_reply("/final"),
            ok_reply(Some("audio/mpeg"), "ID3"),
        ])
        .await;
        let resolved = resolve(&format!("{base}/start")).await.expect("redirect");
        server.await.unwrap();
        assert_eq!(resolved.url, format!("{base}/final"));

        let loop_reply = redirect_reply("/loop");
        let (base, server) = mock(vec![loop_reply; MAX_REDIRECTS + 1]).await;
        assert_eq!(
            resolve(&format!("{base}/loop")).await.unwrap_err(),
            StreamError::RedirectLimit
        );
        server.await.unwrap();
    }

    #[tokio::test]
    async fn accepts_octet_stream_with_audio_extension_without_body() {
        let (base, server) = mock(vec![ok_reply(Some("application/octet-stream"), "ID3")]).await;
        let resolved = resolve(&format!("{base}/live.mp3")).await.expect("audio");
        server.await.unwrap();
        assert_eq!(resolved.url, format!("{base}/live.mp3"));
    }

    #[tokio::test]
    async fn rejects_oversized_pages() {
        let body = "a".repeat(MAX_BODY_BYTES + 1024);
        let (base, server) = mock(vec![ok_reply(Some("text/html"), &body)]).await;
        assert_eq!(
            resolve(&format!("{base}/big.html")).await.unwrap_err(),
            StreamError::TooLarge
        );
        server.await.unwrap();
    }

    #[tokio::test]
    async fn rejects_empty_playlist_and_non_audio_page() {
        let (base, server) = mock(vec![ok_reply(Some("text/plain"), "#EXTM3U\n")]).await;
        assert_eq!(
            resolve(&format!("{base}/empty.m3u")).await.unwrap_err(),
            StreamError::EmptyPlaylist
        );
        server.await.unwrap();

        let (base, server) = mock(vec![ok_reply(Some("text/plain"), "hello world")]).await;
        assert_eq!(
            resolve(&format!("{base}/noise")).await.unwrap_err(),
            StreamError::NotAudio
        );
        server.await.unwrap();
    }

    #[tokio::test]
    async fn rejects_playlist_cycles_and_depth() {
        let (base, server) = mock(vec![ok_reply(Some("text/plain"), "#EXTM3U\nloop.m3u\n")]).await;
        assert_eq!(
            resolve(&format!("{base}/loop.m3u")).await.unwrap_err(),
            StreamError::PlaylistCycle
        );
        server.await.unwrap();

        let first = "#EXTM3U\nsecond.m3u\n";
        let second = "#EXTM3U\nthird.m3u\n";
        let third = "#EXTM3U\nfourth.m3u\n";
        let (base, server) = mock(vec![
            ok_reply(Some("text/plain"), first),
            ok_reply(Some("text/plain"), second),
            ok_reply(Some("text/plain"), third),
        ])
        .await;
        assert_eq!(
            resolve(&format!("{base}/first.m3u")).await.unwrap_err(),
            StreamError::PlaylistDepth
        );
        server.await.unwrap();
    }

    #[tokio::test]
    async fn strict_mode_refuses_private_targets_before_connecting() {
        let client = Client::builder()
            .user_agent(USER_AGENT)
            .no_proxy()
            .redirect(redirect::Policy::none())
            .timeout(REQUEST_TIMEOUT)
            .build()
            .unwrap();
        // Порт 9 закрыт: до сети дело дойти не должно.
        let mut redirects = 0;
        let target = Url::parse("http://127.0.0.1:9/live.mp3").unwrap();
        assert_eq!(
            fetch_following(&client, target, &mut redirects, true)
                .await
                .unwrap_err(),
            StreamError::PrivateTarget
        );
    }

    #[tokio::test]
    async fn publicity_check_handles_public_and_private_literals() {
        let url = |value: &str| Url::parse(value).unwrap();
        assert!(
            ensure_public(&url("https://93.184.216.34/live.mp3"))
                .await
                .is_ok(),
            "публичный literal не требует DNS"
        );
        assert_eq!(
            ensure_public(&url("http://127.0.0.1/live"))
                .await
                .unwrap_err(),
            StreamError::PrivateTarget
        );
        assert_eq!(
            ensure_public(&url("http://[64:ff9b::a00:1]/live"))
                .await
                .unwrap_err(),
            StreamError::PrivateTarget,
            "NAT64 с приватным вложенным адресом"
        );
        assert_eq!(
            ensure_public(&url("http://[::1]/live")).await.unwrap_err(),
            StreamError::PrivateTarget
        );
    }

    #[test]
    fn validation_refuses_non_http_userinfo_and_hls() {
        let parse = |value: &str| validate(&Url::parse(value).unwrap());
        assert!(parse("http://127.0.0.1:8080/live.mp3").is_ok());
        assert!(parse("https://radio.example.org/live").is_ok());
        assert_eq!(
            parse("ftp://radio.example.org/live").unwrap_err(),
            StreamError::UnsupportedScheme
        );
        assert_eq!(
            parse("file:///etc/passwd").unwrap_err(),
            StreamError::UnsupportedScheme
        );
        assert_eq!(
            parse("http://user:pass@radio.example.org/live").unwrap_err(),
            StreamError::InvalidUrl
        );
        assert_eq!(
            parse("http://radio.example.org/live.m3u8?x=1").unwrap_err(),
            StreamError::Hls
        );
        assert!(!Url::parse("http://").unwrap_err().to_string().is_empty());
    }

    #[test]
    fn attribute_scanner_handles_quotes_and_entities() {
        let tag = r#"meta property="og:audio" content="https://x/y.mp3?a=1&amp;b=2""#;
        let attrs = attributes(tag);
        assert_eq!(
            attrs
                .iter()
                .find(|(name, _)| name == "content")
                .map(|(_, value)| value.as_str()),
            Some("https://x/y.mp3?a=1&amp;b=2")
        );
        assert_eq!(unescape("a&amp;b&quot;c"), "a&b\"c");
        assert_eq!(tag_name("source src=x"), "source");
    }
}
