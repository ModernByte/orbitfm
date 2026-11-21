//! Динамическая версия TUI.
//!
//! Базовое semver-число живёт в `Cargo.toml` (или приходит из
//! `ORBITFM_TUI_VERSION` при релизной сборке). git-состояние и дата сборки
//! вшиваются здесь, а не проставляются руками: `--version` всегда говорит
//! правду о том, из какого коммита и с какой грязью собран бинарь.
//!
//! Схема — `docs/tui/versioning.md`. Воспроизводимость: `SOURCE_DATE_EPOCH`
//! управляет датой сборки.

use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    for path in ["../../.git/HEAD", "../../.git/index", "../../.git/refs"] {
        println!("cargo:rerun-if-changed={path}");
    }
    println!("cargo:rerun-if-env-changed=ORBITFM_TUI_VERSION");
    println!("cargo:rerun-if-env-changed=SOURCE_DATE_EPOCH");

    println!("cargo:rustc-env=ORBITFM_TUI_GIT={}", git_state());
    println!("cargo:rustc-env=ORBITFM_TUI_BUILD_DATE={}", build_date());
}

/// `sha` или `sha-dirty`; `unknown` — если git недоступен (сборка из архива).
fn git_state() -> String {
    let Some(sha) = git(&["rev-parse", "--short=12", "HEAD"]) else {
        return "unknown".to_owned();
    };
    if sha.is_empty() {
        return "unknown".to_owned();
    }
    let dirty = git(&["status", "--porcelain"]).is_some_and(|out| !out.is_empty());
    if dirty { format!("{sha}-dirty") } else { sha }
}

fn git(args: &[&str]) -> Option<String> {
    let out = Command::new("git").args(args).output().ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy_owned(out.stdout).trim().to_owned())
}

fn build_date() -> String {
    let secs = std::env::var("SOURCE_DATE_EPOCH")
        .ok()
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or_else(|| {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or_default(|duration| duration.as_secs()) as i64
        });
    format_utc(secs)
}

/// `ГГГГ-ММ-ДД` без внешних зависимостей (алгоритм civil_from_days).
fn format_utc(secs: i64) -> String {
    let (year, month, day) = civil_from_days(secs.div_euclid(86_400));
    format!("{year:04}-{month:02}-{day:02}")
}

fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
    let year = if month <= 2 { year + 1 } else { year };
    (year, month, day)
}
