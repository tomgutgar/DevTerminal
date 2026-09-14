use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::Frame;

use super::{Action, Module};
use crate::platform;
use crate::runner::{has_bin, run_out};
use crate::theme;
use crate::ui::ListView;

/// A package manager. Templates carry `{}` where the package name goes.
struct Pm {
    bin: &'static str,
    /// Its write operations need sudo (never the search).
    sudo: bool,
    /// Ways to list pending updates, in order of preference: the first one that
    /// exists and answers wins.
    updates: &'static [&'static [&'static str]],
    upgrade: &'static [&'static str],
    search: &'static [&'static str],
    install: &'static [&'static str],
    remove: &'static [&'static str],
    /// Cache or source cleanup; empty = this manager has no equivalent.
    clean: &'static [&'static str],
    /// The `updates` output is a table: drop everything up to the dashes row.
    table: bool,
    /// In tables, minimum columns of a real row: drops the trailing summary
    /// ("10 upgrades available.", translated to the Windows display language).
    min_cols: usize,
    /// Noise line prefixes in the `updates` output.
    noise: &'static [&'static str],
}

/// Supported managers, in detection order. The first one on PATH wins: on Arch
/// paru/yay beat pacman (they cover the AUR) and on Windows only one of the
/// last three can exist.
const PMS: &[Pm] = &[
    Pm {
        bin: "paru",
        sudo: false, // AUR helpers ask for sudo themselves when needed
        updates: &[&["checkupdates"], &["pacman", "-Qu"]],
        upgrade: &["paru", "-Syu"],
        search: &["paru", "-Ss", "{}"],
        install: &["paru", "-S", "{}"],
        remove: &["paru", "-Rns", "{}"],
        clean: &["paru", "-Sc"],
        table: false,
        min_cols: 0,
        noise: &[],
    },
    Pm {
        bin: "yay",
        sudo: false,
        updates: &[&["checkupdates"], &["pacman", "-Qu"]],
        upgrade: &["yay", "-Syu"],
        search: &["yay", "-Ss", "{}"],
        install: &["yay", "-S", "{}"],
        remove: &["yay", "-Rns", "{}"],
        clean: &["yay", "-Sc"],
        table: false,
        min_cols: 0,
        noise: &[],
    },
    Pm {
        bin: "pacman",
        sudo: true,
        updates: &[&["checkupdates"], &["pacman", "-Qu"]],
        upgrade: &["pacman", "-Syu"],
        search: &["pacman", "-Ss", "{}"],
        install: &["pacman", "-S", "{}"],
        remove: &["pacman", "-Rns", "{}"],
        clean: &["pacman", "-Sc"],
        table: false,
        min_cols: 0,
        noise: &[],
    },
    Pm {
        bin: "apt",
        sudo: true,
        // `apt list --upgradable` reads the already-downloaded index: no root.
        updates: &[&["apt", "list", "--upgradable"]],
        upgrade: &["apt", "upgrade"],
        search: &["apt", "search", "{}"],
        install: &["apt", "install", "{}"],
        remove: &["apt", "remove", "--purge", "{}"],
        clean: &["apt", "autoremove", "--purge"],
        table: false,
        min_cols: 0,
        noise: &["Listing", "WARNING", "N:", "W:"],
    },
    Pm {
        bin: "dnf",
        sudo: true,
        // dnf check-update exits 100 exactly when there are updates: run_out.
        updates: &[&["dnf", "check-update"]],
        upgrade: &["dnf", "upgrade"],
        search: &["dnf", "search", "{}"],
        install: &["dnf", "install", "{}"],
        remove: &["dnf", "remove", "{}"],
        clean: &["dnf", "clean", "all"],
        table: false,
        min_cols: 0,
        noise: &["Last metadata", "Obsoleting", "Security:", "Updating "],
    },
    Pm {
        bin: "zypper",
        sudo: true,
        updates: &[&["zypper", "--no-refresh", "list-updates"]],
        upgrade: &["zypper", "update"],
        search: &["zypper", "search", "{}"],
        install: &["zypper", "install", "{}"],
        remove: &["zypper", "remove", "{}"],
        clean: &["zypper", "clean"],
        table: true,
        min_cols: 4,
        noise: &[],
    },
    Pm {
        bin: "apk",
        sudo: true,
        updates: &[&["apk", "list", "-u"]],
        upgrade: &["apk", "upgrade"],
        search: &["apk", "search", "{}"],
        install: &["apk", "add", "{}"],
        remove: &["apk", "del", "{}"],
        clean: &["apk", "cache", "clean"],
        table: false,
        min_cols: 0,
        noise: &[],
    },
    Pm {
        bin: "winget",
        sudo: false, // winget raises UAC on its own when a package needs it
        updates: &[&["winget", "upgrade", "--include-unknown"]],
        upgrade: &["winget", "upgrade", "--all", "--include-unknown"],
        search: &["winget", "search", "{}"],
        install: &["winget", "install", "{}"],
        remove: &["winget", "uninstall", "{}"],
        clean: &["winget", "source", "update"],
        table: true,
        min_cols: 4,
        noise: &[],
    },
    Pm {
        bin: "scoop",
        sudo: false,
        updates: &[&["scoop", "status"]],
        upgrade: &["scoop", "update", "*"],
        search: &["scoop", "search", "{}"],
        install: &["scoop", "install", "{}"],
        remove: &["scoop", "uninstall", "{}"],
        clean: &["scoop", "cleanup", "*"],
        table: true,
        min_cols: 3,
        noise: &[],
    },
    Pm {
        bin: "choco",
        sudo: false, // choco already runs elevated or raises UAC
        updates: &[&["choco", "outdated", "-r"]],
        upgrade: &["choco", "upgrade", "all", "-y"],
        search: &["choco", "search", "{}"],
        install: &["choco", "install", "{}", "-y"],
        remove: &["choco", "uninstall", "{}", "-y"],
        clean: &[],
        table: false,
        min_cols: 0,
        noise: &["Chocolatey v"],
    },
];

/// Separator row of the winget/scoop/zypper tables (`-----`, `--+--`).
/// The lone "-" of winget's progress spinner doesn't count.
fn is_separator(line: &str) -> bool {
    let l = line.trim();
    l.len() >= 3 && l.chars().all(|c| c == '-' || c == '+' || c == ' ')
}

/// Raw `updates` output → rows of (line, package name).
fn clean(pm: &Pm, out: &str) -> Vec<(String, String)> {
    let lines: Vec<&str> = out.lines().map(|l| l.trim_end_matches('\r')).collect();
    let rest: &[&str] = if pm.table {
        // No dashes row means no table, hence no updates.
        match lines.iter().position(|l| is_separator(l)) {
            Some(i) => &lines[i + 1..],
            None => &[],
        }
    } else {
        &lines
    };
    rest.iter()
        .map(|l| l.trim_end())
        .filter(|l| {
            !l.trim().is_empty()
                && !is_separator(l)
                && l.split_whitespace().count() >= pm.min_cols
                && !pm.noise.iter().any(|n| l.trim_start().starts_with(n))
        })
        .map(|l| {
            let pkg = l.split_whitespace().next().unwrap_or("").to_string();
            (l.to_string(), pkg)
        })
        .collect()
}

pub struct Packages {
    list: ListView,
    pm: Option<&'static Pm>,
    detected: bool,
}

impl Packages {
    pub fn new() -> Self {
        Self { list: ListView::new(), pm: None, detected: false }
    }

    /// The manager's template, with sudo in front when needed (never on search).
    fn cmd(&self, args: &[&str]) -> Vec<String> {
        let pm = self.pm.expect("manager detected");
        let mut v: Vec<String> = Vec::new();
        if pm.sudo && !platform::WIN {
            v.push("sudo".into());
        }
        v.extend(args.iter().map(|s| s.to_string()));
        v
    }
}

impl Module for Packages {
    fn title(&self) -> &'static str {
        "Packages"
    }

    fn refresh(&mut self) {
        if !self.detected {
            self.pm = PMS.iter().find(|pm| has_bin(pm.bin));
            self.detected = true;
        }
        let Some(pm) = self.pm else {
            self.list.set_items(vec![(
                "No known package manager (pacman/paru/yay, apt, dnf, zypper, apk, winget, scoop, choco)".into(),
                String::new(),
            )]);
            return;
        };
        let rows = pm
            .updates
            .iter()
            .filter(|c| has_bin(c[0]))
            .find_map(|c| {
                let cmd: Vec<String> = c.iter().map(|s| s.to_string()).collect();
                run_out(&cmd).ok().map(|o| clean(pm, &o))
            })
            .unwrap_or_default();
        if rows.is_empty() {
            self.list.set_items(vec![("System up to date ✓".into(), String::new())]);
        } else {
            self.list.set_items(rows);
        }
    }

    fn draw(&mut self, f: &mut Frame, area: Rect) {
        let manager = self.pm.map(|p| p.bin).unwrap_or("none");
        self.list.draw(
            f,
            area,
            &format!("Packages · pending updates (manager: {manager})"),
            self.accent(),
        );
    }

    fn on_key(&mut self, key: KeyEvent) -> Action {
        if self.list.key(key) {
            return Action::Handled;
        }
        let Some(pm) = self.pm else {
            return Action::Ignored;
        };
        match key.code {
            KeyCode::Char('U') => Action::Interactive(self.cmd(pm.upgrade)),
            KeyCode::Char('b') => Action::Prompt {
                label: "Search package".into(),
                // Searching needs no privileges.
                template: pm.search.iter().map(|s| s.to_string()).collect(),
                interactive: false,
                show: true,
            },
            KeyCode::Char('i') => Action::Prompt {
                label: "Package to install".into(),
                template: self.cmd(pm.install),
                interactive: true,
                show: false,
            },
            KeyCode::Char('x') => Action::Prompt {
                label: "Package to remove".into(),
                // The manager asks for confirmation itself
                template: self.cmd(pm.remove),
                interactive: true,
                show: false,
            },
            KeyCode::Char('l') if !pm.clean.is_empty() => Action::Interactive(self.cmd(pm.clean)),
            _ => Action::Ignored,
        }
    }

    fn footer(&self) -> String {
        "U upgrade all · b search · i install · x remove · l clean cache".into()
    }

    fn accent(&self) -> Color {
        theme::p().orange
    }

    fn clip(&self) -> Option<String> {
        self.list.clip()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pm(bin: &str) -> &'static Pm {
        PMS.iter().find(|p| p.bin == bin).unwrap()
    }

    #[test]
    fn cleans_winget_table() {
        let out = "Name      Id           Version  Available   Source\n\
                   ----------------------------------------------------\n\
                   Git       Git.Git      2.55.0   2.56.0      winget\n\
                   7-Zip     7zip.7zip    24.09    25.00       winget\n\
                   2 upgrades available.\n";
        let rows = clean(pm("winget"), out);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].1, "Git");
    }

    #[test]
    fn no_table_means_no_updates() {
        // With nothing to upgrade, winget prints no table (and no dashes row).
        assert!(clean(pm("winget"), "No installed package found matching input criteria.\n").is_empty());
        // The "-" of the progress spinner is not a separator row.
        assert!(clean(pm("winget"), "-\n\\\n|\n").is_empty());
    }

    #[test]
    fn cleans_apt_and_dnf_noise() {
        let rows = clean(pm("apt"), "Listing... Done\nvim/stable 2:9.1 amd64 [upgradable from: 2:9.0]\n");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].1, "vim/stable");

        let out = "Last metadata expiration check: 0:12:01 ago.\n\nkernel.x86_64   6.11.4-201   updates\n";
        let rows = clean(pm("dnf"), out);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].1, "kernel.x86_64");
    }
}
