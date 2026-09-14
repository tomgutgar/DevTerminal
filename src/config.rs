use std::{env, fs, path::PathBuf};

use anyhow::Result;
use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
#[serde(default)]
pub struct Config {
    pub theme: String,
    pub workspaces: Vec<String>,
    pub servers: Vec<Server>,
}

/// Remote server defined by the user in config.toml.
#[derive(Debug, Deserialize, Clone)]
pub struct Server {
    pub alias: String,
    pub host: String,
    #[serde(default)]
    pub user: Option<String>,
    #[serde(default)]
    pub port: Option<u16>,
    #[serde(default)]
    pub desc: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            theme: "dark".into(),
            workspaces: Vec::new(),
            servers: Vec::new(),
        }
    }
}

/// `%APPDATA%\devterminal\config.toml` on Windows,
/// `~/.config/devterminal/config.toml` on Linux.
fn config_path() -> Option<PathBuf> {
    let base = if crate::platform::WIN {
        env::var_os("APPDATA").map(PathBuf::from)
    } else {
        env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| home().map(|h| h.join(".config")))
    }?;
    Some(base.join("devterminal").join("config.toml"))
}

/// The user's home directory: $HOME on Linux, %USERPROFILE% on Windows.
pub fn home() -> Option<PathBuf> {
    env::var_os("HOME")
        .or_else(|| env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

pub fn load() -> Result<Config> {
    match config_path().and_then(|p| fs::read_to_string(p).ok()) {
        Some(s) => Ok(toml::from_str(&s)?),
        None => Ok(Config::default()),
    }
}

/// Expands the `~` prefix to the user's home directory.
pub fn expand_home(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~") {
        if let Some(home) = home() {
            return home.join(rest.trim_start_matches(['/', '\\']));
        }
    }
    PathBuf::from(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_config_and_applies_defaults() {
        let c: Config = toml::from_str(
            r#"
workspaces = ["~/Projects"]

[[servers]]
alias = "vps"
host = "1.2.3.4"
user = "root"
"#,
        )
        .unwrap();
        assert_eq!(c.theme, "dark");
        assert_eq!(c.workspaces, ["~/Projects"]);
        assert_eq!(c.servers[0].alias, "vps");
        assert_eq!(c.servers[0].port, None);
    }
}
