use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::Frame;

use super::{Action, Module};
use crate::config::{expand_home, Server};
use crate::runner::run_cmd;
use crate::theme;
use crate::ui::ListView;

const VIEWS: [&str; 3] = ["Puertos", "SSH", "Red"];

/// Puertos de desarrollo con nombre propio fuera de los rangos de `es_puerto_dev`:
/// bases de datos, colas, observabilidad, LLMs... (n8n 5678, ollama 11434, etc.).
const DEV_PORTS: &[u16] = &[
    1025,  // smtp de pruebas (mailhog/maildev)
    1080, 1313, 1433, 1521, 1880, 1883, 2181, 2375, 2376, 2379, 2380,
    3100, 3200, 3306, 3690, 4200, 4222, 4317, 4318, 4321, 4443, 4566, 4646,
    5555, 5601, 5671, 5672,
    5678,  // n8n
    5858, 5984, 6006, 6060, 6333, 6443, 7077, 7233, 7474, 7687, 7700, 7860,
    8123, 8161, 8200, 8500, 8501, 8529, 8545, 8546, 8761, 8787, 8848, 8983,
    9042, 9200, 9222, 9229, 9300, 9411, 9418, 10000, 10250, 11211,
    11434, // ollama
    15672, 16686, 19000, 19006, 24678, 27017, 27018, 27019, 28015,
    33060, 35729, 50051, 54321, 54322, 61616,
];

/// ¿Es un puerto típico de servidores de desarrollo? (~600 puertos: rangos de
/// frameworks web, vite, postgres, redis, jupyter... + la lista con nombre).
/// Deja fuera el ruido de escritorio: spotify (4070/57621), vscode, discord, cups...
fn es_puerto_dev(port: u16) -> bool {
    matches!(port,
        3000..=3099          // node, react, rails, grafana
        | 4000..=4050        // phoenix, jekyll
        | 5000..=5099        // flask, asp.net
        | 5170..=5180        // vite
        | 5432..=5440        // postgres
        | 6379..=6390        // redis
        | 7000..=7010        // cassandra, gotty
        | 8000..=8099        // django, http alternativo
        | 8440..=8450        // https alternativo
        | 8880..=8899        // jupyter
        | 9000..=9099        // php-fpm, minio, prometheus, kafka
    ) || DEV_PORTS.contains(&port)
}

/// Puertos TCP en escucha (servidores de desarrollo locales) + hosts SSH.
pub struct Servers {
    view: usize,
    list: ListView,
    servers: Vec<Server>,
    /// Mostrar todos los puertos en escucha, no solo los de desarrollo.
    todos: bool,
}

/// Hosts de ~/.ssh/config (ignora patrones con comodines).
pub fn parse_ssh_config(text: &str) -> Vec<String> {
    let mut hosts = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("Host ").or_else(|| line.strip_prefix("host ")) {
            for h in rest.split_whitespace() {
                if !h.contains('*') && !h.contains('?') && !h.contains('!') {
                    hosts.push(h.to_string());
                }
            }
        }
    }
    hosts
}

/// Una línea de `ss -tlnp` → (host, puerto, nombre proceso, pid, expuesto a la red).
fn parse_ss_line(line: &str) -> Option<(String, String, String, String, bool)> {
    let cols: Vec<&str> = line.split_whitespace().collect();
    if cols.first() != Some(&"LISTEN") {
        return None;
    }
    let local = *cols.get(3)?;
    let (addr, port) = local.rsplit_once(':')?;
    let expuesto = matches!(addr, "0.0.0.0" | "*" | "[::]" | "::");
    let host = if expuesto || addr == "127.0.0.1" || addr == "[::1]" { "localhost" } else { addr };
    let proc_tok = cols.get(5).copied().unwrap_or("");
    let name = proc_tok.split('"').nth(1).unwrap_or("?").to_string();
    let pid = proc_tok
        .split("pid=")
        .nth(1)
        .and_then(|s| s.split(|c: char| !c.is_ascii_digit()).next())
        .unwrap_or("")
        .to_string();
    Some((host.to_string(), port.to_string(), name, pid, expuesto))
}

impl Servers {
    pub fn new(servers: Vec<Server>) -> Self {
        Self { view: 0, list: ListView::new(), servers, todos: false }
    }

    /// Cada puerto se pinta como URL real (el terminal la abre con Ctrl+clic)
    /// con cada campo en su color. id = "pid\turl" para matar/abrir/copiar.
    fn refresh_ports(&mut self) {
        let p = theme::p();
        let out = run_cmd(&["ss".into(), "-tlnp".into()]);
        let rows: Vec<(Line<'static>, String)> = match out {
            Ok(o) => o
                .lines()
                .filter_map(parse_ss_line)
                .filter(|(_, port, _, pid, _)| {
                    !pid.is_empty()
                        && (self.todos
                            || port.parse::<u16>().map(es_puerto_dev).unwrap_or(false))
                })
                .map(|(host, port, name, pid, expuesto)| {
                    let url = format!("http://{host}:{port}");
                    let mut spans = vec![
                        Span::styled(
                            format!("{url:<28}"),
                            Style::default().fg(p.cyan).add_modifier(Modifier::UNDERLINED),
                        ),
                        Span::styled(format!(" {name}"), Style::default().fg(p.green)),
                        Span::styled(format!("  pid {pid}"), Style::default().fg(Color::DarkGray)),
                    ];
                    if expuesto {
                        spans.push(Span::styled(
                            "  ⚠ expuesto a la red",
                            Style::default().fg(p.orange),
                        ));
                    }
                    (Line::from(spans), format!("{pid}\t{url}"))
                })
                .collect(),
            Err(e) => vec![(Line::raw(format!("Error: {e}")), String::new())],
        };
        if rows.is_empty() {
            self.list.set_items(vec![(
                "Sin servidores de desarrollo escuchando (t muestra todos los puertos)".into(),
                String::new(),
            )]);
        } else {
            self.list.set_items_rich(rows);
        }
    }

    fn refresh_ssh(&mut self) {
        let p = theme::p();
        let mut rows: Vec<(Line<'static>, String)> = Vec::new();
        for s in &self.servers {
            let target = match &s.user {
                Some(u) => format!("{u}@{}", s.host),
                None => s.host.clone(),
            };
            let mut id = format!("ssh\t{target}");
            if let Some(port) = s.port {
                id = format!("ssh\t-p\t{port}\t{target}");
            }
            let desc = s.desc.clone().unwrap_or_default();
            rows.push((
                Line::from(vec![
                    Span::styled(
                        format!("⚑ {:<16}", s.alias),
                        Style::default().fg(p.yellow).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(format!(" {target}"), Style::default().fg(p.cyan)),
                    Span::styled(format!("  {desc}"), Style::default().fg(Color::DarkGray)),
                ]),
                id,
            ));
        }
        let cfg = std::fs::read_to_string(expand_home("~/.ssh/config")).unwrap_or_default();
        for h in parse_ssh_config(&cfg) {
            rows.push((
                Line::from(Span::styled(format!("  {h}"), Style::default().fg(p.cyan))),
                format!("ssh\t{h}"),
            ));
        }
        if rows.is_empty() {
            rows.push((
                Line::raw("Sin hosts: añade entradas a ~/.ssh/config o [[servers]] en config.toml"),
                String::new(),
            ));
        }
        self.list.set_items_rich(rows);
    }

    /// Interfaces (`ip -br addr`), ruta por defecto y DNS de /etc/resolv.conf.
    fn refresh_red(&mut self) {
        let mut rows: Vec<(String, String, Option<Color>)> = Vec::new();
        match run_cmd(&["ip".into(), "-br".into(), "addr".into()]) {
            Ok(o) => {
                for l in o.lines().filter(|l| !l.trim().is_empty()) {
                    let iface = l.split_whitespace().next().unwrap_or("").to_string();
                    let color = if l.contains(" UP ") {
                        Some(theme::p().green)
                    } else {
                        Some(Color::DarkGray)
                    };
                    rows.push((l.to_string(), iface, color));
                }
            }
            Err(e) => rows.push((format!("Error: {e}"), String::new(), None)),
        }
        if let Ok(o) = run_cmd(&["ip".into(), "route".into(), "show".into(), "default".into()]) {
            for l in o.lines().filter(|l| !l.trim().is_empty()) {
                rows.push((format!("⇡ {l}"), String::new(), None));
            }
        }
        let resolv = std::fs::read_to_string("/etc/resolv.conf").unwrap_or_default();
        for ns in resolv.lines().filter_map(|l| l.trim().strip_prefix("nameserver ")) {
            rows.push((format!("✦ DNS {}", ns.trim()), ns.trim().to_string(), None));
        }
        self.list.set_items_styled(rows);
    }
}

impl Module for Servers {
    fn title(&self) -> &'static str {
        "Servidores"
    }

    fn refresh(&mut self) {
        match self.view {
            0 => self.refresh_ports(),
            1 => self.refresh_ssh(),
            _ => self.refresh_red(),
        }
    }

    fn draw(&mut self, f: &mut Frame, area: Rect) {
        let mut title = format!("Servidores · [{}]", VIEWS[self.view]);
        if self.view == 0 {
            title.push_str(if self.todos { " · todos" } else { " · solo dev" });
        }
        self.list.draw(f, area, &title, self.accent());
    }

    fn on_key(&mut self, key: KeyEvent) -> Action {
        if self.list.key(key) {
            return Action::Handled;
        }
        if key.code == KeyCode::Char('v') {
            self.view = (self.view + 1) % VIEWS.len();
            return Action::Refresh;
        }
        if self.view == 0 && key.code == KeyCode::Char('t') {
            self.todos = !self.todos;
            return Action::Refresh;
        }
        // Ping y traceroute no dependen de la fila seleccionada.
        if self.view == 2 {
            match key.code {
                KeyCode::Char('p') => {
                    return Action::Prompt {
                        label: "Host a hacer ping".into(),
                        template: vec!["ping".into(), "{}".into()],
                        interactive: true,
                        show: false,
                    }
                }
                KeyCode::Char('t') => {
                    return Action::Prompt {
                        label: "Host para traceroute".into(),
                        template: vec!["traceroute".into(), "{}".into()],
                        interactive: true,
                        show: false,
                    }
                }
                _ => {}
            }
        }
        let Some(id) = self.list.selected_id().filter(|s| !s.is_empty()) else {
            return Action::Ignored;
        };
        match (self.view, key.code) {
            (0, KeyCode::Char('k')) => {
                let pid = id.split('\t').next().unwrap_or(&id).to_string();
                Action::Run {
                    cmd: vec!["kill".into(), pid.clone()],
                    confirm: Some(format!("¿Matar el proceso con pid {pid}?")),
                    show: false,
                }
            }
            (0, KeyCode::Char('o')) | (0, KeyCode::Enter) => match id.split('\t').nth(1) {
                Some(url) => Action::Run {
                    cmd: vec!["xdg-open".into(), url.to_string()],
                    confirm: None,
                    show: false,
                },
                None => Action::Handled,
            },
            (1, KeyCode::Enter) => Action::Interactive(id.split('\t').map(String::from).collect()),
            (1, KeyCode::Char('g')) => Action::Interactive(vec!["ssh-keygen".into()]),
            (1, KeyCode::Char('c')) => {
                let mut cmd: Vec<String> = id.split('\t').map(String::from).collect();
                cmd[0] = "ssh-copy-id".into();
                Action::Interactive(cmd)
            }
            _ => Action::Ignored,
        }
    }

    fn footer(&self) -> String {
        match self.view {
            0 => "v vista · Enter/o abrir en navegador (o Ctrl+clic en la URL) · k matar proceso · t todos/dev".into(),
            1 => "v vista · Enter conectar · c copiar clave (ssh-copy-id) · g generar claves".into(),
            _ => "v vista · p ping · t traceroute".into(),
        }
    }

    fn accent(&self) -> Color {
        theme::p().yellow
    }

    /// En la vista de puertos, `y` copia la URL (lo útil); en el resto, el id.
    fn clip(&self) -> Option<String> {
        let id = self.list.clip()?;
        if self.view == 0 {
            if let Some(url) = id.split('\t').nth(1) {
                return Some(url.to_string());
            }
        }
        Some(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filtra_puertos_de_desarrollo() {
        for p in [3000, 5173, 5432, 5678, 8080, 8888, 9092, 11434] {
            assert!(es_puerto_dev(p), "{p} debería ser puerto dev");
        }
        // spotify (4070/57621), cups (631), X11 (6000), aleatorio alto
        for p in [4070, 57621, 631, 6000, 45231] {
            assert!(!es_puerto_dev(p), "{p} no debería ser puerto dev");
        }
    }

    #[test]
    fn parsea_hosts() {
        let cfg = "Host vps\n  HostName 1.2.3.4\nHost *\n  User root\nHost pi nas\n";
        assert_eq!(parse_ssh_config(cfg), vec!["vps", "pi", "nas"]);
    }

    #[test]
    fn parsea_linea_ss() {
        let l = r#"LISTEN 0      511        127.0.0.1:3000       0.0.0.0:*    users:(("node",pid=12345,fd=20))"#;
        let (host, port, name, pid, expuesto) = parse_ss_line(l).unwrap();
        assert_eq!(host, "localhost");
        assert_eq!(port, "3000");
        assert_eq!(name, "node");
        assert_eq!(pid, "12345");
        assert!(!expuesto);
    }

    #[test]
    fn detecta_expuesto_a_la_red() {
        let l = r#"LISTEN 0      128          0.0.0.0:8080       0.0.0.0:*    users:(("python3",pid=999,fd=5))"#;
        let (host, port, _, _, expuesto) = parse_ss_line(l).unwrap();
        assert_eq!(host, "localhost");
        assert_eq!(port, "8080");
        assert!(expuesto);
    }
}
