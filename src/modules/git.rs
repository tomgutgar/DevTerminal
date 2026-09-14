use std::path::PathBuf;

use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::Frame;

use super::{Action, Module};
use crate::config::expand_home;
use crate::platform;
use crate::runner::{has_bin, run_cmd};
use crate::theme;
use crate::ui::ListView;

const VIEWS: [&str; 3] = ["Status", "Log", "Branches"];

pub struct GitMod {
    repo: Option<PathBuf>,
    view: usize,
    list: ListView,
    workspaces: Vec<String>,
    branch: String,
}

/// Detects the repo of the current directory by walking up to the root. Never scans the disk.
fn detect_repo() -> Option<PathBuf> {
    let mut dir = std::env::current_dir().ok()?;
    loop {
        if dir.join(".git").exists() {
            return Some(dir);
        }
        if !dir.pop() {
            return None;
        }
    }
}

/// Color by state of a `git status --porcelain` line.
fn status_color(line: &str) -> Color {
    let mut chars = line.chars();
    let x = chars.next().unwrap_or(' ');
    let y = chars.next().unwrap_or(' ');
    if x == '?' && y == '?' {
        Color::DarkGray
    } else if x == 'D' || y == 'D' {
        theme::p().red // deleted
    } else if x != ' ' {
        theme::p().green // staged
    } else {
        theme::p().yellow // modified, unstaged
    }
}

/// `git status --porcelain` line → (staged, path).
pub fn parse_porcelain(line: &str) -> Option<(bool, String)> {
    if line.len() < 4 {
        return None;
    }
    let x = line.chars().next()?;
    let mut path = line[3..].to_string();
    if let Some((_, new)) = path.split_once(" -> ") {
        path = new.to_string(); // renames
    }
    Some((x != ' ' && x != '?', path))
}

impl GitMod {
    pub fn new(workspaces: Vec<String>) -> Self {
        Self {
            repo: None,
            view: 0,
            list: ListView::new(),
            workspaces,
            branch: String::new(),
        }
    }

    fn git(&self, args: &[&str]) -> Vec<String> {
        let repo = self.repo.as_ref().map(|p| p.display().to_string()).unwrap_or_default();
        let mut v = vec!["git".to_string(), "-C".to_string(), repo];
        v.extend(args.iter().map(|s| s.to_string()));
        v
    }

    /// No repo: offers the repos in the configured workspaces (one level only).
    fn load_picker(&mut self) {
        let mut rows = Vec::new();
        for ws in &self.workspaces {
            let base = expand_home(ws);
            if base.join(".git").exists() {
                rows.push((ws.clone(), base.display().to_string()));
            }
            if let Ok(entries) = std::fs::read_dir(&base) {
                for e in entries.flatten() {
                    let p = e.path();
                    if p.join(".git").exists() {
                        rows.push((format!("{ws}/{}", e.file_name().to_string_lossy()), p.display().to_string()));
                    }
                }
            }
        }
        if rows.is_empty() {
            rows.push((
                "No Git repository in this directory (and no workspaces with repos in config.toml)".into(),
                String::new(),
            ));
        }
        self.list.set_items(rows);
    }
}

impl Module for GitMod {
    fn title(&self) -> &'static str {
        "Git"
    }

    fn refresh(&mut self) {
        if self.repo.is_none() {
            self.repo = detect_repo();
        }
        let Some(_) = self.repo else {
            self.load_picker();
            return;
        };
        self.branch = run_cmd(&self.git(&["rev-parse", "--abbrev-ref", "HEAD"]))
            .map(|s| s.trim().to_string())
            .unwrap_or_default();
        let rows: Result<Vec<(String, String, Option<Color>)>, _> = match self.view {
            0 => run_cmd(&self.git(&["status", "--porcelain"]))
                .map(|o| {
                    let rows: Vec<_> = o
                        .lines()
                        .filter_map(|l| parse_porcelain(l).map(|(staged, path)| {
                            let mark = if staged { "●" } else { " " };
                            (format!("{mark} {l}"), path, Some(status_color(l)))
                        }))
                        .collect();
                    if rows.is_empty() {
                        vec![("Working tree clean ✓".to_string(), String::new(), None)]
                    } else {
                        rows
                    }
                }),
            1 => run_cmd(&self.git(&["log", "--oneline", "--graph", "--decorate", "-60"]))
                .map(|o| o.lines().map(|l| (l.to_string(), String::new(), None)).collect()),
            _ => run_cmd(&self.git(&["branch", "-a"])).map(|o| {
                o.lines()
                    .map(|l| {
                        let name = l.trim_start_matches("* ").trim().to_string();
                        (l.to_string(), name, None)
                    })
                    .collect()
            }),
        };
        match rows {
            Ok(r) => self.list.set_items_styled(r),
            Err(e) => self.list.set_items(vec![(format!("Error: {e}"), String::new())]),
        }
    }

    fn draw(&mut self, f: &mut Frame, area: Rect) {
        let title = match &self.repo {
            Some(p) => format!("Git · {} · branch {} · [{}]", p.display(), self.branch, VIEWS[self.view]),
            None => "Git · choose a repository".to_string(),
        };
        self.list.draw(f, area, &title, self.accent());
    }

    fn on_key(&mut self, key: KeyEvent) -> Action {
        if self.list.key(key) {
            return Action::Handled;
        }
        // Repo picker mode
        if self.repo.is_none() {
            if key.code == KeyCode::Enter {
                if let Some(id) = self.list.selected_id() {
                    if !id.is_empty() {
                        self.repo = Some(PathBuf::from(id));
                        self.refresh();
                    }
                }
                return Action::Handled;
            }
            return Action::Ignored;
        }
        match key.code {
            KeyCode::Char('v') => {
                self.view = (self.view + 1) % VIEWS.len();
                Action::Refresh
            }
            KeyCode::Char('o') => {
                self.repo = None;
                self.load_picker();
                Action::Handled
            }
            KeyCode::Char('e') => Action::Run { cmd: self.git(&["fetch", "--all"]), confirm: None, show: true },
            KeyCode::Char('p') => Action::Run { cmd: self.git(&["pull"]), confirm: None, show: true },
            KeyCode::Char('P') => Action::Run { cmd: self.git(&["push"]), confirm: None, show: true },
            KeyCode::Char('c') => Action::Prompt {
                label: "Commit message".into(),
                template: self.git(&["commit", "-m", "{}"]),
                interactive: false,
                show: true,
            },
            KeyCode::Char(' ') if self.view == 0 => {
                // stage/unstage the selected file based on its actual state
                let Some(path) = self.list.selected_id().filter(|s| !s.is_empty()) else {
                    return Action::Handled;
                };
                let is_staged = run_cmd(&self.git(&["status", "--porcelain", "--", &path]))
                    .ok()
                    .and_then(|o| o.lines().next().and_then(parse_porcelain).map(|(s, _)| s))
                    .unwrap_or(false);
                let cmd = if is_staged {
                    self.git(&["restore", "--staged", "--", &path])
                } else {
                    self.git(&["add", "--", &path])
                };
                Action::Run { cmd, confirm: None, show: false }
            }
            KeyCode::Char('E') if self.view == 0 => {
                let Some(path) = self.list.selected_id().filter(|s| !s.is_empty()) else {
                    return Action::Handled;
                };
                let full = self.repo.as_ref().map(|r| r.join(&path)).unwrap_or_default();
                Action::Interactive(vec![platform::editor(), full.display().to_string()])
            }
            KeyCode::Enter => match self.view {
                0 => {
                    let Some(path) = self.list.selected_id().filter(|s| !s.is_empty()) else {
                        return Action::Handled;
                    };
                    // With delta installed, full-screen diff with color and a pager.
                    if has_bin("delta") {
                        let repo = self.repo.as_ref().map(|p| p.display().to_string()).unwrap_or_default();
                        return Action::Interactive(platform::diff_delta(&repo, &path));
                    }
                    let diff = run_cmd(&self.git(&["diff", "HEAD", "--", &path]))
                        .unwrap_or_else(|e| e.to_string());
                    Action::Show { title: format!("diff {path}"), text: diff }
                }
                2 => {
                    let Some(name) = self.list.selected_id().filter(|s| !s.is_empty()) else {
                        return Action::Handled;
                    };
                    let name = name.trim_start_matches("remotes/origin/").to_string();
                    Action::Run { cmd: self.git(&["checkout", &name]), confirm: None, show: true }
                }
                _ => Action::Handled,
            },
            _ => Action::Ignored,
        }
    }

    fn footer(&self) -> String {
        if self.repo.is_none() {
            "Enter open repo".into()
        } else {
            "v view · Space stage · Enter diff/checkout · E edit · c commit · p pull · P push · e fetch · o other repo".into()
        }
    }

    fn accent(&self) -> Color {
        theme::p().green
    }

    fn clip(&self) -> Option<String> {
        self.list.clip()
    }
}

#[cfg(test)]
mod tests {
    use super::parse_porcelain;

    #[test]
    fn porcelain() {
        assert_eq!(parse_porcelain(" M src/main.rs"), Some((false, "src/main.rs".into())));
        assert_eq!(parse_porcelain("M  src/main.rs"), Some((true, "src/main.rs".into())));
        assert_eq!(parse_porcelain("?? new.txt"), Some((false, "new.txt".into())));
        assert_eq!(parse_porcelain("R  old.txt -> new.txt"), Some((true, "new.txt".into())));
        assert_eq!(parse_porcelain(""), None);
    }
}
