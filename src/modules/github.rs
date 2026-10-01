use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::Frame;

use super::{Action, Module};
use crate::i18n;
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
    /// Context repo (owner/name) for issues/PRs/Actions. Without it, gh
    /// uses the cwd repo — which doesn't exist if devc was launched outside one.
    repo: Option<String>,
}

impl GitHub {
    pub fn new() -> Self {
        Self { view: 0, list: ListView::new(), repo: None }
    }

    fn kind(&self) -> &'static str {
        VIEWS[self.view].1
    }

    /// gh command with `--repo` when a context is set (the `repo *` subcommands
    /// don't accept that flag: they take the repo as a positional argument).
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
            self.list.set_items(vec![(i18n::text("gh (GitHub CLI) is not installed", "gh (GitHub CLI) no está instalado").into(), String::new())]);
            return;
        }
        let cmd: Vec<String> = match self.view {
            // repo list doesn't accept --repo
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
                        // gh separates columns with tabs; the first (or the number) is the id
                        let id = l.split('\t').next().unwrap_or("").trim().to_string();
                        (l.replace('\t', "  "), id)
                    })
                    .collect();
                if rows.is_empty() {
                    self.list.set_items(vec![(i18n::text("No results", "Sin resultados").into(), String::new())]);
                } else {
                    self.list.set_items(rows);
                }
            }
            Err(e) => {
                let mut rows = vec![(format!("Error: {e}"), String::new())];
                if self.view != 0 && self.repo.is_none() {
                    rows.push((String::new(), String::new()));
                    rows.push((
                        i18n::text(
                            "→ No context repo: go to the Repos view (v) and pin one with Space,",
                            "→ No hay repositorio de contexto: ve a Repos (v) y fija uno con Espacio,",
                        ).into(),
                        String::new(),
                    ));
                    rows.push((i18n::text("  or launch devc inside a repository.", "  o inicia devc dentro de un repositorio.").into(), String::new()));
                }
                self.list.set_items(rows);
            }
        }
    }

    fn draw(&mut self, f: &mut Frame, area: Rect) {
        let ctx = match &self.repo {
            Some(r) => format!("repo: {r}"),
            None => i18n::text("cwd repo", "repo del directorio actual").into(),
        };
        let view = match self.view {
            0 => i18n::text("Repos", "Repos"),
            1 => i18n::text("Issues", "Incidencias"),
            2 => i18n::text("PRs", "PRs"),
            _ => i18n::text("Actions", "Acciones"),
        };
        let title = format!("GitHub · [{view}] · {ctx}");
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
            // Pin/unpin the selected repo as context for issues/PRs/Actions
            KeyCode::Char(' ') if self.view == 0 => {
                self.repo = match (&self.repo, id) {
                    (Some(current), Some(new)) if *current == new => None, // toggle
                    (_, Some(new)) => Some(new),
                    (current, None) => current.clone(),
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
                    confirm: Some(i18n::text("Re-run this workflow?", "¿Volver a ejecutar este flujo?").into()),
                    show: true,
                },
                None => Action::Handled,
            },
            _ => Action::Ignored,
        }
    }

    fn footer(&self) -> String {
        match self.view {
            0 => i18n::text("v view · Space pin context repo · Enter details · o open in browser", "v vista · Espacio fijar repo de contexto · Enter detalles · o abrir en navegador").into(),
            3 => i18n::text("v view · Enter details · o open in browser · x re-run", "v vista · Enter detalles · o abrir en navegador · x volver a ejecutar").into(),
            _ => i18n::text("v view · Enter details · o open in browser", "v vista · Enter detalles · o abrir en navegador").into(),
        }
    }

    fn accent(&self) -> Color {
        theme::p().magenta
    }

    fn clip(&self) -> Option<String> {
        self.list.clip()
    }
}
