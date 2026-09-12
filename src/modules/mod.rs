pub mod dashboard;
pub mod docker;
pub mod git;
pub mod github;
pub mod k8s;
pub mod logs;
pub mod packages;
pub mod procs;
pub mod servers;

use ratatui::crossterm::event::KeyEvent;
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::Frame;

use crate::config::Config;

/// What a module asks the main loop to do after a key press.
pub enum Action {
    /// The key is not the module's: try the global shortcuts.
    Ignored,
    /// Consumed, nothing to do.
    Handled,
    /// Consumed; refresh the module *after* drawing the frame
    /// (view changes respond instantly even if the command is slow).
    Refresh,
    /// Run a command capturing output. `confirm` opens a dialog first
    /// (destructive actions); `show` displays the output in a pane when done.
    Run {
        cmd: Vec<String>,
        confirm: Option<String>,
        show: bool,
    },
    /// Suspend the TUI and run full-screen (ssh, yazi, btop...).
    Interactive(Vec<String>),
    /// Ask the user for text; "{}" in the template is replaced with the input.
    Prompt {
        label: String,
        template: Vec<String>,
        interactive: bool,
        show: bool,
    },
    /// Display text in a scrollable pane.
    Show { title: String, text: String },
}

pub trait Module {
    fn title(&self) -> &'static str;
    fn refresh(&mut self);
    fn draw(&mut self, f: &mut Frame, area: Rect);
    fn on_key(&mut self, key: KeyEvent) -> Action;
    /// The module's own shortcuts for the bottom bar.
    fn footer(&self) -> String;
    /// Module identity color (border, text, tab).
    fn accent(&self) -> Color {
        Color::Gray
    }
    /// Text copied by the global `y` key (id or selected line).
    fn clip(&self) -> Option<String> {
        None
    }
}

pub fn all(cfg: &Config) -> Vec<Box<dyn Module>> {
    vec![
        Box::new(dashboard::Dashboard::new()),
        Box::new(git::GitMod::new(cfg.workspaces.clone())),
        Box::new(github::GitHub::new()),
        Box::new(docker::Docker::new()),
        Box::new(k8s::K8s::new()),
        Box::new(servers::Servers::new(cfg.servers.clone())),
        Box::new(procs::Procs::new()),
        Box::new(packages::Packages::new()),
        Box::new(logs::Logs::new()),
    ]
}
