use std::collections::HashMap;
use std::ffi::OsString;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Mutex;

use anyhow::{bail, Result};

use crate::platform;

/// Runs a command capturing its output. Errors if the exit code isn't 0.
pub fn run_cmd(cmd: &[String]) -> Result<String> {
    let out = output(cmd)?;
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    if out.status.success() {
        Ok(stdout)
    } else {
        bail!(
            "{}\n{}",
            stdout.trim_end(),
            String::from_utf8_lossy(&out.stderr).trim_end()
        )
    }
}

/// Like `run_cmd`, but returns the output even when the exit code isn't 0:
/// `pacman -Qu` exits 1 with no updates and `dnf check-update` exits 100
/// precisely when there *are* some.
pub fn run_out(cmd: &[String]) -> Result<String> {
    let out = output(cmd)?;
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

fn output(cmd: &[String]) -> Result<std::process::Output> {
    if cmd.is_empty() {
        bail!("empty command");
    }
    Ok(Command::new(program(&cmd[0])).args(&cmd[1..]).output()?)
}

/// Full path of the binary if it is in PATH, else its bare name (so the spawn
/// fails with the system's own error). It matters on Windows: `CreateProcess`
/// only tries `.exe`, so `scoop.cmd` or `pnpm.cmd` would never start.
pub fn program(name: &str) -> OsString {
    find_bin(name)
        .map(PathBuf::into_os_string)
        .unwrap_or_else(|| name.into())
}

/// Is the binary in PATH? Pure Rust scan (no `which` subprocess) and cached:
/// modules ask on every refresh (every 3 s).
pub fn has_bin(name: &str) -> bool {
    find_bin(name).is_some()
}

fn find_bin(name: &str) -> Option<PathBuf> {
    static CACHE: Mutex<Option<HashMap<String, Option<PathBuf>>>> = Mutex::new(None);
    let mut guard = CACHE.lock().unwrap();
    let cache = guard.get_or_insert_with(HashMap::new);
    if let Some(hit) = cache.get(name) {
        return hit.clone();
    }
    let found = lookup(name);
    cache.insert(name.to_string(), found.clone());
    found
}

fn lookup(name: &str) -> Option<PathBuf> {
    let paths = std::env::var_os("PATH")?;
    // On Windows a "binary" can be .exe, .cmd, .bat or .ps1, per PATHEXT.
    let exts: Vec<String> = if platform::WIN {
        std::env::var("PATHEXT")
            .unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".into())
            .split(';')
            .filter(|e| !e.is_empty())
            .map(str::to_lowercase)
            .collect()
    } else {
        Vec::new()
    };
    for dir in std::env::split_paths(&paths) {
        // On Windows an extensionless file isn't executable: Git for Windows
        // ships shell scripts (ssh-copy-id, notepad) in usr/bin that
        // CreateProcess cannot launch.
        let base = dir.join(name);
        if !platform::WIN && base.is_file() {
            return Some(base);
        }
        for e in &exts {
            let p = dir.join(format!("{name}{e}"));
            if p.is_file() {
                return Some(p);
            }
        }
    }
    None
}

/// Copies text to the clipboard: clip.exe (Windows) or wl-copy/xclip/xsel (Linux).
pub fn copy_clip(text: &str) -> Result<()> {
    // ponytail: clip.exe reads its input in the console code page; plenty for
    // pids, URLs and ASCII paths. Set-Clipboard if UTF-8 is ever needed.
    let cmd: &[&str] = if platform::WIN {
        &["clip"]
    } else if has_bin("wl-copy") {
        &["wl-copy"]
    } else if has_bin("xclip") {
        &["xclip", "-selection", "clipboard"]
    } else if has_bin("xsel") {
        &["xsel", "-ib"]
    } else {
        bail!("no clipboard tool (install wl-clipboard or xclip)");
    };
    let mut child = Command::new(program(cmd[0]))
        .args(&cmd[1..])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    child.stdin.take().unwrap().write_all(text.as_bytes())?;
    child.wait()?;
    Ok(())
}

/// Desktop notification (fire-and-forget); silent if there's nothing to use.
pub fn notify(summary: &str, body: &str) {
    let cmd = if platform::WIN {
        // Tray balloon: needs nothing installed (BurntToast and friends don't
        // ship with Windows).
        let msg = format!("{summary}: {body}").replace('\'', "''");
        platform::ps(&format!(
            "Add-Type -AssemblyName System.Windows.Forms; \
             Add-Type -AssemblyName System.Drawing; \
             $n = New-Object System.Windows.Forms.NotifyIcon; \
             $n.Icon = [System.Drawing.SystemIcons]::Information; \
             $n.Visible = $true; \
             $n.ShowBalloonTip(5000, 'DevTerminal', '{msg}', 'Info'); \
             Start-Sleep -Seconds 6; $n.Dispose()"
        ))
    } else if has_bin("notify-send") {
        vec![
            "notify-send".into(),
            "-a".into(),
            "DevTerminal".into(),
            summary.into(),
            body.into(),
        ]
    } else {
        return;
    };
    let _ = Command::new(program(&cmd[0]))
        .args(&cmd[1..])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
}
