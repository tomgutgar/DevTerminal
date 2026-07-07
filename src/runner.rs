use std::collections::HashMap;
use std::io::Write;
use std::process::{Command, Stdio};
use std::sync::Mutex;

use anyhow::{bail, Result};

/// Ejecuta un comando capturando su salida. Error si el exit code no es 0.
pub fn run_cmd(cmd: &[String]) -> Result<String> {
    if cmd.is_empty() {
        bail!("comando vacío");
    }
    let out = Command::new(&cmd[0]).args(&cmd[1..]).output()?;
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

/// ¿Existe el binario en PATH? Escaneo puro en Rust (sin subproceso `which`)
/// y cacheado: los módulos preguntan en cada refresco (cada 3 s).
pub fn has_bin(name: &str) -> bool {
    static CACHE: Mutex<Option<HashMap<String, bool>>> = Mutex::new(None);
    let mut guard = CACHE.lock().unwrap();
    let cache = guard.get_or_insert_with(HashMap::new);
    if let Some(&hit) = cache.get(name) {
        return hit;
    }
    let found = std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).any(|d| d.join(name).is_file()))
        .unwrap_or(false);
    cache.insert(name.to_string(), found);
    found
}

/// Copia texto al portapapeles vía wl-copy (Wayland) o xclip/xsel (X11).
pub fn copy_clip(text: &str) -> Result<()> {
    let cmd: &[&str] = if has_bin("wl-copy") {
        &["wl-copy"]
    } else if has_bin("xclip") {
        &["xclip", "-selection", "clipboard"]
    } else if has_bin("xsel") {
        &["xsel", "-ib"]
    } else {
        bail!("sin herramienta de portapapeles (instala wl-clipboard o xclip)");
    };
    let mut child = Command::new(cmd[0])
        .args(&cmd[1..])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    child.stdin.take().unwrap().write_all(text.as_bytes())?;
    child.wait()?;
    Ok(())
}

/// Notificación de escritorio (fire-and-forget); silencioso si no hay notify-send.
pub fn notify(summary: &str, body: &str) {
    if has_bin("notify-send") {
        let _ = Command::new("notify-send")
            .args(["-a", "DevTerminal", summary, body])
            .spawn();
    }
}
