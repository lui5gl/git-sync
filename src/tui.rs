use crate::config::{Config, RepoDefinition};
use crate::sync_state::{RepoSyncState, SyncStateSnapshot};
use chrono::Local;
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
use std::time::{Duration, Instant};

#[derive(Clone)]
enum InputMode {
    Normal,
    AddingSource,
    EditingSource(usize),
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
    sync_state: SyncStateSnapshot,
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
            sync_state: SyncStateSnapshot::load(&config.state_file),
        }
    }

    fn tick(&mut self) {
        let now = Instant::now();
        while self.next_sync_deadline <= now {
            self.next_sync_deadline += Duration::from_secs(self.sync_interval);
        }
        self.sync_state = SyncStateSnapshot::load(&self.config.state_file);
    }

    fn seconds_until_next_sync(&self) -> u64 {
        self.next_sync_deadline
            .saturating_duration_since(Instant::now())
            .as_secs()
    }

    fn selected_repo(&self) -> Option<&RepoDefinition> {
        let selected = self.list_state.selected()?;
        self.repos.get(selected)
    }

    fn selected_repo_state(&self) -> Option<&RepoSyncState> {
        let repo = self.selected_repo()?;
        self.sync_state.get(&repo.repo_path)
    }

    fn error_count(&self) -> usize {
        self.repos
            .iter()
            .filter(|repo| {
                self.sync_state
                    .get(&repo.repo_path)
                    .is_some_and(repo_has_active_error)
            })
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
        self.input_mode = InputMode::AddingSource;
        self.input.clear();
        self.set_message(
            "Ruta local del repositorio a sincronizar (ej. /var/www/html/mi-app)",
            Color::Cyan,
        );
    }

    fn start_edit(&mut self) {
        if let Some(index) = self.list_state.selected()
            && let Some(repo) = self.repos.get(index)
        {
            self.input_mode = InputMode::EditingSource(index);
            self.input = repo.repo_path.clone();
            self.set_message(
                "Edita la ruta local del repositorio seleccionado",
                Color::Cyan,
            );
        }
    }

    fn delete_selected(&mut self) -> Result<(), String> {
        if let Some(index) = self.list_state.selected()
            && index < self.repos.len()
        {
            self.repos.remove(index);
            self.persist()?;
            if self.repos.is_empty() {
                self.list_state.select(None);
            } else if index >= self.repos.len() {
                self.list_state.select(Some(self.repos.len() - 1));
            }
            self.set_message("Repositorio eliminado", Color::Yellow);
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

                self.repos.push(RepoDefinition::new(input_value));
                self.persist()?;
                self.list_state.select(Some(self.repos.len() - 1));
                self.set_message("Repositorio añadido", Color::Green);
                self.input_mode = InputMode::Normal;
                self.input.clear();
            }
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

                if let Some(repo) = self.repos.get_mut(index) {
                    repo.repo_path = input_value;
                }
                self.persist()?;
                self.set_message("Repositorio actualizado", Color::Green);
                self.input_mode = InputMode::Normal;
                self.input.clear();
            }
            InputMode::Normal => {}
        }

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

    fn mode_hint(&self) -> &'static str {
        match self.input_mode {
            InputMode::Normal => "Normal",
            InputMode::AddingSource => "Agregar",
            InputMode::EditingSource(_) => "Editar",
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

        if let Event::Key(KeyEvent {
            code, modifiers, ..
        }) = event::read().map_err(|e| format!("No se pudo leer el evento de entrada: {}", e))?
        {
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
                InputMode::AddingSource | InputMode::EditingSource(_) => match code {
                    KeyCode::Enter => manager.submit()?,
                    KeyCode::Esc => manager.cancel_input(),
                    KeyCode::Backspace => manager.backspace(),
                    KeyCode::Char(c) => manager.add_char(c),
                    KeyCode::Left | KeyCode::Right | KeyCode::Home | KeyCode::End => {}
                    _ => {}
                },
            }
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
                Constraint::Length(3),
            ]
            .as_ref(),
        )
        .split(size);

    let remaining = manager.seconds_until_next_sync();
    let header = Paragraph::new(Line::from(vec![
        Span::styled(
            " Git Sync ",
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  |  "),
        Span::styled(
            format!("Modo: {}", manager.mode_hint()),
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
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

    let now_ts = Local::now().timestamp();
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
                let state = manager.sync_state.get(&repo.repo_path);
                let (status_label, status_style) = match state {
                    Some(repo_state) if repo_has_active_error(repo_state) => {
                        (" ERROR ", Style::default().fg(Color::White).bg(Color::Red))
                    }
                    Some(_) => (" OK ", Style::default().fg(Color::Black).bg(Color::Green)),
                    None => (
                        " SIN DATOS ",
                        Style::default().fg(Color::Black).bg(Color::DarkGray),
                    ),
                };
                let last_sync_label = state
                    .and_then(|s| s.last_success_ts)
                    .map(|ts| format!("hace {}", humanize_elapsed(now_ts.saturating_sub(ts))))
                    .unwrap_or_else(|| "sin registros".to_string());

                let label = Line::from(vec![
                    Span::styled(
                        format!("{:>2}. ", i + 1),
                        Style::default().fg(Color::DarkGray),
                    ),
                    Span::styled("SYNC ", Style::default().fg(Color::Black).bg(Color::Blue)),
                    Span::styled(format!(" {}", repo.repo_path), base_style),
                    Span::raw("  | "),
                    Span::styled(status_label, status_style),
                    Span::raw(format!("  |  Últ. sync: {}", last_sync_label)),
                ]);
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

    let error_count = manager.error_count();
    let selected_state = manager.selected_repo_state();
    let selected_last_sync = selected_state
        .and_then(|state| state.last_success_ts)
        .map(|ts| format!("hace {}", humanize_elapsed(now_ts.saturating_sub(ts))))
        .unwrap_or_else(|| "sin registros".to_string());
    let selected_last_attempt = selected_state
        .and_then(|state| state.last_attempt_ts)
        .map(|ts| format!("hace {}", humanize_elapsed(now_ts.saturating_sub(ts))))
        .unwrap_or_else(|| "sin intentos".to_string());
    let selected_status = match selected_state {
        Some(state) if repo_has_active_error(state) => "Error en último intento",
        Some(_) => "Correcto",
        None => "Sin datos",
    };
    let selected_error = selected_state
        .and_then(|state| state.last_error.clone())
        .map(|err| truncate_message(&err, 72))
        .unwrap_or_else(|| "-".to_string());
    let selected_result = selected_state
        .and_then(|state| state.last_result.clone())
        .map(|result| truncate_message(&result, 72))
        .unwrap_or_else(|| "-".to_string());

    let panel_lines = vec![
        Line::from(vec![Span::styled(
            "Resumen",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )]),
        Line::from(format!("Total: {}", manager.repos.len())),
        Line::from(format!("Con error: {}", error_count)),
        Line::from(""),
        Line::from(vec![Span::styled(
            "Daemon",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )]),
        Line::from(format!("Intervalo: {}s", manager.sync_interval)),
        Line::from(format!("Próximo ciclo: {}s", remaining)),
        Line::from(format!("Fecha: {}", Local::now().format("%Y-%m-%d"))),
        Line::from(""),
        Line::from(vec![Span::styled(
            "Seleccionado",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )]),
        Line::from(format!("Estado: {}", selected_status)),
        Line::from(format!("Último resultado: {}", selected_result)),
        Line::from(format!("Último sync OK: {}", selected_last_sync)),
        Line::from(format!("Último intento: {}", selected_last_attempt)),
        Line::from(format!("Último error: {}", selected_error)),
    ];
    let panel = Paragraph::new(panel_lines)
        .style(Style::default().fg(Color::White))
        .block(Block::default().borders(Borders::ALL).title("Estado"));
    frame.render_widget(panel, body_chunks[1]);

    let (input_text, input_title) = match manager.input_mode {
        InputMode::Normal => ("".to_string(), "Entrada"),
        InputMode::AddingSource | InputMode::EditingSource(_) => (
            manager.input.clone(),
            "Ruta del repositorio (ej. /var/www/html/mi-app)",
        ),
    };

    let input_block = Paragraph::new(input_text)
        .style(Style::default().fg(Color::White))
        .block(Block::default().borders(Borders::ALL).title(input_title));
    frame.render_widget(input_block, chunks[2]);

    if matches!(
        manager.input_mode,
        InputMode::AddingSource | InputMode::EditingSource(_)
    ) {
        frame.set_cursor(
            chunks[2].x + manager.input.len() as u16 + 1,
            chunks[2].y + 1,
        );
    }

    let (status_text, status_color) = manager.message.clone().unwrap_or((
        "Listo para editar repositorios".to_string(),
        Color::DarkGray,
    ));
    let status = Paragraph::new(status_text)
        .style(Style::default().fg(status_color))
        .block(Block::default().borders(Borders::ALL).title("Estado"));
    frame.render_widget(status, chunks[3]);

    let shortcuts = Paragraph::new(Line::from(vec![
        Span::styled(
            " ↑/↓ ",
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" mover  "),
        Span::styled(
            " A ",
            Style::default()
                .fg(Color::Black)
                .bg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" añadir  "),
        Span::styled(
            " E/Enter ",
            Style::default()
                .fg(Color::Black)
                .bg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" editar  "),
        Span::styled(
            " D ",
            Style::default()
                .fg(Color::White)
                .bg(Color::Red)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" eliminar  "),
        Span::styled(
            " Esc ",
            Style::default()
                .fg(Color::Black)
                .bg(Color::Gray)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" cancelar  "),
        Span::styled(
            " Q ",
            Style::default()
                .fg(Color::Black)
                .bg(Color::Magenta)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" salir  "),
        Span::styled(
            " Ctrl+C ",
            Style::default()
                .fg(Color::Black)
                .bg(Color::Blue)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" salir"),
    ]))
    .style(Style::default().fg(Color::White))
    .block(Block::default().borders(Borders::ALL).title("Atajos"));
    frame.render_widget(shortcuts, chunks[4]);
}

fn repo_has_active_error(state: &RepoSyncState) -> bool {
    match (state.last_error_ts, state.last_success_ts) {
        (Some(error_ts), Some(success_ts)) => error_ts > success_ts,
        (Some(_), None) => true,
        _ => false,
    }
}

fn humanize_elapsed(seconds: i64) -> String {
    if seconds <= 1 {
        return "1s".to_string();
    }

    if seconds < 60 {
        return format!("{}s", seconds);
    }

    let minutes = seconds / 60;
    if minutes < 60 {
        return format!("{}m", minutes);
    }

    let hours = minutes / 60;
    if hours < 24 {
        return format!("{}h", hours);
    }

    let days = hours / 24;
    format!("{}d", days)
}

fn truncate_message(message: &str, max_chars: usize) -> String {
    let mut out = String::new();

    for (count, ch) in message.chars().enumerate() {
        if count >= max_chars {
            out.push('…');
            return out;
        }
        out.push(ch);
    }

    out
}
