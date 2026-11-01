//! Перезагрузка баров без `killall`/`pkill` (ADR-0042 §3).
//!
//! Waybar сначала получает документированную команду IPC, затем `SIGUSR2`
//! только собственному процессу, найденному по `/proc/*/comm`. Если оба пути
//! недоступны, UI честно просит перезапустить бар вручную.

use std::process::{Command, Stdio};

use super::Target;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReloadOutcome {
    Reloaded,
    Signaled(usize),
    Manual,
}

pub(crate) fn reload(target: Target) -> ReloadOutcome {
    match target {
        Target::Waybar => reload_waybar(),
        Target::Polybar => {
            if command_succeeds("polybar-msg", &["cmd", "restart"]) {
                ReloadOutcome::Reloaded
            } else {
                ReloadOutcome::Manual
            }
        }
        _ => ReloadOutcome::Manual,
    }
}

fn reload_waybar() -> ReloadOutcome {
    if command_succeeds("waybar-msg", &["cmd", "reload"]) {
        return ReloadOutcome::Reloaded;
    }
    match signal_waybar() {
        0 => ReloadOutcome::Manual,
        signaled => ReloadOutcome::Signaled(signaled),
    }
}

#[cfg(target_os = "linux")]
fn signal_waybar() -> usize {
    let mut signaled = 0;
    for pid in waybar_pids() {
        // SAFETY: PID взят из /proc и принадлежит процессу с comm == "waybar";
        // при совпадении имени сигнал безопасен и не завершает процесс.
        if unsafe { libc::kill(pid, libc::SIGUSR2) } == 0 {
            signaled += 1;
        }
    }
    signaled
}

#[cfg(not(target_os = "linux"))]
fn signal_waybar() -> usize {
    0
}

fn command_succeeds(program: &str, args: &[&str]) -> bool {
    Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

#[cfg(target_os = "linux")]
fn waybar_pids() -> Vec<i32> {
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return Vec::new();
    };
    let mut pids = Vec::new();
    for entry in entries.flatten() {
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|name| name.parse::<i32>().ok())
        else {
            continue;
        };
        let Ok(comm) = std::fs::read_to_string(entry.path().join("comm")) else {
            continue;
        };
        if comm.trim() == "waybar" {
            pids.push(pid);
        }
    }
    pids
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsupported_targets_ask_for_a_manual_restart() {
        assert_eq!(reload(Target::I3blocks), ReloadOutcome::Manual);
        assert_eq!(reload(Target::Chrome), ReloadOutcome::Manual);
    }
}
