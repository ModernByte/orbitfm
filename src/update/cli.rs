//! CLI-команда `orbitfm update` (ADR-0067).
//!
//! Cold-start: сеть проверяется здесь, GUI и IPC не поднимаются. Смена канала
//! сохраняется в `settings.json` только когда ядро не запущено — иначе его
//! перезапишет работающий экземпляр (см. UPD-3b).

use anyhow::{Context, Result, anyhow, bail};
use semver::Version;

use crate::ipc::IpcServer;
use crate::settings::SettingsStore;

use super::github::{GithubSource, UpdateSource};
use super::{Channel, UpdateState, format_utc_now, pick_release};

/// Что делает команда.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Проверить обновления (по умолчанию).
    Check,
    /// Сохранить канал в `settings.json`.
    SetChannel(Channel),
    /// Скачать и установить обновление (только AppImage).
    Install,
}

/// Аргументы `orbitfm update ...`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Args {
    pub json: bool,
    pub channel: Channel,
    pub action: Action,
}

/// Разбор аргументов (после `update`).
pub fn parse(args: &[String]) -> Result<Args> {
    let mut parsed = Args {
        json: false,
        channel: Channel::default(),
        action: Action::Check,
    };
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            // `--check` — поведение по умолчанию (совместимость и явность).
            "--check" => {}
            "--json" => parsed.json = true,
            "--channel" => {
                index += 1;
                let value = args
                    .get(index)
                    .context("update --channel требует stable|dev")?;
                parsed.channel = parse_channel(value)?;
            }
            "channel" => {
                index += 1;
                let value = args
                    .get(index)
                    .context("update channel требует stable|dev")?;
                let channel = parse_channel(value)?;
                parsed.channel = channel;
                parsed.action = Action::SetChannel(channel);
            }
            "install" => parsed.action = Action::Install,
            other => bail!("update: неизвестный аргумент `{other}`"),
        }
        index += 1;
    }
    Ok(parsed)
}

fn parse_channel(value: &str) -> Result<Channel> {
    Channel::parse(value).with_context(|| format!("неизвестный канал: {value}"))
}

/// Выполнить команду.
pub fn run(runtime: &tokio::runtime::Runtime, args: &Args) -> Result<()> {
    match args.action {
        Action::SetChannel(channel) => set_channel(runtime, channel),
        Action::Check => check(runtime, args),
        Action::Install => install(runtime, args),
    }
}

/// Скачать и установить AppImage (ADR-0067 §6).
fn install(runtime: &tokio::runtime::Runtime, args: &Args) -> Result<()> {
    if !crate::update::install::is_appimage() {
        bail!("установка доступна только для AppImage (ADR-0067 §6)");
    }
    let target = crate::update::install::installed_path()
        .context("не удалось определить путь установки AppImage")?;
    let current: Version = env!("CARGO_PKG_VERSION")
        .parse()
        .context("CARGO_PKG_VERSION не является semver")?;
    let source = GithubSource::orbitfm().context("не удалось создать клиент обновлений")?;
    let releases = runtime
        .block_on(source.releases())
        .map_err(|error| anyhow!(error.to_string()))?;
    let latest = pick_release(&releases, args.channel).context("в канале нет релизов")?;
    if latest.version <= current {
        println!("уже последняя версия: {current}");
        return Ok(());
    }
    let client =
        crate::update::install::download_client().map_err(|error| anyhow!(error.to_string()))?;
    runtime
        .block_on(crate::update::install::install(&latest, &target, &client))
        .map_err(|error| anyhow!(error.to_string()))?;
    println!(
        "установлено {} → {} (применится при следующем запуске)",
        latest.version,
        target.display()
    );
    Ok(())
}

fn check(runtime: &tokio::runtime::Runtime, args: &Args) -> Result<()> {
    let source = GithubSource::orbitfm().context("не удалось создать клиент обновлений")?;
    let current: Version = env!("CARGO_PKG_VERSION")
        .parse()
        .context("CARGO_PKG_VERSION не является semver")?;

    let releases = runtime
        .block_on(source.releases())
        .map_err(|error| anyhow!(error.to_string()))?;

    let mut state = UpdateState::new(args.channel, current);
    state.latest = pick_release(&releases, args.channel);
    state.checked_at = Some(format_utc_now());

    if args.json {
        print_json(&state);
    } else {
        print_human(&state);
    }
    Ok(())
}

/// Сохранить канал. Пока ядро работает, файл принадлежит ему — не трогаем.
fn set_channel(runtime: &tokio::runtime::Runtime, channel: Channel) -> Result<()> {
    if runtime
        .block_on(IpcServer::send_to_existing("status"))
        .is_ok()
    {
        bail!(
            "OrbitFM запущен: смените канал в настройках или остановите его \
             (settings.json принадлежит работающему экземпляру)"
        );
    }
    let mut store = SettingsStore::load_default().context("не удалось прочитать settings.json")?;
    store
        .set_update_channel(channel)
        .context("не удалось сохранить канал обновлений")?;
    println!("update channel: {}", channel.as_str());
    Ok(())
}

fn print_human(state: &UpdateState) {
    println!("OrbitFM update");
    println!("  channel:   {}", state.channel.as_str());
    println!("  current:   {}", state.current);
    match &state.latest {
        Some(release) => {
            println!("  latest:    {} ({})", release.version, release.tag);
            println!(
                "  available: {}",
                if state.available() { "yes" } else { "no" }
            );
            if let Some(published) = &release.published_at {
                println!("  published: {published}");
            }
            match release.platform_asset() {
                Some(asset) => println!("  asset:     {} ({} bytes)", asset.name, asset.size),
                None => println!("  asset:     нет для этой платформы"),
            }
            if let Some(url) = &release.html_url {
                println!("  release:   {url}");
            }
            if let Some(notes) = &release.notes
                && !notes.trim().is_empty()
            {
                println!("  notes:");
                for line in notes.lines().take(20) {
                    println!("    {line}");
                }
            }
        }
        None => println!("  latest:    нет релизов в канале"),
    }
    if let Some(checked) = &state.checked_at {
        println!("  checked:   {checked}");
    }
}

fn print_json(state: &UpdateState) {
    let release = state.latest.as_ref();
    let asset = release.and_then(|release| {
        release.platform_asset().map(|asset| {
            serde_json::json!({
                "name": asset.name,
                "size": asset.size,
                "url": asset.url,
                "sha256": null,
            })
        })
    });
    let value = serde_json::json!({
        "schema": 1,
        "channel": state.channel.as_str(),
        "current": state.current.to_string(),
        "latest": release.map(|release| release.version.to_string()),
        "available": state.available(),
        "checked_at": state.checked_at,
        "release_url": release.and_then(|release| release.html_url.clone()),
        "published_at": release.and_then(|release| release.published_at.clone()),
        "notes": release.and_then(|release| release.notes.clone()),
        "checksums": release.is_some_and(|release| release.checksums_url().is_some()),
        "asset": asset,
    });
    println!("{value}");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    #[test]
    fn parses_flags() {
        let parsed = parse(&args(&["--check", "--json"])).expect("parse");
        assert!(parsed.json);
        assert_eq!(parsed.channel, Channel::Stable);
        assert_eq!(parsed.action, Action::Check);
    }

    #[test]
    fn parses_channel_override_and_subcommand() {
        let parsed = parse(&args(&["--channel", "dev"])).expect("parse");
        assert_eq!(parsed.channel, Channel::Dev);
        assert_eq!(parsed.action, Action::Check);

        let parsed = parse(&args(&["channel", "dev"])).expect("parse");
        assert_eq!(parsed.action, Action::SetChannel(Channel::Dev));
    }

    #[test]
    fn rejects_bad_channel_and_unknown() {
        assert!(parse(&args(&["--channel", "beta"])).is_err());
        assert!(parse(&args(&["--channel"])).is_err());
        assert!(parse(&args(&["channel"])).is_err());
        assert!(parse(&args(&["--nope"])).is_err());
    }

    #[test]
    fn install_is_a_known_action() {
        assert_eq!(
            parse(&args(&["install"])).expect("parse").action,
            Action::Install
        );
    }
}
