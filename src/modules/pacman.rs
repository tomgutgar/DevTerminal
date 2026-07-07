use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::Frame;

use super::{Action, Module};
use crate::runner::{has_bin, run_cmd};
use crate::theme;
use crate::ui::ListView;


pub struct Pacman {
    list: ListView,
    helper: &'static str, // paru > yay > pacman (los helpers cubren AUR)
}

impl Pacman {
    pub fn new() -> Self {
        Self { list: ListView::new(), helper: "" }
    }

    /// Comando de gestión: helper AUR directo, o pacman con sudo.
    fn manage(&self, args: &[&str]) -> Vec<String> {
        let mut v = if self.helper == "pacman" {
            vec!["sudo".to_string(), "pacman".to_string()]
        } else {
            vec![self.helper.to_string()]
        };
        v.extend(args.iter().map(|s| s.to_string()));
        v
    }
}

impl Module for Pacman {
    fn title(&self) -> &'static str {
        "Pacman"
    }

    fn refresh(&mut self) {
        if self.helper.is_empty() {
            self.helper = ["paru", "yay", "pacman"]
                .into_iter()
                .find(|b| has_bin(b))
                .unwrap_or("");
        }
        if self.helper.is_empty() {
            self.list.set_items(vec![("pacman no disponible (¿no es Arch?)".into(), String::new())]);
            return;
        }
        // checkupdates (pacman-contrib) no toca la BD; fallback a -Qu
        let out = run_cmd(&["checkupdates".into()])
            .or_else(|_| run_cmd(&["pacman".into(), "-Qu".into()]));
        let rows: Vec<_> = match out {
            Ok(o) => o
                .lines()
                .filter(|l| !l.trim().is_empty())
                .map(|l| {
                    let pkg = l.split_whitespace().next().unwrap_or("").to_string();
                    (l.to_string(), pkg)
                })
                .collect(),
            // pacman -Qu devuelve 1 sin salida cuando no hay actualizaciones
            Err(_) => Vec::new(),
        };
        if rows.is_empty() {
            self.list.set_items(vec![("Sistema al día ✓".into(), String::new())]);
        } else {
            self.list.set_items(rows);
        }
    }

    fn draw(&mut self, f: &mut Frame, area: Rect) {
        self.list.draw(
            f,
            area,
            &format!("Pacman · actualizaciones pendientes (gestor: {})", self.helper),
            self.accent(),
        );
    }

    fn on_key(&mut self, key: KeyEvent) -> Action {
        if self.list.key(key) {
            return Action::Handled;
        }
        if self.helper.is_empty() {
            return Action::Ignored;
        }
        match key.code {
            KeyCode::Char('U') => Action::Interactive(self.manage(&["-Syu"])),
            KeyCode::Char('b') => Action::Prompt {
                label: "Buscar paquete".into(),
                template: vec![
                    if self.helper == "pacman" { "pacman".into() } else { self.helper.into() },
                    "-Ss".into(),
                    "{}".into(),
                ],
                interactive: false,
                show: true,
            },
            KeyCode::Char('i') => Action::Prompt {
                label: "Paquete a instalar".into(),
                template: self.manage(&["-S", "{}"]),
                interactive: true,
                show: false,
            },
            KeyCode::Char('x') => Action::Prompt {
                label: "Paquete a eliminar".into(),
                // pacman ya pide confirmación por sí mismo
                template: self.manage(&["-Rns", "{}"]),
                interactive: true,
                show: false,
            },
            KeyCode::Char('l') => Action::Interactive(self.manage(&["-Sc"])),
            _ => Action::Ignored,
        }
    }

    fn footer(&self) -> String {
        "U actualizar todo · b buscar · i instalar · x eliminar · l limpiar caché".into()
    }

    fn accent(&self) -> Color {
        theme::p().orange
    }

    fn clip(&self) -> Option<String> {
        self.list.clip()
    }
}
