use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::Frame;
use sysinfo::{ProcessesToUpdate, System};

use super::{Action, Module};
use crate::platform;
use crate::theme;
use crate::ui::ListView;


pub struct Procs {
    sys: System,
    list: ListView,
}

impl Procs {
    pub fn new() -> Self {
        Self { sys: System::new(), list: ListView::new() }
    }
}

impl Module for Procs {
    fn title(&self) -> &'static str {
        "Processes"
    }

    fn refresh(&mut self) {
        self.sys.refresh_processes(ProcessesToUpdate::All, true);
        let mut procs: Vec<_> = self.sys.processes().values().collect();
        procs.sort_by(|a, b| b.cpu_usage().total_cmp(&a.cpu_usage()));
        let rows: Vec<_> = procs
            .iter()
            .take(400)
            .map(|p| {
                (
                    format!(
                        "{:>7}  {:>5.1}%  {:>8.1} MiB  {}",
                        p.pid(),
                        p.cpu_usage(),
                        p.memory() as f64 / 1024.0 / 1024.0,
                        p.name().to_string_lossy()
                    ),
                    p.pid().to_string(),
                )
            })
            .collect();
        self.list.set_items(rows);
    }

    fn draw(&mut self, f: &mut Frame, area: Rect) {
        self.list.draw(f, area, "Processes · by CPU (/ filters)", self.accent());
    }

    fn on_key(&mut self, key: KeyEvent) -> Action {
        if self.list.key(key) {
            return Action::Handled;
        }
        let Some(pid) = self.list.selected_id().filter(|s| !s.is_empty()) else {
            return Action::Ignored;
        };
        match key.code {
            KeyCode::Char('k') => Action::Run {
                cmd: platform::kill(&pid, false),
                confirm: Some(format!("Kill process {pid}?")),
                show: false,
            },
            KeyCode::Char('K') => Action::Run {
                cmd: platform::kill(&pid, true),
                confirm: Some(format!("Force-kill process {pid}?")),
                show: false,
            },
            KeyCode::Char('n') => {
                let (label, template) = platform::renice(&pid);
                Action::Prompt { label, template, interactive: false, show: true }
            }
            _ => Action::Ignored,
        }
    }

    fn footer(&self) -> String {
        if platform::WIN { "k kill · K force · n priority" } else { "k kill · K SIGKILL · n renice" }.into()
    }

    fn accent(&self) -> Color {
        theme::p().red
    }

    fn clip(&self) -> Option<String> {
        self.list.clip()
    }
}
