use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::Frame;
use sysinfo::{Pid, ProcessesToUpdate, System};

use super::{Action, Module};
use crate::config::{expand_home, Server};
use crate::platform;
use crate::runner::run_cmd;
use crate::theme;
use crate::ui::ListView;

const VIEWS: [&str; 3] = ["Ports", "SSH", "Network"];

/// Named development ports outside the `is_dev_port` ranges:
/// databases, queues, observability, LLMs... (n8n 5678, ollama 11434, etc.).
const DEV_PORTS: &[u16] = &[
    1025,  // test smtp (mailhog/maildev)
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

/// Is this a typical development-server port? (~600 ports: web framework
/// ranges, vite, postgres, redis, jupyter... plus the named list).
/// Leaves out desktop noise: spotify (4070/57621), vscode, discord, cups...
fn is_dev_port(port: u16) -> bool {
    matches!(port,
        3000..=3099          // node, react, rails, grafana
        | 4000..=4050        // phoenix, jekyll
        | 5000..=5099        // flask, asp.net
        | 5170..=5180        // vite
        | 5432..=5440        // postgres
        | 6379..=6390        // redis
        | 7000..=7010        // cassandra, gotty
        | 8000..=8099        // django, alternative http
        | 8440..=8450        // alternative https
        | 8880..=8899        // jupyter
        | 9000..=9099        // php-fpm, minio, prometheus, kafka
    ) || DEV_PORTS.contains(&port)
}

/// Listening TCP ports (local dev servers) + SSH hosts.
pub struct Servers {
    view: usize,
    list: ListView,
    servers: Vec<Server>,
    /// Show all listening ports, not just development ones.
    show_all: bool,
    /// Windows only: netstat gives the pid but not the process name.
    sys: System,
}

/// Hosts from ~/.ssh/config (ignores wildcard patterns).
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

/// One `ss -tlnp` line → (host, port, process name, pid, exposed to the network).
fn parse_ss_line(line: &str) -> Option<(String, String, String, String, bool)> {
    let cols: Vec<&str> = line.split_whitespace().collect();
    if cols.first() != Some(&"LISTEN") {
        return None;
    }
    let local = *cols.get(3)?;
    let (addr, port) = local.rsplit_once(':')?;
    let exposed = matches!(addr, "0.0.0.0" | "*" | "[::]" | "::");
    let host = if exposed || addr == "127.0.0.1" || addr == "[::1]" { "localhost" } else { addr };
    let proc_tok = cols.get(5).copied().unwrap_or("");
    let name = proc_tok.split('"').nth(1).unwrap_or("?").to_string();
    let pid = proc_tok
        .split("pid=")
        .nth(1)
        .and_then(|s| s.split(|c: char| !c.is_ascii_digit()).next())
        .unwrap_or("")
        .to_string();
    Some((host.to_string(), port.to_string(), name, pid, exposed))
}

/// One listening line of `netstat -ano` -> (host, port, pid, exposed).
/// The state column (`LISTENING`) is not used as a filter: it is translated on
/// non-English Windows. What marks a listening socket, in any language, is that
/// its remote address is port 0.
fn parse_netstat_line(line: &str) -> Option<(String, String, String, bool)> {
    let cols: Vec<&str> = line.split_whitespace().collect();
    if cols.len() < 5 || !cols[0].eq_ignore_ascii_case("tcp") || !cols[2].ends_with(":0") {
        return None;
    }
    let pid = cols[4];
    if pid.is_empty() || !pid.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let (addr, port) = cols[1].rsplit_once(':')?;
    let exposed = matches!(addr, "0.0.0.0" | "*" | "[::]");
    let host = if exposed || addr == "127.0.0.1" || addr == "[::1]" { "localhost" } else { addr };
    Some((host.to_string(), port.to_string(), pid.to_string(), exposed))
}

impl Servers {
    pub fn new(servers: Vec<Server>) -> Self {
        Self { view: 0, list: ListView::new(), servers, show_all: false, sys: System::new() }
    }

    /// Each port is drawn as a real URL (the terminal opens it with Ctrl+click)
    /// with each field in its own color. id = "pid\turl" for kill/open/copy.
    /// Listening TCP ports: `ss -tlnp` on Linux, `netstat -ano` on Windows
    /// (which gives the pid but not the name, so it is joined with sysinfo).
    fn listening(&mut self) -> anyhow::Result<Vec<(String, String, String, String, bool)>> {
        if !platform::WIN {
            return Ok(run_cmd(&["ss".into(), "-tlnp".into()])?
                .lines()
                .filter_map(parse_ss_line)
                .collect());
        }
        let out = run_cmd(&["netstat".into(), "-ano".into()])?;
        self.sys.refresh_processes(ProcessesToUpdate::All, true);
        Ok(out
            .lines()
            .filter_map(parse_netstat_line)
            .map(|(host, port, pid, exposed)| {
                let name = pid
                    .parse::<u32>()
                    .ok()
                    .and_then(|n| self.sys.process(Pid::from_u32(n)))
                    .map(|pr| pr.name().to_string_lossy().into_owned())
                    .unwrap_or_else(|| "?".into());
                (host, port, name, pid, exposed)
            })
            .collect())
    }

    fn refresh_ports(&mut self) {
        let p = theme::p();
        let show_all = self.show_all;
        let out = self.listening();
        let rows: Vec<(Line<'static>, String)> = match out {
            Ok(o) => o
                .into_iter()
                .filter(|(_, port, _, pid, _)| {
                    !pid.is_empty()
                        && (show_all || port.parse::<u16>().map(is_dev_port).unwrap_or(false))
                })
                .map(|(host, port, name, pid, exposed)| {
                    let url = format!("http://{host}:{port}");
                    let mut spans = vec![
                        Span::styled(
                            format!("{url:<28}"),
                            Style::default().fg(p.cyan).add_modifier(Modifier::UNDERLINED),
                        ),
                        Span::styled(format!(" {name}"), Style::default().fg(p.green)),
                        Span::styled(format!("  pid {pid}"), Style::default().fg(Color::DarkGray)),
                    ];
                    if exposed {
                        spans.push(Span::styled(
                            "  ⚠ exposed to the network",
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
                "No development servers listening (t shows all ports)".into(),
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
                Line::raw("No hosts: add entries to ~/.ssh/config or [[servers]] in config.toml"),
                String::new(),
            ));
        }
        self.list.set_items_rich(rows);
    }

    /// Interfaces, default route and DNS. `ip` + resolv.conf on Linux, the
    /// PowerShell Net* cmdlets on Windows.
    fn refresh_network(&mut self) {
        if platform::WIN {
            return self.refresh_network_win();
        }
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

    /// A single PowerShell call (starting it costs ~300 ms) emitting tagged
    /// lines; the symbols are added here and not in the script so the console
    /// code page can't mangle them.
    fn refresh_network_win(&mut self) {
        let script = "Get-NetAdapter | ForEach-Object { 'IF|{0}|{1}|{2}' -f $_.Name, $_.Status, \
             ((Get-NetIPAddress -InterfaceIndex $_.ifIndex -AddressFamily IPv4 -ErrorAction SilentlyContinue).IPAddress -join ' ') }; \
             Get-NetRoute -DestinationPrefix '0.0.0.0/0' -ErrorAction SilentlyContinue | ForEach-Object { 'GW|{0}|{1}' -f $_.NextHop, $_.InterfaceAlias }; \
             Get-DnsClientServerAddress -AddressFamily IPv4 -ErrorAction SilentlyContinue | ForEach-Object { $_.ServerAddresses } | Sort-Object -Unique | ForEach-Object { 'DNS|{0}' -f $_ }";
        let out = match run_cmd(&platform::ps(script)) {
            Ok(o) => o,
            Err(e) => {
                self.list.set_items(vec![(format!("Error: {e}"), String::new())]);
                return;
            }
        };
        let mut rows: Vec<(String, String, Option<Color>)> = Vec::new();
        for l in out.lines().map(str::trim).filter(|l| !l.is_empty()) {
            let c: Vec<&str> = l.split('|').collect();
            match c.as_slice() {
                ["IF", name, status, ips] => {
                    let color = if status.eq_ignore_ascii_case("up") {
                        Some(theme::p().green)
                    } else {
                        Some(Color::DarkGray)
                    };
                    rows.push((
                        format!("{name:<24} {status:<14} {ips}"),
                        name.to_string(),
                        color,
                    ));
                }
                ["GW", gw, iface] => {
                    rows.push((format!("\u{21e1} default via {gw} dev {iface}"), String::new(), None))
                }
                ["DNS", ns] => rows.push((format!("\u{2726} DNS {ns}"), ns.to_string(), None)),
                _ => {}
            }
        }
        self.list.set_items_styled(rows);
    }
}

impl Module for Servers {
    fn title(&self) -> &'static str {
        "Servers"
    }

    fn refresh(&mut self) {
        match self.view {
            0 => self.refresh_ports(),
            1 => self.refresh_ssh(),
            _ => self.refresh_network(),
        }
    }

    fn draw(&mut self, f: &mut Frame, area: Rect) {
        let mut title = format!("Servers · [{}]", VIEWS[self.view]);
        if self.view == 0 {
            title.push_str(if self.show_all { " · all" } else { " · dev only" });
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
            self.show_all = !self.show_all;
            return Action::Refresh;
        }
        // Ping and traceroute don't depend on the selected row.
        if self.view == 2 {
            match key.code {
                KeyCode::Char('p') => {
                    return Action::Prompt {
                        label: "Host to ping".into(),
                        // Windows ping sends 4 packets and stops; -t keeps it going like Linux.
                        template: if platform::WIN {
                            vec!["ping".into(), "-t".into(), "{}".into()]
                        } else {
                            vec!["ping".into(), "{}".into()]
                        },
                        interactive: true,
                        show: false,
                    }
                }
                KeyCode::Char('t') => {
                    return Action::Prompt {
                        label: "Host to traceroute".into(),
                        template: vec![
                            if platform::WIN { "tracert".to_string() } else { "traceroute".to_string() },
                            "{}".into(),
                        ],
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
                    cmd: platform::kill(&pid, false),
                    confirm: Some(format!("Kill the process with pid {pid}?")),
                    show: false,
                }
            }
            (0, KeyCode::Char('o')) | (0, KeyCode::Enter) => match id.split('\t').nth(1) {
                Some(url) => Action::Run {
                    cmd: platform::open_url(url),
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
                // The OpenSSH shipped with Windows has no ssh-copy-id: show the equivalent.
                if !crate::runner::has_bin("ssh-copy-id") {
                    let target = cmd.last().cloned().unwrap_or_default();
                    return Action::Show {
                        title: "ssh-copy-id not available".into(),
                        text: format!(
                            "The OpenSSH shipped with Windows has no ssh-copy-id.\n\n\
                             PowerShell equivalent, with the public key already generated (key g):\n\n\
                             type $env:USERPROFILE\\.ssh\\id_ed25519.pub | ssh {target} \
                             \"mkdir -p ~/.ssh && cat >> ~/.ssh/authorized_keys\"\n"
                        ),
                    };
                }
                Action::Interactive(cmd)
            }
            _ => Action::Ignored,
        }
    }

    fn footer(&self) -> String {
        match self.view {
            0 => "v view · Enter/o open in browser (or Ctrl+click the URL) · k kill process · t all/dev".into(),
            1 => "v view · Enter connect · c copy key (ssh-copy-id) · g generate keys".into(),
            _ => "v view · p ping · t traceroute".into(),
        }
    }

    fn accent(&self) -> Color {
        theme::p().yellow
    }

    /// In the ports view `y` copies the URL (the useful bit); elsewhere, the id.
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
    fn filters_dev_ports() {
        for p in [3000, 5173, 5432, 5678, 8080, 8888, 9092, 11434] {
            assert!(is_dev_port(p), "{p} should be a dev port");
        }
        // spotify (4070/57621), cups (631), X11 (6000), random high port
        for p in [4070, 57621, 631, 6000, 45231] {
            assert!(!is_dev_port(p), "{p} should not be a dev port");
        }
    }

    #[test]
    fn parses_hosts() {
        let cfg = "Host vps\n  HostName 1.2.3.4\nHost *\n  User root\nHost pi nas\n";
        assert_eq!(parse_ssh_config(cfg), vec!["vps", "pi", "nas"]);
    }

    #[test]
    fn parses_ss_line() {
        let l = r#"LISTEN 0      511        127.0.0.1:3000       0.0.0.0:*    users:(("node",pid=12345,fd=20))"#;
        let (host, port, name, pid, exposed) = parse_ss_line(l).unwrap();
        assert_eq!(host, "localhost");
        assert_eq!(port, "3000");
        assert_eq!(name, "node");
        assert_eq!(pid, "12345");
        assert!(!exposed);
    }

    #[test]
    fn parses_netstat_line() {
        // State column in Spanish: the parser doesn't look at it.
        let l = "  TCP    127.0.0.1:5173         0.0.0.0:0              ESCUCHANDO      4242";
        let (host, port, pid, exposed) = parse_netstat_line(l).unwrap();
        assert_eq!(
            (host.as_str(), port.as_str(), pid.as_str(), exposed),
            ("localhost", "5173", "4242", false)
        );

        let l = "  TCP    [::]:8080              [::]:0                 LISTENING       99";
        let (_, port, _, exposed) = parse_netstat_line(l).unwrap();
        assert_eq!(port, "8080");
        assert!(exposed);

        // Established connection (remote != :0) and UDP: out.
        assert!(parse_netstat_line("  TCP  10.0.0.2:52000  140.82.121.4:443  ESTABLISHED  7").is_none());
        assert!(parse_netstat_line("  UDP  0.0.0.0:5353    *:*                            8").is_none());
    }

    #[test]
    fn detects_network_exposed() {
        let l = r#"LISTEN 0      128          0.0.0.0:8080       0.0.0.0:*    users:(("python3",pid=999,fd=5))"#;
        let (host, port, _, _, exposed) = parse_ss_line(l).unwrap();
        assert_eq!(host, "localhost");
        assert_eq!(port, "8080");
        assert!(exposed);
    }
}
