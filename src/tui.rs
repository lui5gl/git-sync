use crate::config::Config;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::ExecutableCommand;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};
use ratatui::Frame;
use ratatui::Terminal;
use std::io::{stdout, Stdout};

enum InputMode {
    Normal,
    Adding,
    Editing(usize),
}

pub fn run_repo_manager(config: &Config) -> Result<(), String> {
    enable_raw_mode().map_err(|e| format!("No se pudo activar el modo raw del terminal: {}", e))?;
    let mut stdout = stdout();
    stdout
        .execute(EnterAlternateScreen)
        .map_err(|e| format!("No se pudo abrir la pantalla alternativa: {}", e))?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal =
        Terminal::new(backend).map_err(|e| format!("No se pudo inicializar el terminal: {}", e))?;

    let result = run_loop(&mut terminal, config);

    disable_raw_mode().map_err(|e| format!("No se pudo desactivar el modo raw del terminal: {}", e))?;
    terminal
        .backend_mut()
        .execute(LeaveAlternateScreen)
        .map_err(|e| format!("No se pudo restaurar la pantalla original: {}", e))?;
    terminal
        .show_cursor()
        .map_err(|e| format!("No se pudo restaurar el cursor del terminal: {}", e))?;

    result
}

struct RepoManager<'a> {
    config: &'a Config,
    repos: Vec<String>,
    list_state: ListState,
    input_mode: InputMode,
    input: String,
    message: Option<(String, Color)>,
}

impl<'a> RepoManager<'a> {
    fn new(config: &'a Config) -> Self {
        let repos = config.read_repos();
        let mut list_state = ListState::default();
        if !repos.is_empty() {
            list_state.select(Some(0));
        }

        RepoManager {
            config,
            repos,
            list_state,
            input_mode: InputMode::Normal,
            input: String::new(),
            message: None,
        }
    }

    fn select_next(&mut self) {
        let next_index = match self.list_state.selected() {
            Some(i) if !self.repos.is_empty() => (i + 1).min(self.repos.len() - 1),
            _ => 0,
        };
        if !self.repos.is_empty() {
            self.list_state.select(Some(next_index));
        }
    }

    fn select_previous(&mut self) {
        let prev_index = match self.list_state.selected() {
            Some(i) if i > 0 => i - 1,
            _ => 0,
        };
        if !self.repos.is_empty() {
            self.list_state.select(Some(prev_index));
        }
    }

    fn start_add(&mut self) {
        self.input_mode = InputMode::Adding;
        self.input.clear();
        self.message = None;
    }

    fn start_edit(&mut self) {
        if let Some(index) = self.list_state.selected() {
            if let Some(repo) = self.repos.get(index) {
                self.input_mode = InputMode::Editing(index);
                self.input = repo.clone();
                self.message = None;
            }
        }
    }

    fn delete_selected(&mut self) -> Result<(), String> {
        if let Some(index) = self.list_state.selected() {
            if index < self.repos.len() {
                self.repos.remove(index);
                self.persist()?;
                if self.repos.is_empty() {
                    self.list_state.select(None);
                } else if index >= self.repos.len() {
                    self.list_state.select(Some(self.repos.len() - 1));
                }
                self.set_message("Repositorio eliminado", Color::Yellow);
            }
        }
        Ok(())
    }

    fn submit(&mut self) -> Result<(), String> {
        let trimmed = self.input.trim();
        if trimmed.is_empty() {
            self.set_message("La ruta no puede estar vacía", Color::Red);
            return Ok(());
        }

        match self.input_mode {
            InputMode::Adding => {
                self.repos.push(trimmed.to_string());
                self.persist()?;
                self.list_state.select(Some(self.repos.len() - 1));
                self.set_message("Repositorio añadido", Color::Green);
            }
            InputMode::Editing(index) => {
                if let Some(slot) = self.repos.get_mut(index) {
                    *slot = trimmed.to_string();
                    self.persist()?;
                    self.set_message("Repositorio actualizado", Color::Green);
                }
            }
            InputMode::Normal => {}
        }

        self.input_mode = InputMode::Normal;
        self.input.clear();
        Ok(())
    }

    fn cancel_input(&mut self) {
        self.input_mode = InputMode::Normal;
        self.input.clear();
        self.set_message("Acción cancelada", Color::Yellow);
    }

    fn persist(&self) -> Result<(), String> {
        self.config.write_repos(&self.repos)
    }

    fn add_char(&mut self, ch: char) {
        self.input.push(ch);
    }

    fn backspace(&mut self) {
        self.input.pop();
    }

    fn set_message<S: Into<String>>(&mut self, message: S, color: Color) {
        self.message = Some((message.into(), color));
    }
}

fn run_loop(terminal: &mut Terminal<CrosstermBackend<Stdout>>, config: &Config) -> Result<(), String> {
    let mut manager = RepoManager::new(config);

    loop {
        terminal
            .draw(|frame| draw_ui(frame, &mut manager))
            .map_err(|e| format!("No se pudo renderizar la interfaz: {}", e))?;

        match event::read().map_err(|e| format!("No se pudo leer el evento de entrada: {}", e))? {
            Event::Key(KeyEvent { code, modifiers, .. }) => {
                if modifiers.contains(KeyModifiers::CONTROL) && code == KeyCode::Char('c') {
                    return Ok(());
                }

                match manager.input_mode {
                    InputMode::Normal => match code {
                        KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
                        KeyCode::Char('a') => manager.start_add(),
                        KeyCode::Char('e') | KeyCode::Enter => manager.start_edit(),
                        KeyCode::Char('d') => manager.delete_selected()?,
                        KeyCode::Down => manager.select_next(),
                        KeyCode::Up => manager.select_previous(),
                        _ => {}
                    },
                    InputMode::Adding | InputMode::Editing(_) => match code {
                        KeyCode::Enter => manager.submit()?,
                        KeyCode::Esc => manager.cancel_input(),
                        KeyCode::Backspace => manager.backspace(),
                        KeyCode::Char(c) => manager.add_char(c),
                        KeyCode::Left | KeyCode::Right | KeyCode::Home | KeyCode::End => {}
                        _ => {}
                    },
                }
            }
            _ => {}
        }
    }
}

fn draw_ui(frame: &mut Frame, manager: &mut RepoManager) {
    let size = frame.size();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(
            [
                Constraint::Min(5),
                Constraint::Length(3),
                Constraint::Length(3),
                Constraint::Length(1),
            ]
            .as_ref(),
        )
        .split(size);

    let items: Vec<ListItem> = if manager.repos.is_empty() {
        vec![ListItem::new(Line::from(vec![Span::styled(
            "No hay repositorios configurados",
            Style::default().fg(Color::DarkGray),
        )]))]
    } else {
        manager
            .repos
            .iter()
            .map(|repo| ListItem::new(Span::raw(repo.clone())))
            .collect()
    };

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title("Repositorios"))
        .highlight_style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))
        .highlight_symbol("➜ ");

    frame.render_stateful_widget(list, chunks[0], &mut manager.list_state);

    let instructions = match manager.input_mode {
        InputMode::Normal => "↑/↓ mover • a añadir • e editar • d eliminar • Enter editar • q/Esc salir",
        InputMode::Adding => "Modo añadir: escribe la ruta y presiona Enter para guardar, Esc para cancelar",
        InputMode::Editing(_) => "Modo editar: modifica la ruta y presiona Enter para guardar, Esc para cancelar",
    };

    let instruction_paragraph = Paragraph::new(instructions)
        .style(Style::default().fg(Color::Cyan))
        .block(Block::default().borders(Borders::ALL).title("Comandos"));
    frame.render_widget(instruction_paragraph, chunks[1]);

    let input_text = match manager.input_mode {
        InputMode::Normal => "".to_string(),
        _ => manager.input.clone(),
    };
    let input_block = Paragraph::new(input_text)
        .style(Style::default().fg(Color::White))
        .block(Block::default().borders(Borders::ALL).title("Ruta"));
    frame.render_widget(input_block, chunks[2]);

    if let InputMode::Adding | InputMode::Editing(_) = manager.input_mode {
        frame.set_cursor(chunks[2].x + manager.input.len() as u16 + 1, chunks[2].y + 1);
    }

    if let Some((message, color)) = &manager.message {
        let msg = Paragraph::new(message.clone())
            .style(Style::default().fg(*color))
            .block(Block::default().borders(Borders::ALL).title("Estado"));
        frame.render_widget(msg, chunks[3]);
    }
}
