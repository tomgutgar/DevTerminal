use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::Frame;

use super::{Action, Module};
use crate::runner::{has_bin, run_cmd};
use crate::theme;
use crate::ui::ListView;

const VIEWS: [(&str, &str); 4] = [
    ("Repos", "repo"),
    ("Issues", "issue"),
    ("PRs", "pr"),
    ("Actions", "run"),
];

pub struct GitHub {
    view: usize,
    list: ListView,
    /// Repo de contexto (owner/nombre) para issues/PRs/Actions. Sin él, gh
    /// usa el repo del cwd — que no existe si devc se lanzó fuera de uno.
    repo: Option<String>,
}

impl GitHub {
    pub fn new() -> Self {
        Self { view: 0, list: ListView::new(), repo: None }
    }

    fn kind(&self) -> &'static str {
        VIEWS[self.view].1
    }

    /// Comando gh con `--repo` si hay contexto fijado (los subcomandos `repo *`
    /// no admiten esa flag: reciben el repo como argumento posicional).
    fn gh(&self, args: &[&str]) -> Vec<String> {
        let mut v: Vec<String> = std::iter::once("gh".to_string())
            .chain(args.iter().map(|s| s.to_string()))
            .collect();
        if args.first() != Some(&"repo") {
            if let Some(r) = &self.repo {
                v.push("--repo".into());
                v.push(r.clone());
            }
        }
        v
    }
}

impl Module for GitHub {
    fn title(&self) -> &'static str {
        "GitHub"
    }

    fn refresh(&mut self) {
        if !has_bin("gh") {
            self.list.set_items(vec![("gh (GitHub CLI) no está instalado".into(), String::new())]);
            return;
        }
        let cmd: Vec<String> = match self.view {
            // repo list no admite --repo
            0 => vec!["gh".into(), "repo".into(), "list".into(), "--limit".into(), "40".into()],
            3 => self.gh(&["run", "list", "--limit", "20"]),
            _ => self.gh(&[self.kind(), "list", "--limit", "40"]),
        };
        match run_cmd(&cmd) {
            Ok(o) => {
                let rows: Vec<_> = o
                    .lines()
                    .filter(|l| !l.trim().is_empty())
                    .map(|l| {
                        // gh separa columnas con tabuladores; la 1ª (o el nº) es el id
                        let id = l.split('\t').next().unwrap_or("").trim().to_string();
                        (l.replace('\t', "  "), id)
                    })
                    .collect();
                if rows.is_empty() {
                    self.list.set_items(vec![("Sin resultados".into(), String::new())]);
                } else {
                    self.list.set_items(rows);
                }
            }
            Err(e) => {
                let mut rows = vec![(format!("Error: {e}"), String::new())];
                if self.view != 0 && self.repo.is_none() {
                    rows.push((String::new(), String::new()));
                    rows.push((
                        "→ Sin repo de contexto: ve a la vista Repos (v) y fija uno con Space,"
                            .into(),
                        String::new(),
                    ));
                    rows.push(("  o lanza devc dentro de un repositorio.".into(), String::new()));
                }
                self.list.set_items(rows);
            }
        }
    }

    fn draw(&mut self, f: &mut Frame, area: Rect) {
        let ctx = match &self.repo {
            Some(r) => format!("repo: {r}"),
            None => "repo del cwd".into(),
        };
        let title = format!("GitHub · [{}] · {ctx}", VIEWS[self.view].0);
        self.list.draw(f, area, &title, self.accent());
    }

    fn on_key(&mut self, key: KeyEvent) -> Action {
        if self.list.key(key) {
            return Action::Handled;
        }
        let id = self.list.selected_id().filter(|s| !s.is_empty());
        match key.code {
            KeyCode::Char('v') => {
                self.view = (self.view + 1) % VIEWS.len();
                Action::Refresh
            }
            // Fijar/quitar el repo seleccionado como contexto de issues/PRs/Actions
            KeyCode::Char(' ') if self.view == 0 => {
                self.repo = match (&self.repo, id) {
                    (Some(actual), Some(nuevo)) if *actual == nuevo => None, // toggle
                    (_, Some(nuevo)) => Some(nuevo),
                    (actual, None) => actual.clone(),
                };
                Action::Handled
            }
            KeyCode::Enter => match id {
                Some(id) => {
                    let out = run_cmd(&self.gh(&[self.kind(), "view", &id]))
                        .unwrap_or_else(|e| e.to_string());
                    Action::Show { title: format!("{} {id}", self.kind()), text: out }
                }
                None => Action::Handled,
            },
            KeyCode::Char('o') => match id {
                Some(id) => Action::Run {
                    cmd: self.gh(&[self.kind(), "view", &id, "--web"]),
                    confirm: None,
                    show: false,
                },
                None => Action::Handled,
            },
            KeyCode::Char('x') if self.view == 3 => match id {
                Some(id) => Action::Run {
                    cmd: self.gh(&["run", "rerun", &id]),
                    confirm: Some("¿Reejecutar este workflow?".into()),
                    show: true,
                },
                None => Action::Handled,
            },
            _ => Action::Ignored,
        }
    }

    fn footer(&self) -> String {
        match self.view {
            0 => "v vista · Space fijar repo de contexto · Enter detalles · o abrir en web".into(),
            3 => "v vista · Enter detalles · o abrir en web · x reejecutar run".into(),
            _ => "v vista · Enter detalles · o abrir en web".into(),
        }
    }

    fn accent(&self) -> Color {
        theme::p().magenta
    }

    fn clip(&self) -> Option<String> {
        self.list.clip()
    }
}
