use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::Frame;

use super::{Action, Module};
use crate::runner::{has_bin, run_cmd};
use crate::theme;
use crate::ui::{block_c, ListView};

const VIEWS: [&str; 2] = ["Logs", "Services"];

/// journalctl (Logs view) + systemd units (Services view).
pub struct Logs {
    view: usize,
    text: String,
    scroll: u16,
    errors_only: bool,
    list: ListView,
}

/// Color from the ACTIVE/SUB columns of `systemctl list-units`.
fn unit_color(active: &str, sub: &str) -> Color {
    match (active, sub) {
        ("failed", _) | (_, "failed") => theme::p().red,
        ("active", "running") => theme::p().green,
        _ => Color::DarkGray, // exited, dead, inactive...
    }
}

impl Logs {
    pub fn new() -> Self {
        Self {
            view: 0,
            text: String::new(),
            scroll: 0,
            errors_only: false,
            list: ListView::new(),
        }
    }

    fn refresh_logs(&mut self) {
        let mut cmd = vec!["journalctl".to_string(), "-n".into(), "400".into(), "--no-pager".into()];
        if self.errors_only {
            cmd.push("-p".into());
            cmd.push("err".into());
        }
        self.text = run_cmd(&cmd).unwrap_or_else(|e| e.to_string());
        // Start at the end, where the recent entries are
        self.scroll = (self.text.lines().count() as u16).saturating_sub(20);
    }

    fn refresh_services(&mut self) {
        let cmd: Vec<String> =
            ["systemctl", "list-units", "--type=service", "--no-legend", "--no-pager", "--plain"]
                .iter()
                .map(|s| s.to_string())
                .collect();
        match run_cmd(&cmd) {
            Ok(o) => {
                let rows: Vec<_> = o
                    .lines()
                    .filter(|l| !l.trim().is_empty())
                    .map(|l| {
                        let cols: Vec<&str> = l.split_whitespace().collect();
                        let unit = cols.first().copied().unwrap_or("").to_string();
                        let color = unit_color(
                            cols.get(2).copied().unwrap_or(""),
                            cols.get(3).copied().unwrap_or(""),
                        );
                        (l.to_string(), unit, Some(color))
                    })
                    .collect();
                self.list.set_items_styled(rows);
            }
            Err(e) => self.list.set_items(vec![(format!("Error: {e}"), String::new())]),
        }
    }
}

impl Module for Logs {
    fn title(&self) -> &'static str {
        "System"
    }

    fn refresh(&mut self) {
        if !has_bin("journalctl") {
            self.text = "journalctl not available".into();
            return;
        }
        match self.view {
            0 => self.refresh_logs(),
            _ => self.refresh_services(),
        }
    }

    fn draw(&mut self, f: &mut Frame, area: Rect) {
        if self.view == 1 {
            self.list.draw(f, area, "System · [systemd services]", self.accent());
            return;
        }
        let title = if self.errors_only {
            "System · [journalctl · errors only]"
        } else {
            "System · [journalctl · all]"
        };
        f.render_widget(
            Paragraph::new(self.text.as_str())
                .style(Style::default().fg(self.accent()))
                .block(block_c(title, self.accent()))
                .wrap(Wrap { trim: false })
                .scroll((self.scroll, 0)),
            area,
        );
    }

    fn on_key(&mut self, key: KeyEvent) -> Action {
        if key.code == KeyCode::Char('v') {
            self.view = (self.view + 1) % VIEWS.len();
            return Action::Refresh;
        }
        if self.view == 1 {
            if self.list.key(key) {
                return Action::Handled;
            }
            let Some(unit) = self.list.selected_id().filter(|s| !s.is_empty()) else {
                return Action::Ignored;
            };
            let sysctl = |verb: &str| {
                Action::Interactive(vec!["sudo".into(), "systemctl".into(), verb.into(), unit.clone()])
            };
            return match key.code {
                KeyCode::Enter => {
                    let out = run_cmd(&[
                        "journalctl".into(), "-u".into(), unit.clone(),
                        "-n".into(), "200".into(), "--no-pager".into(),
                    ])
                    .unwrap_or_else(|e| e.to_string());
                    Action::Show { title: format!("journalctl -u {unit}"), text: out }
                }
                KeyCode::Char('u') => sysctl("start"),
                KeyCode::Char('x') => sysctl("stop"),
                KeyCode::Char('t') => sysctl("restart"),
                _ => Action::Ignored,
            };
        }
        match key.code {
            KeyCode::Char('w') | KeyCode::Up => {
                self.scroll = self.scroll.saturating_sub(1);
                Action::Handled
            }
            KeyCode::Char('s') | KeyCode::Down => {
                self.scroll = self.scroll.saturating_add(1);
                Action::Handled
            }
            KeyCode::PageUp => {
                self.scroll = self.scroll.saturating_sub(20);
                Action::Handled
            }
            KeyCode::PageDown => {
                self.scroll = self.scroll.saturating_add(20);
                Action::Handled
            }
            KeyCode::Char('e') => {
                self.errors_only = !self.errors_only;
                Action::Refresh
            }
            _ => Action::Ignored,
        }
    }

    fn footer(&self) -> String {
        match self.view {
            0 => "v services · w/s scroll · PgUp/PgDn page · e errors only".into(),
            _ => "v logs · Enter service logs · u start · x stop · t restart".into(),
        }
    }

    fn accent(&self) -> Color {
        theme::p().blue2
    }

    fn clip(&self) -> Option<String> {
        (self.view == 1).then(|| self.list.clip()).flatten()
    }
}
