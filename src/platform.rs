//! Everything that differs between Windows and Linux, in one place.
//!
//! The choice is made at run time with `cfg!(windows)` rather than `#[cfg]`:
//! that way both branches always compile and get type-checked, and touching one
//! platform can't silently break the other. The cost is a few bytes of dead
//! code in the binary.

use std::env;

use chrono::{Datelike, Local};

pub const WIN: bool = cfg!(windows);

fn s(v: &[&str]) -> Vec<String> {
    v.iter().map(|x| x.to_string()).collect()
}

/// Opens a URL in the system browser.
pub fn open_url(url: &str) -> Vec<String> {
    // The "" after `start` is the window title: without it, start treats the
    // quoted URL as the title and opens nothing.
    if WIN {
        s(&["cmd", "/c", "start", "", url])
    } else {
        s(&["xdg-open", url])
    }
}

/// Terminates a process. `force` = SIGKILL / `taskkill /F`.
pub fn kill(pid: &str, force: bool) -> Vec<String> {
    match (WIN, force) {
        (true, false) => s(&["taskkill", "/PID", pid]),
        (true, true) => s(&["taskkill", "/F", "/T", "/PID", pid]),
        (false, false) => s(&["kill", pid]),
        (false, true) => s(&["kill", "-9", pid]),
    }
}

/// Change a process priority: `renice` on Unix, `PriorityClass` on Windows.
/// Returns (prompt label, template with `{}`).
pub fn renice(pid: &str) -> (String, Vec<String>) {
    if WIN {
        (
            format!("Priority for {pid} (Idle · BelowNormal · Normal · AboveNormal · High)"),
            s(&[
                "powershell",
                "-NoProfile",
                "-Command",
                &format!("(Get-Process -Id {pid}).PriorityClass='{{}}'"),
            ]),
        )
    } else {
        (
            format!("Priority (nice) for {pid}"),
            s(&["renice", "{}", "-p", pid]),
        )
    }
}

/// Runs a command with elevated privileges: `sudo` on Unix, UAC on Windows.
pub fn elevate(cmd: &[&str]) -> Vec<String> {
    if WIN {
        let inner = cmd.join(" ").replace('\'', "''");
        s(&[
            "powershell",
            "-NoProfile",
            "-Command",
            &format!("Start-Process powershell -Verb RunAs -ArgumentList '-NoProfile','-Command','{inner}'"),
        ])
    } else {
        let mut v = s(&["sudo"]);
        v.extend(s(cmd));
        v
    }
}

/// A one-shot PowerShell command, standing in for what `ss`, `ip` or
/// `journalctl` do on Linux. `-NoProfile` skips the user profile (faster start).
pub fn ps(script: &str) -> Vec<String> {
    // Without pinning the encoding, PowerShell writes in the console code page
    // (CP850 on a Spanish Windows) and non-ASCII text arrives mangled.
    s(&[
        "powershell",
        "-NoProfile",
        "-NonInteractive",
        "-Command",
        &format!("[Console]::OutputEncoding=[System.Text.Encoding]::UTF8; {script}"),
    ])
}

/// The user's text editor.
pub fn editor() -> String {
    env::var("EDITOR")
        .or_else(|_| env::var("VISUAL"))
        .unwrap_or_else(|_| if WIN { "notepad".into() } else { "nano".into() })
}

/// Full-screen `git diff` of one file through delta.
pub fn diff_delta(repo: &str, path: &str) -> Vec<String> {
    if WIN {
        s(&[
            "cmd",
            "/c",
            &format!(r#"git -C "{repo}" diff HEAD -- "{path}" | delta --paging=always"#),
        ])
    } else {
        // Positional arguments: paths with spaces or quotes break nothing.
        let mut v = s(&[
            "sh",
            "-c",
            r#"git -C "$1" diff HEAD -- "$2" | delta --paging=always"#,
            "sh",
        ]);
        v.push(repo.into());
        v.push(path.into());
        v
    }
}

/// Local date, replacing the Unix `date +%A ...` call.
pub fn now() -> String {
    const DAYS: [&str; 7] = [
        "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday",
    ];
    let t = Local::now();
    format!(
        "{} {}",
        DAYS[t.weekday().num_days_from_monday() as usize],
        t.format("%d/%m/%Y  %H:%M")
    )
}
