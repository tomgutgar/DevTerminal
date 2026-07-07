pub mod dashboard;
pub mod docker;
pub mod git;
pub mod github;
pub mod k8s;
pub mod logs;
pub mod pacman;
pub mod procs;
pub mod servers;

use ratatui::crossterm::event::KeyEvent;
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::Frame;

use crate::config::Config;

/// Lo que un módulo pide al bucle principal tras una tecla.
pub enum Action {
    /// La tecla no es del módulo: probar atajos globales.
    Ignored,
    /// Consumida, nada que hacer.
    Handled,
    /// Consumida; refrescar el módulo *después* de dibujar el frame
    /// (los cambios de vista responden al instante aunque el comando tarde).
    Refresh,
    /// Ejecutar comando capturando salida. `confirm` abre diálogo antes;
    /// `show` muestra la salida en un panel al terminar.
    Run {
        cmd: Vec<String>,
        confirm: Option<String>,
        show: bool,
    },
    /// Suspender la TUI y ejecutar a pantalla completa (ssh, yazi, btop...).
    Interactive(Vec<String>),
    /// Pedir texto al usuario; "{}" en template se sustituye por la entrada.
    Prompt {
        label: String,
        template: Vec<String>,
        interactive: bool,
        show: bool,
    },
    /// Mostrar texto en un panel scrollable.
    Show { title: String, text: String },
}

pub trait Module {
    fn title(&self) -> &'static str;
    fn refresh(&mut self);
    fn draw(&mut self, f: &mut Frame, area: Rect);
    fn on_key(&mut self, key: KeyEvent) -> Action;
    /// Atajos propios para la barra inferior.
    fn footer(&self) -> String;
    /// Color de identidad del módulo (borde, texto, pestaña).
    fn accent(&self) -> Color {
        Color::Gray
    }
    /// Texto que copia la tecla global `y` (id o línea seleccionada).
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
        Box::new(pacman::Pacman::new()),
        Box::new(logs::Logs::new()),
    ]
}
