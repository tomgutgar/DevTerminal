use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, BorderType, Borders, List, ListItem, ListState};
use ratatui::Frame;

pub fn block(title: &str) -> Block<'static> {
    block_c(title, Color::DarkGray)
}

/// Bloque con borde y título en el color de identidad del módulo.
pub fn block_c(title: &str, color: Color) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(color))
        .title_style(Style::default().fg(color).add_modifier(Modifier::BOLD))
        .title(format!(" {title} "))
}

/// Rect centrado al pct% del área dada (overlays).
pub fn centered(area: Rect, pct_x: u16, pct_y: u16) -> Rect {
    let w = area.width * pct_x / 100;
    let h = area.height * pct_y / 100;
    Rect::new(
        area.x + (area.width - w) / 2,
        area.y + (area.height - h) / 2,
        w,
        h,
    )
}

/// Fila de la lista: línea renderizable (con spans de colores), texto plano
/// para filtrar/copiar y un id para las acciones.
struct Row {
    line: Line<'static>,
    text: String,
    id: String,
}

/// Lista con selección (W/S) y filtro (/ o F) compartida por todos los módulos.
pub struct ListView {
    items: Vec<Row>,
    pub state: ListState,
    pub filter: String,
    pub filtering: bool,
}

impl ListView {
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            state: ListState::default(),
            filter: String::new(),
            filtering: false,
        }
    }

    pub fn set_items(&mut self, items: Vec<(String, String)>) {
        self.items = items
            .into_iter()
            .map(|(t, id)| Row { line: Line::raw(t.clone()), text: t, id })
            .collect();
        self.clamp();
    }

    /// Como `set_items`, pero cada fila puede fijar su propio color (estado: running, error...).
    pub fn set_items_styled(&mut self, items: Vec<(String, String, Option<Color>)>) {
        self.items = items
            .into_iter()
            .map(|(t, id, color)| {
                let line = match color {
                    Some(c) => Line::styled(t.clone(), Style::default().fg(c)),
                    None => Line::raw(t.clone()),
                };
                Row { line, text: t, id }
            })
            .collect();
        self.clamp();
    }

    /// Filas con spans de varios colores (composición título/valor, URLs...).
    pub fn set_items_rich(&mut self, items: Vec<(Line<'static>, String)>) {
        self.items = items
            .into_iter()
            .map(|(line, id)| {
                let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
                Row { line, text, id }
            })
            .collect();
        self.clamp();
    }

    fn visible(&self) -> Vec<&Row> {
        let f = self.filter.to_lowercase();
        self.items
            .iter()
            .filter(|r| f.is_empty() || r.text.to_lowercase().contains(&f))
            .collect()
    }

    pub fn selected_id(&self) -> Option<String> {
        let vis = self.visible();
        self.state.selected().and_then(|i| vis.get(i)).map(|r| r.id.clone())
    }

    /// Texto a copiar con `y`: el id si existe (pid, hash, ruta...), si no la línea visible.
    pub fn clip(&self) -> Option<String> {
        let vis = self.visible();
        let row = self.state.selected().and_then(|i| vis.get(i))?;
        let out = if row.id.is_empty() { row.text.trim() } else { &row.id };
        (!out.is_empty()).then(|| out.to_string())
    }

    fn clamp(&mut self) {
        let len = self.visible().len();
        if len == 0 {
            self.state.select(None);
        } else {
            let i = self.state.selected().unwrap_or(0).min(len - 1);
            self.state.select(Some(i));
        }
    }

    /// Devuelve true si la tecla fue consumida por la lista.
    pub fn key(&mut self, key: KeyEvent) -> bool {
        if self.filtering {
            match key.code {
                KeyCode::Esc => {
                    self.filtering = false;
                    self.filter.clear();
                }
                KeyCode::Enter => self.filtering = false,
                KeyCode::Backspace => {
                    self.filter.pop();
                }
                KeyCode::Char(c) => self.filter.push(c),
                _ => {}
            }
            self.clamp();
            return true;
        }
        match key.code {
            KeyCode::Char('w') | KeyCode::Char('W') | KeyCode::Up => {
                let len = self.visible().len();
                if len > 0 {
                    let i = self.state.selected().unwrap_or(0);
                    self.state.select(Some(i.saturating_sub(1)));
                }
                true
            }
            KeyCode::Char('s') | KeyCode::Char('S') | KeyCode::Down => {
                let len = self.visible().len();
                if len > 0 {
                    let i = self.state.selected().unwrap_or(0);
                    self.state.select(Some((i + 1).min(len - 1)));
                }
                true
            }
            KeyCode::Char('/') | KeyCode::Char('f') | KeyCode::Char('F') => {
                self.filtering = true;
                true
            }
            _ => false,
        }
    }

    pub fn draw(&mut self, f: &mut Frame, area: Rect, title: &str, color: Color) {
        let title = if self.filter.is_empty() && !self.filtering {
            title.to_string()
        } else {
            format!("{title} — filtro: {}▏", self.filter)
        };
        let items: Vec<ListItem> = self
            .visible()
            .into_iter()
            .map(|r| ListItem::new(r.line.clone()))
            .collect();
        let list = List::new(items)
            .block(block_c(&title, color))
            .style(Style::default().fg(color))
            .highlight_style(
                Style::default()
                    .bg(crate::theme::p().sel_bg)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("▶ ");
        f.render_stateful_widget(list, area, &mut self.state);
    }
}
