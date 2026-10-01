mod config;
mod i18n;
mod modules;
mod platform;
mod runner;
mod theme;
mod ui;

use std::process::{Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

/// Auto-refresh cadence for the active module (dashboard, ports, docker...).
const AUTO_REFRESH: Duration = Duration::from_secs(3);

use anyhow::{bail, Result};
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
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
    /// Commits the repo it was built from is behind its upstream (see `check_updates`).
    updates: Receiver<usize>,
    behind: usize,
    /// Ctrl+U: quit, pull, reinstall and relaunch.
    update: bool,
}

fn main() -> Result<()> {
    let cfg = config::load()?;
    i18n::init(&cfg.language);
    theme::init(&cfg.theme);
    let mut terminal = ratatui::init();
    let mut app = App::new(&cfg);
    let res = app.run(&mut terminal);
    ratatui::restore();
    res?;
    if app.update {
        self_update()?;
    }
    Ok(())
}

/// Source checkout this binary was built from (`cargo install --path .`).
/// A prebuilt binary points at a path that doesn't exist, so git just fails.
const REPO: &str = env!("CARGO_MANIFEST_DIR");

fn git(args: &[&str]) -> Command {
    let mut c = Command::new("git");
    // No credential prompt drawing over the TUI.
    c.arg("-C").arg(REPO).args(args).env("GIT_TERMINAL_PROMPT", "0");
    c
}

/// `git fetch` in the background on launch; sends how many commits behind it is.
fn check_updates() -> Receiver<usize> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let quiet = |c: &mut Command| c.stdin(Stdio::null()).stderr(Stdio::null()).output().ok();
        // First launch after `cargo install`: enable hooks/post-merge, unless the
        // user already set their own hooks path.
        if quiet(&mut git(&["config", "core.hooksPath"])).is_some_and(|o| o.status.code() == Some(1)) {
            quiet(&mut git(&["config", "core.hooksPath", "hooks"]));
        }
        if !quiet(&mut git(&["fetch", "--quiet"])).is_some_and(|o| o.status.success()) {
            return;
        }
        let n = quiet(&mut git(&["rev-list", "--count", "HEAD..@{u}"]))
            .and_then(|o| String::from_utf8_lossy(&o.stdout).trim().parse().ok())
            .unwrap_or(0);
        let _ = tx.send(n);
    });
    rx
}

/// Pull, reinstall and relaunch, in the plain terminal so git/cargo output is visible.
fn self_update() -> Result<()> {
    // DEVC_UPDATING: the post-merge hook (hooks/post-merge) would install twice.
    if !git(&["pull", "--ff-only"]).env("DEVC_UPDATING", "1").status()?.success() {
        bail!("{} {REPO}", i18n::text("git pull failed in", "git pull falló en"));
    }
    let exe = std::env::current_exe()?;
    // Windows won't overwrite a running .exe, but it will rename it.
    let old = exe.with_extension("old");
    if platform::WIN {
        let _ = std::fs::remove_file(&old);
        std::fs::rename(&exe, &old)?;
    }
    let ok = Command::new("cargo").args(["install", "--locked", "--path", REPO]).status()?.success();
    if !ok {
        if platform::WIN {
            std::fs::rename(&old, &exe)?;
        }
        bail!("{}", i18n::text("cargo install failed; devc was not updated", "cargo install falló; devc no se actualizó"));
    }
    Command::new(&exe).status()?;
    Ok(())
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
            updates: check_updates(),
            behind: 0,
            update: false,
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
                if let Ok(n) = self.updates.try_recv() {
                    self.behind = n;
                }
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
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) && self.behind > 0 => {
                self.update = true;
                return true;
            }
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
                        Ok(()) => format!("{}: {text}", i18n::text("Copied", "Copiado")),
                        Err(e) => format!("{}: {e}", i18n::text("Clipboard", "Portapapeles")),
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
                    title: format!("{} · {}", i18n::text("Error", "Error"), cmd.join(" ")),
                    text: e.to_string(),
                    scroll: 0,
                });
            }
        }
        if start.elapsed() > Duration::from_secs(10) {
            runner::notify(i18n::text("Command finished", "Comando finalizado"), &cmd.join(" "));
        }
        self.refresh_active();
    }

    /// Suspends the TUI and hands the full terminal over to the command (ssh, yazi, btop...).
    fn run_interactive(&mut self, terminal: &mut DefaultTerminal, cmd: &[String]) {
        if cmd.is_empty() {
            return;
        }
        ratatui::restore();
        let status = std::process::Command::new(runner::program(&cmd[0]))
            .args(&cmd[1..])
            .status();
        *terminal = ratatui::init();
        let _ = terminal.clear();
        if let Err(e) = status {
            self.overlay = Some(Overlay::Pane {
                title: i18n::text("Error", "Error").into(),
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
        let mut brand = String::from(if self.pending_refresh { "DevTerminal ⟳" } else { "DevTerminal" });
        if self.behind > 0 {
            brand += &format!(
                " · ⬆ {} {}",
                self.behind,
                i18n::text("new commits, Ctrl+U to update", "commits nuevos, Ctrl+U para actualizar")
            );
        }
        f.render_widget(
            Tabs::new(titles)
                .select(self.active)
                .block(block_c(&brand, active_accent)),
            header,
        );

        self.modules[self.active].draw(f, body);

        let text = match &self.notice {
            Some(n) => format!(" {n}"),
            None => {
                let hints = self.modules[self.active].footer();
                let sep = if hints.is_empty() { "" } else { " · " };
                format!(
                    " {hints}{sep}{}",
                    i18n::text(
                        "y copy · Tab/A/D module · 1-9 jump · R refresh · Q quit",
                        "y copiar · Tab/A/D módulo · 1-9 ir a módulo · R actualizar · Q salir",
                    )
                )
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
                        .block(block(&format!(
                            "{title} — {}",
                            i18n::text("w/s scroll · Esc closes", "w/s desplazar · Esc cerrar")
                        )))
                        .wrap(Wrap { trim: false })
                        .scroll((*scroll, 0)),
                    area,
                );
            }
            Some(Overlay::Confirm { msg, .. }) => {
                let area = centered(f.area(), 60, 25);
                f.render_widget(Clear, area);
                f.render_widget(
                    Paragraph::new(format!(
                        "{msg}\n\n{}",
                        i18n::text("Enter/y confirm · Esc/n cancel", "Enter/y confirmar · Esc/n cancelar")
                    ))
                        .block(block(i18n::text("Confirm", "Confirmar")))
                        .wrap(Wrap { trim: false }),
                    area,
                );
            }
            Some(Overlay::Prompt { label, input, .. }) => {
                let area = centered(f.area(), 60, 25);
                f.render_widget(Clear, area);
                f.render_widget(
                    Paragraph::new(format!(
                        "{label}\n\n> {input}▏\n\n{}",
                        i18n::text("Enter accept · Esc cancel", "Enter aceptar · Esc cancelar")
                    ))
                        .block(block(i18n::text("Input", "Entrada")))
                        .wrap(Wrap { trim: false }),
                    area,
                );
            }
            None => {}
        }
    }
}
