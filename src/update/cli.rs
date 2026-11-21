//! CLI-команда `orbitfm update` (ADR-0067).
//!
//! Cold-start: сеть проверяется здесь, GUI и IPC не поднимаются.

use anyhow::{Context, Result, anyhow, bail};
use semver::Version;

use super::github::{GithubSource, UpdateSource};
use super::{Channel, UpdateState, format_utc_now, pick_release};

/// Аргументы `orbitfm update ...`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Args {
    pub json: bool,
    pub channel: Channel,
}

/// Разбор аргументов (после `update`).
pub fn parse(args: &[String]) -> Result<Args> {
    let mut parsed = Args {
        json: false,
        channel: Channel::default(),
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
                parsed.channel =
                    Channel::parse(value).with_context(|| format!("неизвестный канал: {value}"))?;
            }
            "channel" => bail!(
                "update channel пока не сохраняется в settings.json \
                 (ADR-0067 UPD-3); используйте `update --channel stable|dev`"
            ),
            "install" => bail!("update install ещё не реализован (ADR-0067 UPD-6)"),
            other => bail!("update: неизвестный аргумент `{other}`"),
        }
        index += 1;
    }
    Ok(parsed)
}

/// Выполнить проверку обновлений.
pub fn run(runtime: &tokio::runtime::Runtime, args: &Args) -> Result<()> {
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
            if let Some(asset) = release.platform_asset() {
                println!("  asset:     {} ({} bytes)", asset.name, asset.size);
            } else {
                println!("  asset:     нет для этой платформы");
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

    #[test]
    fn parses_flags() {
        let args = parse(&["--check".to_owned(), "--json".to_owned()]).expect("parse");
        assert!(args.json);
        assert_eq!(args.channel, Channel::Stable);

        let args = parse(&["--channel".to_owned(), "dev".to_owned()]).expect("parse");
        assert_eq!(args.channel, Channel::Dev);
    }

    #[test]
    fn rejects_bad_channel_and_unknown() {
        assert!(parse(&["--channel".to_owned(), "beta".to_owned()]).is_err());
        assert!(parse(&["--channel".to_owned()]).is_err());
        assert!(parse(&["--nope".to_owned()]).is_err());
    }

    #[test]
    fn install_and_channel_are_honest_about_not_being_ready() {
        assert!(parse(&["install".to_owned()]).is_err());
        assert!(parse(&["channel".to_owned(), "dev".to_owned()]).is_err());
    }
}
