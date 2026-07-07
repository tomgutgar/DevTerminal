use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::Frame;

use super::{Action, Module};
use crate::runner::{has_bin, run_cmd};
use crate::theme;
use crate::ui::ListView;


/// Color según la columna STATUS de `kubectl get pods`.
fn pod_status_color(status: &str) -> Color {
    match status {
        "Running" | "Completed" => theme::p().green,
        "Pending" | "ContainerCreating" | "PodInitializing" | "Terminating" => {
            theme::p().yellow
        }
        _ => theme::p().red, // CrashLoopBackOff, Error, ImagePullBackOff...
    }
}

// (título, recurso kubectl, con namespace)
const VIEWS: [(&str, &str, bool); 5] = [
    ("Pods", "pods", true),
    ("Deployments", "deployments", true),
    ("Services", "services", true),
    ("Nodes", "nodes", false),
    ("Namespaces", "namespaces", false),
];

pub struct K8s {
    view: usize,
    list: ListView,
}

impl K8s {
    pub fn new() -> Self {
        Self { view: 0, list: ListView::new() }
    }

    fn resource(&self) -> &'static str {
        VIEWS[self.view].1
    }

    fn namespaced(&self) -> bool {
        VIEWS[self.view].2
    }

    /// id = "ns nombre" para recursos con namespace, "nombre" para el resto.
    fn split_id(id: &str) -> (Option<&str>, &str) {
        match id.split_once(' ') {
            Some((ns, name)) => (Some(ns), name),
            None => (None, id),
        }
    }

    fn kubectl(&self, id: &str, args: &[&str]) -> Vec<String> {
        let (ns, name) = Self::split_id(id);
        let mut v = vec!["kubectl".to_string()];
        v.extend(args.iter().map(|s| s.to_string()));
        v.push(name.to_string());
        if let Some(ns) = ns {
            v.push("-n".into());
            v.push(ns.to_string());
        }
        v
    }
}

impl Module for K8s {
    fn title(&self) -> &'static str {
        "K8s"
    }

    fn refresh(&mut self) {
        if !has_bin("kubectl") {
            self.list.set_items(vec![("kubectl no está instalado".into(), String::new())]);
            return;
        }
        let mut cmd = vec!["kubectl".to_string(), "get".into(), self.resource().into()];
        if self.namespaced() {
            cmd.push("-A".into());
        }
        cmd.push("--no-headers".into());
        match run_cmd(&cmd) {
            Ok(o) => {
                let ns = self.namespaced();
                let is_pods = self.view == 0;
                let rows: Vec<_> = o
                    .lines()
                    .filter(|l| !l.trim().is_empty())
                    .map(|l| {
                        let cols: Vec<&str> = l.split_whitespace().collect();
                        let id = if ns && cols.len() >= 2 {
                            format!("{} {}", cols[0], cols[1])
                        } else {
                            cols.first().unwrap_or(&"").to_string()
                        };
                        // STATUS es la 4ª columna en `kubectl get pods -A` (ns name ready STATUS ...).
                        let color = is_pods.then(|| cols.get(3).copied().unwrap_or("")).map(pod_status_color);
                        (l.to_string(), id, color)
                    })
                    .collect();
                if rows.is_empty() {
                    self.list.set_items(vec![("(vacío)".into(), String::new())]);
                } else {
                    self.list.set_items_styled(rows);
                }
            }
            Err(e) => self.list.set_items(vec![(format!("Error: {e}"), String::new())]),
        }
    }

    fn draw(&mut self, f: &mut Frame, area: Rect) {
        self.list.draw(f, area, &format!("Kubernetes · [{}]", VIEWS[self.view].0), self.accent());
    }

    fn on_key(&mut self, key: KeyEvent) -> Action {
        if self.list.key(key) {
            return Action::Handled;
        }
        if key.code == KeyCode::Char('v') {
            self.view = (self.view + 1) % VIEWS.len();
            return Action::Refresh;
        }
        let Some(id) = self.list.selected_id().filter(|s| !s.is_empty()) else {
            return Action::Ignored;
        };
        match key.code {
            KeyCode::Enter => {
                let cmd = if self.view == 0 {
                    self.kubectl(&id, &["logs", "--tail=300"])
                } else {
                    self.kubectl(&id, &["describe", self.resource()])
                };
                let out = run_cmd(&cmd).unwrap_or_else(|e| e.to_string());
                Action::Show { title: id, text: out }
            }
            KeyCode::Char('e') if self.view == 0 => {
                let mut cmd = self.kubectl(&id, &["exec", "-it"]);
                cmd.extend(["--".into(), "sh".into()]);
                Action::Interactive(cmd)
            }
            KeyCode::Char('t') if self.view == 1 => Action::Run {
                cmd: self.kubectl(&id, &["rollout", "restart", "deployment"]),
                confirm: None,
                show: true,
            },
            KeyCode::Char('c') if self.view == 1 => Action::Prompt {
                label: "Número de réplicas".into(),
                template: {
                    let mut c = self.kubectl(&id, &["scale", "deployment"]);
                    c.push("--replicas={}".into());
                    c
                },
                interactive: false,
                show: true,
            },
            KeyCode::Char('k') => Action::Run {
                cmd: self.kubectl(&id, &["delete", self.resource()]),
                confirm: Some(format!("¿Eliminar {} {id}?", self.resource())),
                show: true,
            },
            _ => Action::Ignored,
        }
    }

    fn footer(&self) -> String {
        "v vista · Enter logs/describe · e shell · t rollout restart · c escalar · k eliminar".into()
    }

    fn accent(&self) -> Color {
        theme::p().teal
    }

    fn clip(&self) -> Option<String> {
        self.list.clip()
    }
}
