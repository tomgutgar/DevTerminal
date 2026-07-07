use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::Frame;

use super::{Action, Module};
use crate::runner::{has_bin, run_cmd};
use crate::theme;
use crate::ui::ListView;

const VIEWS: [&str; 5] = ["Containers", "Images", "Volumes", "Networks", "Compose"];

pub struct Docker {
    view: usize,
    list: ListView,
}

impl Docker {
    pub fn new() -> Self {
        Self { view: 0, list: ListView::new() }
    }
}

fn d(args: &[&str]) -> Vec<String> {
    let mut v = vec!["docker".to_string()];
    v.extend(args.iter().map(|s| s.to_string()));
    v
}

/// `docker compose ls --all` line → (project, status, compose file).
/// ponytail: if there are several files (comma-separated) the first one is used.
pub fn parse_compose_line(line: &str) -> Option<(String, String, String)> {
    let cols: Vec<&str> = line.split_whitespace().collect();
    if cols.len() < 3 || cols[0] == "NAME" {
        return None;
    }
    let file = cols[2].split(',').next().unwrap_or("").to_string();
    Some((cols[0].to_string(), cols[1].to_string(), file))
}

impl Module for Docker {
    fn title(&self) -> &'static str {
        "Docker"
    }

    fn refresh(&mut self) {
        if !has_bin("docker") {
            self.list.set_items(vec![("docker is not installed".into(), String::new())]);
            return;
        }
        if self.view == 4 {
            let rows: Vec<_> = run_cmd(&d(&["compose", "ls", "--all"]))
                .map(|o| {
                    o.lines()
                        .filter_map(parse_compose_line)
                        .map(|(name, status, file)| {
                            let color = if status.starts_with("running") {
                                theme::p().green
                            } else {
                                Color::DarkGray
                            };
                            // id = compose file: it's what up/down/logs need
                            (format!("{name:<24} {status:<14} {file}"), file, Some(color))
                        })
                        .collect()
                })
                .unwrap_or_else(|e| vec![(format!("Error: {e}"), String::new(), None)]);
            if rows.is_empty() {
                self.list.set_items(vec![("No compose projects".into(), String::new())]);
            } else {
                self.list.set_items_styled(rows);
            }
            return;
        }
        let cmd = match self.view {
            0 => d(&["ps", "-a", "--format", "{{.ID}}\t{{.Names}}\t{{.Status}}\t{{.Image}}"]),
            1 => d(&["images", "--format", "{{.ID}}\t{{.Repository}}:{{.Tag}}\t{{.Size}}"]),
            2 => d(&["volume", "ls", "--format", "{{.Name}}\t{{.Driver}}"]),
            _ => d(&["network", "ls", "--format", "{{.ID}}\t{{.Name}}\t{{.Driver}}"]),
        };
        match run_cmd(&cmd) {
            Ok(o) => {
                let rows: Vec<_> = o
                    .lines()
                    .filter(|l| !l.trim().is_empty())
                    .map(|l| {
                        let cols: Vec<&str> = l.split('\t').collect();
                        let id = cols.first().copied().unwrap_or("").to_string();
                        // State (containers view only): Up = green, anything else = red.
                        let color = (self.view == 0)
                            .then(|| cols.get(2).copied().unwrap_or(""))
                            .map(|status| {
                                if status.starts_with("Up") {
                                    theme::p().green
                                } else {
                                    theme::p().red
                                }
                            });
                        (l.replace('\t', "  "), id, color)
                    })
                    .collect();
                if rows.is_empty() {
                    self.list.set_items(vec![("(empty)".into(), String::new())]);
                } else {
                    self.list.set_items_styled(rows);
                }
            }
            Err(e) => self.list.set_items(vec![(format!("Error: {e}"), String::new())]),
        }
    }

    fn draw(&mut self, f: &mut Frame, area: Rect) {
        self.list.draw(f, area, &format!("Docker · [{}]", VIEWS[self.view]), self.accent());
    }

    fn on_key(&mut self, key: KeyEvent) -> Action {
        if self.list.key(key) {
            return Action::Handled;
        }
        if key.code == KeyCode::Char('v') {
            self.view = (self.view + 1) % VIEWS.len();
            return Action::Refresh;
        }
        if key.code == KeyCode::Char('P') {
            return Action::Run {
                cmd: d(&["system", "prune", "-f"]),
                confirm: Some("Run docker system prune? Removes everything unused.".into()),
                show: true,
            };
        }
        let Some(id) = self.list.selected_id().filter(|s| !s.is_empty()) else {
            return Action::Ignored;
        };
        match (self.view, key.code) {
            (0, KeyCode::Char('u')) => Action::Run { cmd: d(&["start", &id]), confirm: None, show: false },
            (0, KeyCode::Char('x')) => Action::Run { cmd: d(&["stop", &id]), confirm: None, show: false },
            (0, KeyCode::Char('t')) => Action::Run { cmd: d(&["restart", &id]), confirm: None, show: false },
            (0, KeyCode::Enter) => {
                let logs = run_cmd(&d(&["logs", "--tail", "300", &id])).unwrap_or_else(|e| e.to_string());
                Action::Show { title: format!("logs {id}"), text: logs }
            }
            (0, KeyCode::Char('e')) => Action::Interactive(d(&["exec", "-it", &id, "sh"])),
            (0, KeyCode::Char('i')) => {
                let out = run_cmd(&d(&["inspect", &id])).unwrap_or_else(|e| e.to_string());
                Action::Show { title: format!("inspect {id}"), text: out }
            }
            (0, KeyCode::Char('k')) => Action::Run {
                cmd: d(&["rm", "-f", &id]),
                confirm: Some(format!("Delete container {id}?")),
                show: false,
            },
            (1, KeyCode::Char('k')) => Action::Run {
                cmd: d(&["rmi", &id]),
                confirm: Some(format!("Delete image {id}?")),
                show: true,
            },
            (2, KeyCode::Char('k')) => Action::Run {
                cmd: d(&["volume", "rm", &id]),
                confirm: Some(format!("Delete volume {id}?")),
                show: true,
            },
            (3, KeyCode::Char('k')) => Action::Run {
                cmd: d(&["network", "rm", &id]),
                confirm: Some(format!("Delete network {id}?")),
                show: true,
            },
            // Compose: id = path of the project's compose file
            (4, KeyCode::Char('u')) => Action::Run {
                cmd: d(&["compose", "-f", &id, "up", "-d"]),
                confirm: None,
                show: true,
            },
            (4, KeyCode::Char('x')) => Action::Run {
                cmd: d(&["compose", "-f", &id, "down"]),
                confirm: Some(format!("docker compose down? ({id})")),
                show: true,
            },
            (4, KeyCode::Enter) => {
                let logs = run_cmd(&d(&["compose", "-f", &id, "logs", "--tail", "200"]))
                    .unwrap_or_else(|e| e.to_string());
                Action::Show { title: format!("compose logs · {id}"), text: logs }
            }
            _ => Action::Ignored,
        }
    }

    fn footer(&self) -> String {
        match self.view {
            0 => "v view · u start · x stop · t restart · Enter logs · e shell · i inspect · k delete · P prune".into(),
            4 => "v view · u up -d · x down · Enter logs".into(),
            _ => "v view · k delete · P prune".into(),
        }
    }

    fn accent(&self) -> Color {
        theme::p().blue
    }

    fn clip(&self) -> Option<String> {
        self.list.clip()
    }
}

#[cfg(test)]
mod tests {
    use super::parse_compose_line;

    #[test]
    fn parses_compose_ls() {
        assert_eq!(parse_compose_line("NAME  STATUS  CONFIG FILES"), None);
        assert_eq!(
            parse_compose_line("myapp    running(3)    /home/x/compose.yaml"),
            Some(("myapp".into(), "running(3)".into(), "/home/x/compose.yaml".into()))
        );
        assert_eq!(
            parse_compose_line("other  exited(1)  /a/docker-compose.yml,/a/override.yml"),
            Some(("other".into(), "exited(1)".into(), "/a/docker-compose.yml".into()))
        );
    }
}
