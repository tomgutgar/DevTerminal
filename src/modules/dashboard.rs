use std::collections::HashSet;

use ratatui::crossterm::event::KeyEvent;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Gauge, Paragraph};
use ratatui::Frame;
use sysinfo::{Disks, System};

use super::{Action, Module};
use crate::runner;
use crate::theme;
use crate::ui::block_c;

pub struct Dashboard {
    sys: System,
    cpu: f32,
    mem: (u64, u64),  // usado, total (bytes)
    swap: (u64, u64),
    disks: Vec<(String, f64, f64)>, // punto de montaje, usado GiB, total GiB
    info: Vec<(&'static str, String)>,
    primed: bool, // ya hay una muestra previa de CPU (el % es real sin dormir)
}

impl Dashboard {
    pub fn new() -> Self {
        Self {
            sys: System::new_all(),
            cpu: 0.0,
            mem: (0, 1),
            swap: (0, 1),
            disks: Vec::new(),
            info: Vec::new(),
            primed: false,
        }
    }
}

fn gib(bytes: u64) -> f64 {
    bytes as f64 / 1024.0 / 1024.0 / 1024.0
}

impl Module for Dashboard {
    fn title(&self) -> &'static str {
        "Dashboard"
    }

    fn refresh(&mut self) {
        // El % de CPU se calcula entre dos muestras. Solo la primera vez hay
        // que esperar el intervalo mínimo; después la muestra del refresco
        // anterior ya sirve de base y no se duerme nada.
        self.sys.refresh_cpu_usage();
        if !self.primed {
            std::thread::sleep(sysinfo::MINIMUM_CPU_UPDATE_INTERVAL);
            self.sys.refresh_cpu_usage();
            self.primed = true;
        }
        self.sys.refresh_memory();
        self.cpu = self.sys.global_cpu_usage();
        self.mem = (self.sys.used_memory(), self.sys.total_memory().max(1));
        self.swap = (self.sys.used_swap(), self.sys.total_swap().max(1));

        // btrfs monta la misma partición en varios subvolúmenes (/, /home...):
        // se deduplica por dispositivo+tamaño y se muestra el primer montaje.
        let mut vistos = HashSet::new();
        self.disks = Disks::new_with_refreshed_list()
            .iter()
            .filter(|d| vistos.insert((d.name().to_owned(), d.total_space())))
            .map(|d| {
                let total = d.total_space();
                let used = total - d.available_space();
                (d.mount_point().display().to_string(), gib(used), gib(total))
            })
            .collect();

        let load = System::load_average();
        let fecha = runner::run_cmd(&["date".into(), "+%A %d/%m/%Y  %H:%M".into()])
            .unwrap_or_default();
        // ponytail: docker se consulta al refrescar, no en vivo; refresco asíncrono cuando duela
        let docker = if runner::has_bin("docker") {
            runner::run_cmd(&["docker".into(), "ps".into(), "-q".into()])
                .map(|o| format!("{} contenedores activos", o.lines().count()))
                .unwrap_or_else(|_| "daemon parado".into())
        } else {
            "no instalado".into()
        };
        self.info = vec![
            (
                "Host",
                format!(
                    "{} ({} {})",
                    System::host_name().unwrap_or_default(),
                    System::name().unwrap_or_default(),
                    System::os_version().unwrap_or_default()
                ),
            ),
            ("Kernel", System::kernel_version().unwrap_or_default()),
            (
                "Uptime",
                format!("{} h {} min", System::uptime() / 3600, System::uptime() % 3600 / 60),
            ),
            ("Carga", format!("{:.2} {:.2} {:.2}", load.one, load.five, load.fifteen)),
            ("Docker", docker),
            ("Fecha", fecha.trim().to_string()),
        ];
    }

    fn draw(&mut self, f: &mut Frame, area: Rect) {
        let p = theme::p();
        let [left, right] =
            Layout::horizontal([Constraint::Percentage(45), Constraint::Percentage(55)])
                .areas(area);

        let [g_cpu, g_mem, g_swap, disks] = Layout::vertical([
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Min(0),
        ])
        .areas(left);

        // Etiqueta del gauge en blanco negrita: legible tanto sobre la parte
        // llena de la barra como sobre la vacía (antes era del color de la barra).
        let gauge = |title: String, ratio: f64, color: Color| {
            let ratio = ratio.clamp(0.0, 1.0);
            Gauge::default()
                .block(block_c(&title, color))
                .gauge_style(Style::default().fg(color))
                .label(Span::styled(
                    format!("{:.0}%", ratio * 100.0),
                    Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
                ))
                .ratio(ratio)
        };
        f.render_widget(gauge("CPU".into(), self.cpu as f64 / 100.0, p.cyan), g_cpu);
        f.render_widget(
            gauge(
                format!("RAM {:.1}/{:.1} GiB", gib(self.mem.0), gib(self.mem.1)),
                self.mem.0 as f64 / self.mem.1 as f64,
                p.green,
            ),
            g_mem,
        );
        f.render_widget(
            gauge(
                format!("Swap {:.1}/{:.1} GiB", gib(self.swap.0), gib(self.swap.1)),
                self.swap.0 as f64 / self.swap.1 as f64,
                p.yellow,
            ),
            g_swap,
        );

        // Composición título/valor: montaje en acento, cifras en blanco,
        // % de uso coloreado por umbral.
        let val = Style::default().fg(Color::White);
        let disk_lines: Vec<Line> = self
            .disks
            .iter()
            .map(|(mount, used, total)| {
                let pct = if *total > 0.0 { used / total * 100.0 } else { 0.0 };
                let pct_color = if pct < 70.0 {
                    p.green
                } else if pct < 90.0 {
                    p.yellow
                } else {
                    p.red
                };
                Line::from(vec![
                    Span::styled(format!("{mount:<18}"), Style::default().fg(self.accent())),
                    Span::styled(format!("{used:>7.1} / {total:>7.1} GiB"), val),
                    Span::styled(format!("  {pct:>3.0}%"), Style::default().fg(pct_color)),
                ])
            })
            .collect();
        f.render_widget(
            Paragraph::new(disk_lines).block(block_c("Discos", self.accent())),
            disks,
        );

        let info_lines: Vec<Line> = self
            .info
            .iter()
            .flat_map(|(label, value)| {
                [
                    Line::from(vec![
                        Span::styled(
                            format!("{label:<10}"),
                            Style::default().fg(self.accent()).add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(value.clone(), val),
                    ]),
                    Line::raw(""),
                ]
            })
            .collect();
        f.render_widget(
            Paragraph::new(info_lines).block(block_c("Sistema", self.accent())),
            right,
        );
    }

    fn on_key(&mut self, _key: KeyEvent) -> Action {
        Action::Ignored
    }

    fn footer(&self) -> String {
        String::new()
    }

    fn accent(&self) -> Color {
        theme::p().cyan
    }
}
