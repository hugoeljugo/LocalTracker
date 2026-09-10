use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};
use ratatui::Frame;

use crate::models::MatchHistoryEntry;

/// Estado de la TUI, separado a propósito del bucle de dibujo y de
/// `crossterm::event` para poder testearlo sin abrir una terminal real
/// (evita que los tests se congelen esperando eventos, per Regla 7).
#[derive(Debug, Clone, Default)]
pub struct App {
    pub puuid: Option<String>,
    pub matches: Vec<MatchHistoryEntry>,
    pub should_quit: bool,
}

impl App {
    pub fn new(puuid: Option<String>, matches: Vec<MatchHistoryEntry>) -> Self {
        Self {
            puuid,
            matches,
            should_quit: false,
        }
    }

    /// Traduce una tecla a un cambio de estado. Recibe un `char` plano en
    /// vez de un evento de `crossterm` para poder testearse sin depender
    /// de tipos de terminal.
    pub fn on_key(&mut self, key: char) {
        if key == 'q' {
            self.should_quit = true;
        }
    }
}

/// Dibuja el estado actual de `App` en el frame dado. Pura función de
/// estado -> UI, sin efectos secundarios ni lectura de eventos.
pub fn draw(frame: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)])
        .split(frame.area());

    let title = format!(
        "VALORANT Local Tracker — cuenta: {}  (q para salir)",
        app.puuid.as_deref().unwrap_or("desconocida")
    );
    frame.render_widget(
        Paragraph::new(title).block(Block::default().borders(Borders::ALL)),
        chunks[0],
    );

    let items: Vec<ListItem> = app
        .matches
        .iter()
        .map(|m| {
            ListItem::new(Line::from(format!(
                "{} | inicio: {} | cola: {}",
                m.match_id,
                m.game_start_time,
                m.queue_id.as_deref().unwrap_or("desconocida")
            )))
        })
        .collect();
    frame.render_widget(
        List::new(items).block(Block::default().borders(Borders::ALL).title("Historial")),
        chunks[1],
    );
}

/// Bucle real de dibujo + lectura de eventos de terminal. Deliberadamente
/// no se testea: correría indefinidamente esperando input real. La lógica
/// que sí se testea (transiciones de estado) vive en [`App::on_key`].
pub fn run(mut terminal: ratatui::DefaultTerminal, mut app: App) -> std::io::Result<()> {
    loop {
        terminal.draw(|frame| draw(frame, &app))?;

        if crossterm::event::poll(std::time::Duration::from_millis(250))? {
            if let crossterm::event::Event::Key(key) = crossterm::event::read()? {
                if key.kind == crossterm::event::KeyEventKind::Press {
                    if let crossterm::event::KeyCode::Char(c) = key.code {
                        app.on_key(c);
                    }
                }
            }
        }

        if app.should_quit {
            return Ok(());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_tecla_q_marca_should_quit() {
        let mut app = App::new(None, vec![]);
        assert!(!app.should_quit);

        app.on_key('q');

        assert!(app.should_quit);
    }

    #[test]
    fn otras_teclas_no_afectan_el_estado() {
        let mut app = App::new(Some("puuid-x".to_string()), vec![]);

        app.on_key('a');

        assert!(!app.should_quit);
        assert_eq!(app.puuid.as_deref(), Some("puuid-x"));
    }
}
