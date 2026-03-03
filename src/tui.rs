use crate::config::{Config, RepoDefinition};
use chrono::Local;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::ExecutableCommand;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};
use ratatui::Frame;
use ratatui::Terminal;
use std::io::{stdout, Stdout};
use std::time::{Duration, Instant};

#[derive(Clone)]
enum InputMode {
    Normal,
    ChoosingBuildType,
    AddingSource {
        requires_build: bool,
    },
    AddingDestination {
        source: String,
    },
    EditingSource(usize),
    EditingDestination {
        index: usize,
        source: String,
    },
}

pub fn run_repo_manager(config: &Config, sync_interval: u64) -> Result<(), String> {
    enable_raw_mode().map_err(|e| format!("No se pudo activar el modo raw del terminal: {}", e))?;
    let mut stdout = stdout();
    stdout
        .execute(EnterAlternateScreen)
        .map_err(|e| format!("No se pudo abrir la pantalla alternativa: {}", e))?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal =
        Terminal::new(backend).map_err(|e| format!("No se pudo inicializar el terminal: {}", e))?;

    let result = run_loop(&mut terminal, config, sync_interval);

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
    sync_interval: u64,
    next_sync_deadline: Instant,
}

impl<'a> RepoManager<'a> {
    fn new(config: &'a Config, sync_interval: u64) -> Self {
        let repos = config.read_repos();
        let mut list_state = ListState::default();
        if !repos.is_empty() {
            list_state.select(Some(0));
        }

        let safe_interval = sync_interval.max(1);

        RepoManager {
            config,
            repos,
            list_state,
            input_mode: InputMode::Normal,
            input: String::new(),
            message: None,
            sync_interval: safe_interval,
            next_sync_deadline: Instant::now() + Duration::from_secs(safe_interval),
        }
    }

    fn tick(&mut self) {
        let now = Instant::now();
        while self.next_sync_deadline <= now {
            self.next_sync_deadline += Duration::from_secs(self.sync_interval);
        }
    }

    fn seconds_until_next_sync(&self) -> u64 {
        self.next_sync_deadline
            .saturating_duration_since(Instant::now())
            .as_secs()
    }

    fn build_count(&self) -> usize {
        self.repos
            .iter()
            .filter(|repo| repo.deploy_target.as_ref().is_some_and(|target| !target.is_empty()))
            .count()
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
        self.input_mode = InputMode::ChoosingBuildType;
        self.input.clear();
        self.set_message(
            "¿Requiere compilación? 1) No • 2) Sí (build + deploy de dist)",
            Color::Cyan,
        );
    }

    fn start_edit(&mut self) {
        if let Some(index) = self.list_state.selected() {
            if let Some(repo) = self.repos.get(index) {
                self.input_mode = InputMode::EditingSource(index);
                self.input = repo.repo_path.clone();
                self.set_message(
                    "Edita la ruta local del repositorio seleccionado",
                    Color::Cyan,
                );
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
            InputMode::AddingSource { requires_build } => {
                if input_value.is_empty() {
                    self.set_message("La ruta del repositorio no puede estar vacía", Color::Red);
                    return Ok(());
                }

                if requires_build {
                    self.begin_destination_input(input_value);
                } else {
                    return self.finalize_simple_repo(input_value);
                }
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
                self.set_message("Repositorio con build añadido", Color::Green);
                self.input_mode = InputMode::Normal;
                self.input.clear();
            }
            InputMode::ChoosingBuildType => {}
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
                    "Ruta destino de build (vacío para desactivar build)",
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
                        "Repositorio actualizado (build activo)"
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

    fn begin_source_input(&mut self, requires_build: bool) {
        self.input_mode = InputMode::AddingSource { requires_build };
        self.input.clear();
        if requires_build {
            self.set_message(
                "Ruta origen del proyecto a compilar (ej. /root/proyects/mi-app)",
                Color::Cyan,
            );
        } else {
            self.set_message(
                "Ruta del repositorio sin compilación (ej. /var/www/html/mi-app)",
                Color::Cyan,
            );
        }
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
            "Ruta destino de build (ej. /var/www/html/mi-app/public). Vacío = simple",
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

    fn mode_hint(&self) -> &'static str {
        match self.input_mode {
            InputMode::Normal => "Normal",
            InputMode::ChoosingBuildType => "Elegir tipo",
            InputMode::AddingSource { .. } => "Agregar origen",
            InputMode::AddingDestination { .. } => "Agregar destino",
            InputMode::EditingSource(_) => "Editar origen",
            InputMode::EditingDestination { .. } => "Editar destino",
        }
    }
}

fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    config: &Config,
    sync_interval: u64,
) -> Result<(), String> {
    let mut manager = RepoManager::new(config, sync_interval);

    loop {
        manager.tick();

        terminal
            .draw(|frame| draw_ui(frame, &mut manager))
            .map_err(|e| format!("No se pudo renderizar la interfaz: {}", e))?;

        if !event::poll(Duration::from_millis(250))
            .map_err(|e| format!("No se pudo leer el evento de entrada: {}", e))?
        {
            continue;
        }

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
                    InputMode::ChoosingBuildType => match code {
                        KeyCode::Char('1') | KeyCode::Char('n') | KeyCode::Char('N') => {
                            manager.begin_source_input(false)
                        }
                        KeyCode::Char('2') | KeyCode::Char('s') | KeyCode::Char('S') => {
                            manager.begin_source_input(true)
                        }
                        KeyCode::Esc => manager.cancel_input(),
                        _ => {}
                    },
                    InputMode::AddingSource { .. }
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
                Constraint::Length(3),
                Constraint::Min(8),
                Constraint::Length(3),
                Constraint::Length(3),
                Constraint::Length(2),
            ]
            .as_ref(),
        )
        .split(size);

    let remaining = manager.seconds_until_next_sync();
    let header = Paragraph::new(Line::from(vec![
        Span::styled(" Git Sync ", Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD)),
        Span::raw("  |  "),
        Span::styled(
            format!("Modo: {}", manager.mode_hint()),
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
        ),
        Span::raw("  |  "),
        Span::styled(
            format!("Próximo ciclo: {}s", remaining),
            Style::default().fg(Color::Green),
        ),
        Span::raw("  |  "),
        Span::styled(
            format!("Hora: {}", Local::now().format("%H:%M:%S")),
            Style::default().fg(Color::White),
        ),
    ]))
    .block(Block::default().borders(Borders::ALL).title("Dashboard"));
    frame.render_widget(header, chunks[0]);

    let body_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(70), Constraint::Percentage(30)].as_ref())
        .split(chunks[1]);

    let items: Vec<ListItem> = if manager.repos.is_empty() {
        vec![ListItem::new(Line::from(vec![Span::styled(
            "No hay repositorios configurados",
            Style::default().fg(Color::DarkGray),
        )]))]
    } else {
        manager
            .repos
            .iter()
            .enumerate()
            .map(|(i, repo)| {
                let base_style = Style::default().fg(Color::White);
                let label = match &repo.deploy_target {
                    Some(target) if !target.is_empty() => {
                        Line::from(vec![
                            Span::styled(format!("{:>2}. ", i + 1), Style::default().fg(Color::DarkGray)),
                            Span::styled("BUILD ", Style::default().fg(Color::Black).bg(Color::Green)),
                            Span::styled(format!(" {} -> {}", repo.repo_path, target), base_style),
                        ])
                    }
                    _ => Line::from(vec![
                        Span::styled(format!("{:>2}. ", i + 1), Style::default().fg(Color::DarkGray)),
                        Span::styled("SYNC ", Style::default().fg(Color::Black).bg(Color::Blue)),
                        Span::styled(format!(" {}", repo.repo_path), base_style),
                    ]),
                };
                ListItem::new(label)
            })
            .collect()
    };

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!("Repositorios ({})", manager.repos.len())),
        )
        .highlight_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("❯ ");
    frame.render_stateful_widget(list, body_chunks[0], &mut manager.list_state);

    let build_count = manager.build_count();
    let sync_only_count = manager.repos.len().saturating_sub(build_count);
    let panel_lines = vec![
        Line::from(vec![Span::styled(
            "Resumen",
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        )]),
        Line::from(format!("Total: {}", manager.repos.len())),
        Line::from(format!("Con build: {}", build_count)),
        Line::from(format!("Solo sync: {}", sync_only_count)),
        Line::from(""),
        Line::from(vec![Span::styled(
            "Daemon",
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        )]),
        Line::from(format!("Intervalo: {}s", manager.sync_interval)),
        Line::from(format!("Próximo ciclo: {}s", remaining)),
        Line::from(format!("Fecha: {}", Local::now().format("%Y-%m-%d"))),
    ];
    let panel = Paragraph::new(panel_lines)
        .style(Style::default().fg(Color::White))
        .block(Block::default().borders(Borders::ALL).title("Estado"));
    frame.render_widget(panel, body_chunks[1]);

    let (input_text, input_title) = match manager.input_mode {
        InputMode::Normal => ("".to_string(), "Entrada"),
        InputMode::AddingSource {
            requires_build: false,
        } => (
            manager.input.clone(),
            "Ruta origen sin compilación (ej. /var/www/html/mi-app)",
        ),
        InputMode::AddingSource {
            requires_build: true,
        } => (
            manager.input.clone(),
            "Ruta origen para compilar (ej. /root/proyects/mi-app)",
        ),
        InputMode::EditingSource(_) => (
            manager.input.clone(),
            "Ruta origen (sin compilación: /var/www/html/mi-app | con compilación: /root/proyects/mi-app)",
        ),
        InputMode::AddingDestination { .. } | InputMode::EditingDestination { .. } => (
            manager.input.clone(),
            "Ruta destino de build (ej. /var/www/html/mi-app/public)",
        ),
        InputMode::ChoosingBuildType => (
            "1 = Sin compilación | 2 = Con compilación".to_string(),
            "Tipo de repositorio",
        ),
    };

    let input_block = Paragraph::new(input_text)
        .style(Style::default().fg(Color::White))
        .block(Block::default().borders(Borders::ALL).title(input_title));
    frame.render_widget(input_block, chunks[2]);

    if matches!(
        manager.input_mode,
        InputMode::AddingSource { .. }
            | InputMode::AddingDestination { .. }
            | InputMode::EditingSource(_)
            | InputMode::EditingDestination { .. }
    ) {
        frame.set_cursor(
            chunks[2].x + manager.input.len() as u16 + 1,
            chunks[2].y + 1,
        );
    }

    let (status_text, status_color) = manager
        .message
        .clone()
        .unwrap_or(("Listo para editar repositorios".to_string(), Color::DarkGray));
    let status = Paragraph::new(status_text)
        .style(Style::default().fg(status_color))
        .block(Block::default().borders(Borders::ALL).title("Estado"));
    frame.render_widget(status, chunks[3]);

    let shortcuts = Paragraph::new(
        "↑/↓ mover  a añadir  e/Enter editar  d eliminar  Esc cancelar  q salir  Ctrl+C salir",
    )
    .style(Style::default().fg(Color::Black).bg(Color::Gray));
    frame.render_widget(shortcuts, chunks[4]);
}
