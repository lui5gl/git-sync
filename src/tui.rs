use crate::config::{Config, RepoDefinition};
use crossterm::ExecutableCommand;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Frame;
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};
use std::io::{Stdout, stdout};

#[derive(Clone)]
enum InputMode {
    Normal,
    AddingSource,
    ChoosingBuildType { source: String },
    AddingDestination { source: String },
    EditingSource(usize),
    EditingDestination { index: usize, source: String },
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

    disable_raw_mode()
        .map_err(|e| format!("No se pudo desactivar el modo raw del terminal: {}", e))?;
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
    repos: Vec<RepoDefinition>,
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
        self.input_mode = InputMode::AddingSource;
        self.input.clear();
        self.set_message(
            "Introduce la ruta del repositorio a sincronizar",
            Color::Cyan,
        );
    }

    fn start_edit(&mut self) {
        if let Some(index) = self.list_state.selected() {
            if let Some(repo) = self.repos.get(index) {
                self.input_mode = InputMode::EditingSource(index);
                self.input = repo.repo_path.clone();
                self.set_message("Actualiza la ruta del repositorio", Color::Cyan);
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
        let input_value = self.input.trim().to_string();
        match self.input_mode.clone() {
            InputMode::AddingSource => {
                if input_value.is_empty() {
                    self.set_message("La ruta del repositorio no puede estar vacía", Color::Red);
                    return Ok(());
                }
                self.input_mode = InputMode::ChoosingBuildType {
                    source: input_value.clone(),
                };
                self.input.clear();
                self.set_message(
                    "¿El proyecto requiere compilación? 1) No • 2) Sí",
                    Color::Cyan,
                );
            }
            InputMode::AddingDestination { source } => {
                if input_value.is_empty() {
                    self.finalize_simple_repo(source)?;
                    return Ok(());
                }

                let deploy_target = input_value.clone();
                self.repos
                    .push(RepoDefinition::new(source, Some(deploy_target.clone())));
                self.persist()?;
                self.list_state.select(Some(self.repos.len() - 1));
                self.set_message("Repositorio de compilación añadido", Color::Green);
                self.input_mode = InputMode::Normal;
                self.input.clear();
            }
            InputMode::ChoosingBuildType { .. } => {}
            InputMode::EditingSource(index) => {
                if input_value.is_empty() {
                    self.set_message("La ruta del repositorio no puede estar vacía", Color::Red);
                    return Ok(());
                }
                if index >= self.repos.len() {
                    self.set_message("No se encontró el repositorio seleccionado", Color::Red);
                    self.cancel_input();
                    return Ok(());
                }
                let current_destination =
                    self.repos[index].deploy_target.clone().unwrap_or_default();
                self.input_mode = InputMode::EditingDestination {
                    index,
                    source: input_value,
                };
                self.input = current_destination;
                self.set_message(
                    "Actualiza la ruta de destino (opcional). Deja vacío para deshabilitar la compilación.",
                    Color::Cyan,
                );
            }
            InputMode::EditingDestination { index, source } => {
                if index >= self.repos.len() {
                    self.set_message("No se encontró el repositorio seleccionado", Color::Red);
                    self.input_mode = InputMode::Normal;
                    self.input.clear();
                    return Ok(());
                }
                let deploy_target = if input_value.is_empty() {
                    None
                } else {
                    Some(input_value.clone())
                };
                if let Some(repo) = self.repos.get_mut(index) {
                    repo.repo_path = source;
                    repo.deploy_target = deploy_target.clone();
                }
                self.persist()?;
                self.set_message(
                    if deploy_target.is_some() {
                        "Repositorio actualizado (compilación)"
                    } else {
                        "Repositorio actualizado"
                    },
                    Color::Green,
                );
                self.input_mode = InputMode::Normal;
                self.input.clear();
            }
            InputMode::Normal => {}
        }

        Ok(())
    }

    fn finalize_simple_repo(&mut self, source: String) -> Result<(), String> {
        self.repos
            .push(RepoDefinition::new(source, Option::<String>::None));
        self.persist()?;
        self.list_state.select(Some(self.repos.len() - 1));
        self.set_message("Repositorio añadido", Color::Green);
        self.input_mode = InputMode::Normal;
        self.input.clear();
        Ok(())
    }

    fn begin_destination_input(&mut self, source: String) {
        self.input_mode = InputMode::AddingDestination { source };
        self.input.clear();
        self.set_message(
            "Introduce la ruta de destino para desplegar el `dist` (Enter para confirmar, vacío para guardar como simple).",
            Color::Cyan,
        );
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

fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    config: &Config,
) -> Result<(), String> {
    let mut manager = RepoManager::new(config);

    loop {
        terminal
            .draw(|frame| draw_ui(frame, &mut manager))
            .map_err(|e| format!("No se pudo renderizar la interfaz: {}", e))?;

        match event::read().map_err(|e| format!("No se pudo leer el evento de entrada: {}", e))? {
            Event::Key(KeyEvent {
                code, modifiers, ..
            }) => {
                if modifiers.contains(KeyModifiers::CONTROL) && code == KeyCode::Char('c') {
                    return Ok(());
                }

                match manager.input_mode.clone() {
                    InputMode::Normal => match code {
                        KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
                        KeyCode::Char('a') => manager.start_add(),
                        KeyCode::Char('e') | KeyCode::Enter => manager.start_edit(),
                        KeyCode::Char('d') => manager.delete_selected()?,
                        KeyCode::Down => manager.select_next(),
                        KeyCode::Up => manager.select_previous(),
                        _ => {}
                    },
                    InputMode::ChoosingBuildType { source } => match code {
                        KeyCode::Char('1') | KeyCode::Char('n') | KeyCode::Char('N') => {
                            manager.finalize_simple_repo(source)?
                        }
                        KeyCode::Char('2') | KeyCode::Char('s') | KeyCode::Char('S') => {
                            manager.begin_destination_input(source)
                        }
                        KeyCode::Esc => manager.cancel_input(),
                        _ => {}
                    },
                    InputMode::AddingSource
                    | InputMode::AddingDestination { .. }
                    | InputMode::EditingSource(_)
                    | InputMode::EditingDestination { .. } => match code {
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
            .map(|repo| {
                let label = match &repo.deploy_target {
                    Some(target) if !target.is_empty() => {
                        format!("{} ⇒ {}", repo.repo_path, target)
                    }
                    _ => repo.repo_path.clone(),
                };
                ListItem::new(Span::raw(label))
            })
            .collect()
    };

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title("Repositorios"))
        .highlight_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("➜ ");

    frame.render_stateful_widget(list, chunks[0], &mut manager.list_state);

    let instructions = match manager.input_mode {
        InputMode::Normal => {
            "↑/↓ mover • a añadir • e editar • d eliminar • Enter editar • q/Esc salir"
        }
        InputMode::AddingSource => {
            "Modo añadir (origen): escribe la ruta del repositorio y presiona Enter"
        }
        InputMode::ChoosingBuildType { .. } => {
            "Selecciona si el proyecto requiere compilación: 1) No • 2) Sí • Esc cancelar"
        }
        InputMode::AddingDestination { .. } => {
            "Modo añadir (destino): escribe la ruta destino y presiona Enter, o deja vacío para guardar como simple"
        }
        InputMode::EditingSource(_) => "Modo editar (origen): modifica la ruta y presiona Enter",
        InputMode::EditingDestination { .. } => {
            "Modo editar (destino opcional): modifica la ruta destino y presiona Enter, o deja vacío"
        }
    };

    let instruction_paragraph = Paragraph::new(instructions)
        .style(Style::default().fg(Color::Cyan))
        .block(Block::default().borders(Borders::ALL).title("Comandos"));
    frame.render_widget(instruction_paragraph, chunks[1]);

    let (input_text, input_title) = match manager.input_mode {
        InputMode::Normal => ("".to_string(), "Ruta"),
        InputMode::AddingSource | InputMode::EditingSource(_) => {
            (manager.input.clone(), "Ruta origen")
        }
        InputMode::AddingDestination { .. } | InputMode::EditingDestination { .. } => {
            (manager.input.clone(), "Ruta destino (opcional)")
        }
        InputMode::ChoosingBuildType { .. } => (
            "1) Sin build • 2) Ejecutar build y desplegar dist".to_string(),
            "Tipo de proyecto",
        ),
    };
    let input_block = Paragraph::new(input_text)
        .style(Style::default().fg(Color::White))
        .block(Block::default().borders(Borders::ALL).title(input_title));
    frame.render_widget(input_block, chunks[2]);

    if matches!(
        manager.input_mode,
        InputMode::AddingSource
            | InputMode::AddingDestination { .. }
            | InputMode::EditingSource(_)
            | InputMode::EditingDestination { .. }
    ) {
        frame.set_cursor(
            chunks[2].x + manager.input.len() as u16 + 1,
            chunks[2].y + 1,
        );
    }

    if let Some((message, color)) = &manager.message {
        let msg = Paragraph::new(message.clone())
            .style(Style::default().fg(*color))
            .block(Block::default().borders(Borders::ALL).title("Estado"));
        frame.render_widget(msg, chunks[3]);
    }
}
