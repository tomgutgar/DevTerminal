mod config;
mod modules;
mod runner;
mod theme;
mod ui;

use std::time::{Duration, Instant};

/// Auto-refresh cadence for the active module (dashboard, ports, docker...).
const AUTO_REFRESH: Duration = Duration::from_secs(3);

use anyhow::Result;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind};
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph, Tabs, Wrap};
use ratatui::{DefaultTerminal, Frame};

use modules::{Action, Module};
use ui::{block, block_c, centered};

enum Overlay {
    Confirm { msg: String, cmd: Vec<String>, show: bool },
    Prompt { label: String, template: Vec<String>, interactive: bool, show: bool, input: String },
    Pane { title: String, text: String, scroll: u16 },
}

struct App {
    modules: Vec<Box<dyn Module>>,
    active: usize,
    overlay: Option<Overlay>,
    last_refresh: Instant,
    /// Refresh after drawing the frame: the key responds instantly and the
    /// command (docker, gh, kubectl...) runs with the ⟳ already on screen.
    pending_refresh: bool,
    /// Ephemeral notice in the footer (e.g. "Copied"); cleared on the next key press.
    notice: Option<String>,
}

fn main() -> Result<()> {
    let cfg = config::load()?;
    theme::init(&cfg.theme);
    let mut terminal = ratatui::init();
    let res = App::new(&cfg).run(&mut terminal);
    ratatui::restore();
    res
}

impl App {
    fn new(cfg: &config::Config) -> Self {
        let modules = modules::all(cfg);
        Self {
            modules,
            active: 0,
            overlay: None,
            last_refresh: Instant::now(),
            pending_refresh: true,
            notice: None,
        }
    }

    /// Refreshes the active module and restarts the auto-refresh clock.
    fn refresh_active(&mut self) {
        self.modules[self.active].refresh();
        self.last_refresh = Instant::now();
    }

    fn run(&mut self, terminal: &mut DefaultTerminal) -> Result<()> {
        loop {
            terminal.draw(|f| self.draw(f))?;
            // Deferred refresh: the frame with the new view is already painted.
            if self.pending_refresh {
                self.pending_refresh = false;
                self.refresh_active();
                continue;
            }
            if !event::poll(Duration::from_millis(250))? {
                if self.overlay.is_none() && self.last_refresh.elapsed() >= AUTO_REFRESH {
                    self.refresh_active();
                }
                continue;
            }
            let Event::Key(key) = event::read()? else { continue };
            if key.kind != KeyEventKind::Press {
                continue;
            }
            self.notice = None;
            if self.overlay.is_some() {
                self.overlay_key(key, terminal);
                continue;
            }
            match self.modules[self.active].on_key(key) {
                Action::Ignored => {
                    if self.global_key(key) {
                        return Ok(());
                    }
                }
                Action::Handled => {}
                Action::Refresh => self.pending_refresh = true,
                Action::Run { cmd, confirm: Some(msg), show } => {
                    self.overlay = Some(Overlay::Confirm { msg, cmd, show });
                }
                Action::Run { cmd, confirm: None, show } => self.exec(&cmd, show),
                Action::Interactive(cmd) => self.run_interactive(terminal, &cmd),
                Action::Prompt { label, template, interactive, show } => {
                    self.overlay = Some(Overlay::Prompt {
                        label,
                        template,
                        interactive,
                        show,
                        input: String::new(),
                    });
                }
                Action::Show { title, text } => {
                    self.overlay = Some(Overlay::Pane { title, text, scroll: 0 });
                }
            }
        }
    }

    /// true = quit the application.
    fn global_key(&mut self, key: KeyEvent) -> bool {
        let n = self.modules.len();
        let prev = self.active;
        match key.code {
            KeyCode::Char('q') | KeyCode::Char('Q') => return true,
            KeyCode::Tab | KeyCode::Char('d') | KeyCode::Char('D') => {
                self.active = (self.active + 1) % n;
            }
            KeyCode::BackTab | KeyCode::Char('a') | KeyCode::Char('A') => {
                self.active = (self.active + n - 1) % n;
            }
            KeyCode::Char('r') | KeyCode::Char('R') => self.pending_refresh = true,
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                if let Some(text) = self.modules[self.active].clip() {
                    self.notice = Some(match runner::copy_clip(&text) {
                        Ok(()) => format!("Copied: {text}"),
                        Err(e) => format!("Clipboard: {e}"),
                    });
                }
            }
            KeyCode::Char(c @ '1'..='9') => {
                let i = c as usize - '1' as usize;
                if i < n {
                    self.active = i;
                }
            }
            _ => {}
        }
        if self.active != prev {
            // New module on screen instantly; its data arrives right after.
            self.pending_refresh = true;
        }
        false
    }

    fn overlay_key(&mut self, key: KeyEvent, terminal: &mut DefaultTerminal) {
        let Some(ov) = self.overlay.take() else { return };
        match ov {
            Overlay::Pane { title, text, scroll } => {
                let max = text.lines().count() as u16;
                let s = match key.code {
                    KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('a') | KeyCode::Enter => return,
                    KeyCode::Char('w') | KeyCode::Up => scroll.saturating_sub(1),
                    KeyCode::Char('s') | KeyCode::Down => (scroll + 1).min(max),
                    KeyCode::PageUp => scroll.saturating_sub(20),
                    KeyCode::PageDown => (scroll + 20).min(max),
                    _ => scroll,
                };
                self.overlay = Some(Overlay::Pane { title, text, scroll: s });
            }
            Overlay::Confirm { msg, cmd, show } => match key.code {
                KeyCode::Enter | KeyCode::Char('y') | KeyCode::Char('Y') => self.exec(&cmd, show),
                KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('N') => {}
                _ => self.overlay = Some(Overlay::Confirm { msg, cmd, show }),
            },
            Overlay::Prompt { label, template, interactive, show, mut input } => match key.code {
                KeyCode::Esc => {}
                KeyCode::Enter => {
                    if input.is_empty() {
                        return;
                    }
                    let cmd: Vec<String> =
                        template.iter().map(|a| a.replace("{}", &input)).collect();
                    if interactive {
                        self.run_interactive(terminal, &cmd);
                    } else {
                        self.exec(&cmd, show);
                    }
                }
                KeyCode::Backspace => {
                    input.pop();
                    self.overlay = Some(Overlay::Prompt { label, template, interactive, show, input });
                }
                KeyCode::Char(c) => {
                    input.push(c);
                    self.overlay = Some(Overlay::Prompt { label, template, interactive, show, input });
                }
                _ => self.overlay = Some(Overlay::Prompt { label, template, interactive, show, input }),
            },
        }
    }

    /// Runs capturing output; shows a pane if `show` or on failure. Refreshes the module.
    /// If it took long enough for the user to have switched windows, notify.
    fn exec(&mut self, cmd: &[String], show: bool) {
        let start = Instant::now();
        match runner::run_cmd(cmd) {
            Ok(out) => {
                if show {
                    let text = if out.trim().is_empty() { "OK ✓".into() } else { out };
                    self.overlay = Some(Overlay::Pane { title: cmd.join(" "), text, scroll: 0 });
                }
            }
            Err(e) => {
                self.overlay = Some(Overlay::Pane {
                    title: format!("Error · {}", cmd.join(" ")),
                    text: e.to_string(),
                    scroll: 0,
                });
            }
        }
        if start.elapsed() > Duration::from_secs(10) {
            runner::notify("Command finished", &cmd.join(" "));
        }
        self.refresh_active();
    }

    /// Suspends the TUI and hands the full terminal over to the command (ssh, yazi, btop...).
    fn run_interactive(&mut self, terminal: &mut DefaultTerminal, cmd: &[String]) {
        if cmd.is_empty() {
            return;
        }
        ratatui::restore();
        let status = std::process::Command::new(&cmd[0]).args(&cmd[1..]).status();
        *terminal = ratatui::init();
        let _ = terminal.clear();
        if let Err(e) = status {
            self.overlay = Some(Overlay::Pane {
                title: "Error".into(),
                text: format!("{}: {e}", cmd[0]),
                scroll: 0,
            });
        }
        self.refresh_active();
    }

    fn draw(&mut self, f: &mut Frame) {
        let [header, body, footer] =
            Layout::vertical([Constraint::Length(3), Constraint::Min(0), Constraint::Length(1)])
                .areas(f.area());

        let active_accent = self.modules[self.active].accent();
        let titles: Vec<Line> = self
            .modules
            .iter()
            .enumerate()
            .map(|(i, m)| {
                let label = format!("{} {}", (i + 1) % 10, m.title());
                let style = if i == self.active {
                    Style::default().fg(m.accent()).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(m.accent())
                };
                Line::from(Span::styled(label, style))
            })
            .collect();
        let brand = if self.pending_refresh { "DevTerminal ⟳" } else { "DevTerminal" };
        f.render_widget(
            Tabs::new(titles)
                .select(self.active)
                .block(block_c(brand, active_accent)),
            header,
        );

        self.modules[self.active].draw(f, body);

        let text = match &self.notice {
            Some(n) => format!(" {n}"),
            None => {
                let hints = self.modules[self.active].footer();
                let sep = if hints.is_empty() { "" } else { " · " };
                format!(" {hints}{sep}y copy · Tab/A/D module · 1-9 jump · R refresh · Q quit")
            }
        };
        f.render_widget(
            Paragraph::new(text).style(Style::default().fg(active_accent)),
            footer,
        );

        match &self.overlay {
            Some(Overlay::Pane { title, text, scroll }) => {
                let area = centered(f.area(), 90, 85);
                f.render_widget(Clear, area);
                f.render_widget(
                    Paragraph::new(text.as_str())
                        .block(block(&format!("{title} — w/s scroll · Esc closes")))
                        .wrap(Wrap { trim: false })
                        .scroll((*scroll, 0)),
                    area,
                );
            }
            Some(Overlay::Confirm { msg, .. }) => {
                let area = centered(f.area(), 60, 25);
                f.render_widget(Clear, area);
                f.render_widget(
                    Paragraph::new(format!("{msg}\n\nEnter/y confirm · Esc/n cancel"))
                        .block(block("Confirm"))
                        .wrap(Wrap { trim: false }),
                    area,
                );
            }
            Some(Overlay::Prompt { label, input, .. }) => {
                let area = centered(f.area(), 60, 25);
                f.render_widget(Clear, area);
                f.render_widget(
                    Paragraph::new(format!("{label}\n\n> {input}▏\n\nEnter accept · Esc cancel"))
                        .block(block("Input"))
                        .wrap(Wrap { trim: false }),
                    area,
                );
            }
            None => {}
        }
    }
}
