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

/// Servidor remoto definido por el usuario en config.toml.
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

fn config_path() -> Option<PathBuf> {
    let base = env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("devterminal").join("config.toml"))
}

pub fn load() -> Result<Config> {
    match config_path().and_then(|p| fs::read_to_string(p).ok()) {
        Some(s) => Ok(toml::from_str(&s)?),
        None => Ok(Config::default()),
    }
}

/// Expande el prefijo `~` a $HOME.
pub fn expand_home(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~") {
        if let Some(home) = env::var_os("HOME") {
            return PathBuf::from(home).join(rest.trim_start_matches('/'));
        }
    }
    PathBuf::from(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parsea_config_y_aplica_defaults() {
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
