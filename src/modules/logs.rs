use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::Frame;

use super::{Action, Module};
use crate::platform;
use crate::runner::{has_bin, run_cmd};
use crate::theme;
use crate::ui::{block_c, ListView};

const VIEWS: [&str; 2] = ["Logs", "Services"];

/// System log (Logs view) + services (Services view).
/// journalctl/systemd on Linux, the event logs and the SCM on Windows.
pub struct Logs {
    view: usize,
    text: String,
    scroll: u16,
    errors_only: bool,
    list: ListView,
}

/// Color from the unit/service state.
/// Linux: ACTIVE/SUB columns of `systemctl list-units`. Windows: `Status`.
fn unit_color(active: &str, sub: &str) -> Color {
    match (active, sub) {
        ("failed", _) | (_, "failed") => theme::p().red,
        ("active", "running") | ("Running", _) => theme::p().green,
        _ => Color::DarkGray, // exited, dead, inactive, Stopped...
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
        let cmd = if platform::WIN {
            // The System and Application logs are the practical equivalent of
            // the journal. Sorted oldest-first, the way journalctl prints them.
            let filter = if self.errors_only {
                "@{LogName='System','Application'; Level=1,2}"
            } else {
                "@{LogName='System','Application'}"
            };
            platform::ps(&format!(
                concat!(
                    "Get-WinEvent -FilterHashtable {} -MaxEvents 400 -ErrorAction SilentlyContinue | ",
                    "Sort-Object TimeCreated | ",
                    r"ForEach-Object {{ '{{0:dd/MM HH:mm:ss}} {{1,-11}} {{2,-26}} {{3}}' -f ",
                    r"$_.TimeCreated, $_.LevelDisplayName, $_.ProviderName, ($_.Message -replace '\s+',' ') }}"
                ),
                filter
            ))
        } else {
            let mut c = vec!["journalctl".to_string(), "-n".into(), "400".into(), "--no-pager".into()];
            if self.errors_only {
                c.push("-p".into());
                c.push("err".into());
            }
            c
        };
        self.text = run_cmd(&cmd).unwrap_or_else(|e| e.to_string());
        // Start at the bottom, where the recent stuff is
        self.scroll = (self.text.lines().count() as u16).saturating_sub(20);
    }

    fn refresh_services(&mut self) {
        let cmd = if platform::WIN {
            platform::ps(
                "Get-Service | Sort-Object Name | ForEach-Object \
                 { '{0,-38} {1,-10} {2}' -f $_.Name, $_.Status, $_.DisplayName }",
            )
        } else {
            ["systemctl", "list-units", "--type=service", "--no-legend", "--no-pager", "--plain"]
                .iter()
                .map(|s| s.to_string())
                .collect()
        };
        match run_cmd(&cmd) {
            Ok(o) => {
                let rows: Vec<_> = o
                    .lines()
                    .filter(|l| !l.trim().is_empty())
                    .map(|l| {
                        let cols: Vec<&str> = l.split_whitespace().collect();
                        let unit = cols.first().copied().unwrap_or("").to_string();
                        // systemctl: UNIT LOAD ACTIVE SUB · Get-Service: Name Status DisplayName
                        let (a, b) = if platform::WIN { (1, 1) } else { (2, 3) };
                        let color = unit_color(
                            cols.get(a).copied().unwrap_or(""),
                            cols.get(b).copied().unwrap_or(""),
                        );
                        (l.to_string(), unit, Some(color))
                    })
                    .collect();
                self.list.set_items_styled(rows);
            }
            Err(e) => self.list.set_items(vec![(format!("Error: {e}"), String::new())]),
        }
    }

    /// Log of one unit: `journalctl -u` on Linux; on Windows, the service entry
    /// plus whatever the Service Control Manager recorded about it (there is no
    /// per-unit journal).
    fn unit_logs(&self, unit: &str) -> Action {
        let cmd = if platform::WIN {
            platform::ps(&format!(
                concat!(
                    "Get-Service -Name '{0}' | Format-List Name,DisplayName,Status,StartType; ",
                    "Get-WinEvent -FilterHashtable @{{LogName='System'; ProviderName='Service Control Manager'}} ",
                    "-MaxEvents 400 -ErrorAction SilentlyContinue | ",
                    "Where-Object {{ $_.Message -like '*{0}*' }} | Sort-Object TimeCreated | ",
                    r"ForEach-Object {{ '{{0:dd/MM HH:mm:ss}}  {{1}}' -f $_.TimeCreated, ($_.Message -replace '\s+',' ') }}"
                ),
                unit.replace('\'', "''")
            ))
        } else {
            vec![
                "journalctl".into(), "-u".into(), unit.to_string(),
                "-n".into(), "200".into(), "--no-pager".into(),
            ]
        };
        let title = cmd.join(" ");
        let text = run_cmd(&cmd).unwrap_or_else(|e| e.to_string());
        Action::Show {
            title: if platform::WIN { format!("Service {unit}") } else { title },
            text,
        }
    }

    /// start/stop/restart of the service, with sudo (Linux) or UAC (Windows).
    fn service_cmd(&self, verb: &str, unit: &str) -> Action {
        let cmd = if platform::WIN {
            let cmdlet = match verb {
                "start" => "Start-Service",
                "stop" => "Stop-Service",
                _ => "Restart-Service",
            };
            platform::elevate(&[cmdlet, "-Name", &format!("'{}'", unit.replace('\'', "''"))])
        } else {
            platform::elevate(&["systemctl", verb, unit])
        };
        Action::Interactive(cmd)
    }
}

impl Module for Logs {
    fn title(&self) -> &'static str {
        "System"
    }

    fn refresh(&mut self) {
        if !platform::WIN && !has_bin("journalctl") {
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
            let t = if platform::WIN {
                "System · [Windows services]"
            } else {
                "System · [systemd services]"
            };
            self.list.draw(f, area, t, self.accent());
            return;
        }
        let source = if platform::WIN { "event log" } else { "journalctl" };
        let title = if self.errors_only {
            format!("System · [{source} · errors only]")
        } else {
            format!("System · [{source} · all]")
        };
        f.render_widget(
            Paragraph::new(self.text.as_str())
                .style(Style::default().fg(self.accent()))
                .block(block_c(&title, self.accent()))
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
            return match key.code {
                KeyCode::Enter => self.unit_logs(&unit),
                KeyCode::Char('u') => self.service_cmd("start", &unit),
                KeyCode::Char('x') => self.service_cmd("stop", &unit),
                KeyCode::Char('t') => self.service_cmd("restart", &unit),
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
            _ => "v logs · Enter service log · u start · x stop · t restart".into(),
        }
    }

    fn accent(&self) -> Color {
        theme::p().blue2
    }

    fn clip(&self) -> Option<String> {
        (self.view == 1).then(|| self.list.clip()).flatten()
    }
}
