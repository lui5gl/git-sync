use crate::config::{Config, RepoDefinition};
use crate::git::GitRepo;
use crate::logger::Logger;
use crate::processor::RepoProcessor;
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
use std::collections::HashMap;
use std::io::{Stdout, stdout};
use std::path::Path;
use std::time::{Duration, Instant};

#[derive(Clone)]
enum InputMode {
    Normal,
    AddingSource,
    EditingSource(usize),
}

#[derive(Clone, Debug, Default)]
struct RepoRefreshStatus {
    branch: Option<String>,
    commits_behind: Option<usize>,
    last_refresh_ts: Option<i64>,
    last_error: Option<String>,
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
    refresh_cursor: usize,
    last_refresh_step: Instant,
    refresh_step_interval: Duration,
    sync_state: SyncStateSnapshot,
    refresh_status: HashMap<String, RepoRefreshStatus>,
    details_open: bool,
    details_lines: Vec<String>,
    details_repo_path: Option<String>,
}

impl<'a> RepoManager<'a> {
    fn new(config: &'a Config, sync_interval: u64) -> Self {
        let repos = config.read_repos();
        let mut list_state = ListState::default();
        if !repos.is_empty() {
            list_state.select(Some(0));
        }

        let safe_interval = sync_interval.max(1);
        let active_count = repos.iter().filter(|repo| repo.enabled).count().max(1) as u64;
        let per_repo_ms = ((safe_interval * 1000) / active_count).clamp(800, 4000);

        RepoManager {
            config,
            repos,
            list_state,
            input_mode: InputMode::Normal,
            input: String::new(),
            message: None,
            sync_interval: safe_interval,
            refresh_cursor: 0,
            last_refresh_step: Instant::now(),
            refresh_step_interval: Duration::from_millis(per_repo_ms),
            sync_state: SyncStateSnapshot::load(&config.state_file),
            refresh_status: HashMap::new(),
            details_open: false,
            details_lines: vec![
                "Pulse Espacio para ver detalles del repositorio seleccionado.".to_string(),
            ],
            details_repo_path: None,
        }
    }

    fn tick(&mut self) {
        self.sync_state = SyncStateSnapshot::load(&self.config.state_file);

        if self.last_refresh_step.elapsed() >= self.refresh_step_interval {
            self.refresh_next_status();
            self.last_refresh_step = Instant::now();
        }

        if self.details_open {
            let selected_path = self.selected_repo().map(|repo| repo.repo_path.clone());
            if selected_path != self.details_repo_path {
                self.refresh_details();
            }
        }
    }

    fn selected_repo(&self) -> Option<&RepoDefinition> {
        let selected = self.list_state.selected()?;
        self.repos.get(selected)
    }

    fn selected_repo_state(&self) -> Option<&RepoSyncState> {
        let repo = self.selected_repo()?;
        self.sync_state.get(&repo.repo_path)
    }

    fn selected_refresh_status(&self) -> Option<&RepoRefreshStatus> {
        let repo = self.selected_repo()?;
        self.refresh_status.get(&repo.repo_path)
    }

    fn error_count(&self) -> usize {
        self.repos
            .iter()
            .filter(|repo| repo.enabled)
            .filter(|repo| {
                self.sync_state
                    .get(&repo.repo_path)
                    .is_some_and(repo_has_active_error)
            })
            .count()
    }

    fn paused_count(&self) -> usize {
        self.repos.iter().filter(|repo| !repo.enabled).count()
    }

    fn recompute_refresh_interval(&mut self) {
        let active_count = self.repos.iter().filter(|repo| repo.enabled).count().max(1) as u64;
        let per_repo_ms = ((self.sync_interval.max(1) * 1000) / active_count).clamp(800, 4000);
        self.refresh_step_interval = Duration::from_millis(per_repo_ms);
    }

    fn refresh_next_status(&mut self) {
        if self.repos.is_empty() {
            return;
        }

        for _ in 0..self.repos.len() {
            let index = self.refresh_cursor % self.repos.len();
            self.refresh_cursor = (self.refresh_cursor + 1) % self.repos.len();

            if let Some(repo) = self.repos.get(index) {
                if !repo.enabled {
                    continue;
                }
                let repo_clone = repo.clone();
                self.refresh_repo_status(&repo_clone);
                if self.details_open {
                    self.refresh_details();
                }
                break;
            }
        }
    }

    fn outdated_count(&self) -> usize {
        self.repos
            .iter()
            .filter(|repo| repo.enabled)
            .filter(|repo| {
                self.refresh_status
                    .get(&repo.repo_path)
                    .and_then(|status| status.commits_behind)
                    .is_some_and(|count| count > 0)
            })
            .count()
    }

    fn refresh_all_status(&mut self) {
        let repos = self.repos.clone();
        for repo in repos {
            if !repo.enabled {
                continue;
            }
            self.refresh_repo_status(&repo);
        }

        if self.details_open {
            self.refresh_details();
        }
    }

    fn refresh_repo_status(&mut self, repo: &RepoDefinition) {
        let now_ts = Local::now().timestamp();

        if !Path::new(&repo.repo_path).exists() {
            self.refresh_status.insert(
                repo.repo_path.clone(),
                RepoRefreshStatus {
                    last_refresh_ts: Some(now_ts),
                    last_error: Some("La ruta no existe".to_string()),
                    ..RepoRefreshStatus::default()
                },
            );
            return;
        }

        if !Path::new(&format!("{}/.git", repo.repo_path)).exists() {
            self.refresh_status.insert(
                repo.repo_path.clone(),
                RepoRefreshStatus {
                    last_refresh_ts: Some(now_ts),
                    last_error: Some("No es un repositorio Git válido".to_string()),
                    ..RepoRefreshStatus::default()
                },
            );
            return;
        }

        let git_repo = GitRepo::new(repo.repo_path.clone());
        let result = git_repo
            .fetch()
            .and_then(|_| {
                let branch = git_repo.get_default_branch();
                let behind = git_repo.count_commits_behind(&branch)?;
                Ok((branch, behind))
            })
            .map_err(|err| err.to_string());

        match result {
            Ok((branch, behind)) => {
                self.refresh_status.insert(
                    repo.repo_path.clone(),
                    RepoRefreshStatus {
                        branch: Some(branch),
                        commits_behind: Some(behind),
                        last_refresh_ts: Some(now_ts),
                        last_error: None,
                    },
                );
            }
            Err(err) => {
                self.refresh_status.insert(
                    repo.repo_path.clone(),
                    RepoRefreshStatus {
                        last_refresh_ts: Some(now_ts),
                        last_error: Some(truncate_message(&err, 120)),
                        ..RepoRefreshStatus::default()
                    },
                );
            }
        }
    }

    fn select_next(&mut self) {
        let next_index = match self.list_state.selected() {
            Some(i) if !self.repos.is_empty() => (i + 1).min(self.repos.len() - 1),
            _ => 0,
        };
        if !self.repos.is_empty() {
            self.list_state.select(Some(next_index));
            if self.details_open {
                self.refresh_details();
            }
        }
    }

    fn select_previous(&mut self) {
        let prev_index = match self.list_state.selected() {
            Some(i) if i > 0 => i - 1,
            _ => 0,
        };
        if !self.repos.is_empty() {
            self.list_state.select(Some(prev_index));
            if self.details_open {
                self.refresh_details();
            }
        }
    }

    fn toggle_details(&mut self) {
        self.details_open = !self.details_open;
        if self.details_open {
            self.refresh_details();
            self.set_message(
                "Vista detallada activada (últimos commits y errores)",
                Color::Cyan,
            );
        } else {
            self.set_message("Vista detallada oculta", Color::DarkGray);
        }
    }

    fn refresh_details(&mut self) {
        self.details_lines.clear();

        let Some(repo_path) = self.selected_repo().map(|repo| repo.repo_path.clone()) else {
            self.details_repo_path = None;
            self.details_lines
                .push("No hay repositorio seleccionado.".to_string());
            return;
        };
        let repo_enabled = self
            .selected_repo()
            .map(|repo| repo.enabled)
            .unwrap_or(true);

        self.details_repo_path = Some(repo_path.clone());
        self.details_lines
            .push(format!("Repositorio: {}", repo_path));
        self.details_lines.push(format!(
            "Sync: {}",
            if repo_enabled {
                "Activo"
            } else {
                "Desactivado"
            }
        ));

        let state = self.sync_state.get(&repo_path);
        let refresh = self.refresh_status.get(&repo_path);
        let branch = refresh
            .and_then(|s| s.branch.clone())
            .or_else(|| state.and_then(|s| s.last_branch.clone()))
            .unwrap_or_else(|| "-".to_string());
        self.details_lines
            .push(format!("Rama detectada: {}", branch));

        let pending = refresh
            .and_then(|s| s.commits_behind)
            .map(|count| count.to_string())
            .unwrap_or_else(|| "-".to_string());
        self.details_lines
            .push(format!("Commits pendientes (remote): {}", pending));

        if let Some(ts) = refresh.and_then(|s| s.last_refresh_ts) {
            self.details_lines.push(format!(
                "Último refresh remoto: hace {}",
                humanize_elapsed(Local::now().timestamp().saturating_sub(ts))
            ));
        } else {
            self.details_lines
                .push("Último refresh remoto: sin datos".to_string());
        }

        if let Some(refresh_error) = refresh.and_then(|s| s.last_error.clone()) {
            self.details_lines.push(format!(
                "Error de refresh: {}",
                truncate_message(&refresh_error, 120)
            ));
        }

        if let Some(last_pull) = state.and_then(|s| s.last_pulled_commit.clone()) {
            self.details_lines
                .push(format!("Último commit aplicado en pull: {}", last_pull));
        } else {
            self.details_lines
                .push("Último commit aplicado en pull: sin datos".to_string());
        }

        if let Some(last_error) = state.and_then(|s| s.last_error.clone()) {
            self.details_lines
                .push("Último error detallado:".to_string());
            for line in wrap_text(&last_error, 120, 3) {
                self.details_lines.push(format!("  {}", line));
            }
        } else {
            self.details_lines
                .push("Último error detallado: sin errores registrados".to_string());
        }

        if !Path::new(&repo_path).exists() {
            self.details_lines
                .push("No se puede leer commits: la ruta no existe".to_string());
            return;
        }

        if !Path::new(&format!("{}/.git", repo_path)).exists() {
            self.details_lines
                .push("No se puede leer commits: no es un repositorio Git válido".to_string());
            return;
        }

        let git_repo = GitRepo::new(repo_path);
        match git_repo.recent_commits(5) {
            Ok(commits) if commits.is_empty() => {
                self.details_lines
                    .push("Últimos commits: sin historial".to_string());
            }
            Ok(commits) => {
                self.details_lines
                    .push("Últimos commits locales:".to_string());
                for commit in commits {
                    self.details_lines.push(format!("  - {}", commit));
                }
            }
            Err(err) => {
                self.details_lines
                    .push(format!("No se pudieron leer commits: {}", err));
            }
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
            self.recompute_refresh_interval();
            if self.repos.is_empty() {
                self.list_state.select(None);
            } else if index >= self.repos.len() {
                self.list_state.select(Some(self.repos.len() - 1));
            }
            self.set_message("Repositorio eliminado", Color::Yellow);
        }
        Ok(())
    }

    fn toggle_selected_sync(&mut self) -> Result<(), String> {
        let Some(index) = self.list_state.selected() else {
            return Ok(());
        };
        if index >= self.repos.len() {
            return Ok(());
        }

        let mut enabled = true;
        if let Some(repo) = self.repos.get_mut(index) {
            repo.enabled = !repo.enabled;
            enabled = repo.enabled;
        }

        let label = if enabled {
            "Sync activado para el repositorio"
        } else {
            "Sync desactivado para el repositorio"
        };
        self.set_message(label, if enabled { Color::Green } else { Color::Yellow });

        self.persist()?;
        self.recompute_refresh_interval();
        if self.details_open {
            self.refresh_details();
        }
        Ok(())
    }

    fn sync_selected_now(&mut self) -> Result<(), String> {
        let Some(repo) = self.selected_repo().cloned() else {
            return Ok(());
        };

        if !repo.enabled {
            self.set_message(
                "El repositorio está pausado. Actívelo con 's' antes de sincronizar.",
                Color::Yellow,
            );
            return Ok(());
        }

        self.run_sync_now(vec![repo], "Repositorio sincronizado")
    }

    fn sync_all_now(&mut self) -> Result<(), String> {
        let repos = self
            .repos
            .iter()
            .filter(|repo| repo.enabled)
            .cloned()
            .collect::<Vec<_>>();

        if repos.is_empty() {
            self.set_message(
                "No hay repositorios activos para sincronizar",
                Color::Yellow,
            );
            return Ok(());
        }

        self.run_sync_now(repos, "Sincronización manual completada")
    }

    fn selected_repo_path(&self) -> Option<String> {
        self.selected_repo().map(|repo| repo.repo_path.clone())
    }

    fn run_sync_now(
        &mut self,
        repos: Vec<RepoDefinition>,
        success_message: &str,
    ) -> Result<(), String> {
        self.set_message("Sincronizando...", Color::Cyan);
        let logger = Logger::new(self.config.log_file.clone());
        let processor = RepoProcessor::new(&logger, false, self.config.state_file.clone());

        match processor.process_all(repos) {
            Ok(_) => {
                self.sync_state = SyncStateSnapshot::load(&self.config.state_file);
                if self.details_open {
                    self.refresh_details();
                }
                self.set_message(success_message, Color::Green);
                Ok(())
            }
            Err(err) => {
                self.sync_state = SyncStateSnapshot::load(&self.config.state_file);
                if self.details_open {
                    self.refresh_details();
                }
                self.set_message(
                    truncate_message(&format!("Falló sincronización: {}", err), 120),
                    Color::Red,
                );
                Ok(())
            }
        }
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
                self.recompute_refresh_interval();
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
                self.recompute_refresh_interval();
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
    manager.refresh_all_status();

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
                    KeyCode::Char('s') => manager.toggle_selected_sync()?,
                    KeyCode::Char('u') => manager.sync_selected_now()?,
                    KeyCode::Char('U') => manager.sync_all_now()?,
                    KeyCode::Char('v') => {
                        if let Some(path) = manager.selected_repo_path() {
                            open_shell_in_repo(terminal, &path)?;
                            manager.set_message(
                                "Sesión de shell finalizada. Regresaste a git-sync.",
                                Color::Green,
                            );
                        } else {
                            manager.set_message("No hay repositorio seleccionado", Color::Yellow);
                        }
                    }
                    KeyCode::Char(' ') => manager.toggle_details(),
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

fn open_shell_in_repo(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    repo_path: &str,
) -> Result<(), String> {
    if !Path::new(repo_path).exists() {
        return Err(format!("La ruta no existe: {}", repo_path));
    }

    disable_raw_mode().map_err(|e| format!("No se pudo salir de modo raw: {}", e))?;
    terminal
        .backend_mut()
        .execute(LeaveAlternateScreen)
        .map_err(|e| format!("No se pudo salir de la pantalla alternativa: {}", e))?;

    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string());
    let shell_result = std::process::Command::new(&shell)
        .current_dir(repo_path)
        .status()
        .map_err(|e| format!("No se pudo abrir shell en {}: {}", repo_path, e));

    terminal
        .backend_mut()
        .execute(EnterAlternateScreen)
        .map_err(|e| format!("No se pudo restaurar pantalla alternativa: {}", e))?;
    enable_raw_mode().map_err(|e| format!("No se pudo reactivar modo raw: {}", e))?;

    match shell_result {
        Ok(status) if status.success() => Ok(()),
        Ok(_) => Err("La sesión de shell terminó con error".to_string()),
        Err(err) => Err(err),
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
                if manager.details_open {
                    Constraint::Length(10)
                } else {
                    Constraint::Length(3)
                },
                Constraint::Length(3),
                Constraint::Length(3),
                Constraint::Length(3),
            ]
            .as_ref(),
        )
        .split(size);

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
            format!(
                "Refresh auto secuencial ~{:.1}s/repo | Pull: u/U",
                manager.refresh_step_interval.as_millis() as f64 / 1000.0
            ),
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
                let refresh = manager.refresh_status.get(&repo.repo_path);
                let (status_label, status_style) = match (repo.enabled, refresh, state) {
                    _ if !repo.enabled => (
                        " PAUSADO ",
                        Style::default().fg(Color::Black).bg(Color::Yellow),
                    ),
                    (true, Some(remote), _) if remote.last_error.is_some() => (
                        " ERROR REFRESH ",
                        Style::default().fg(Color::White).bg(Color::Red),
                    ),
                    (true, Some(remote), _) if remote.commits_behind.unwrap_or(0) > 0 => (
                        " DESACTUALIZADO ",
                        Style::default().fg(Color::Black).bg(Color::Yellow),
                    ),
                    (true, _, Some(repo_state)) if repo_has_active_error(repo_state) => {
                        (" ERROR ", Style::default().fg(Color::White).bg(Color::Red))
                    }
                    (true, Some(_), _) => (
                        " ACTUALIZADO ",
                        Style::default().fg(Color::Black).bg(Color::Green),
                    ),
                    _ => (
                        " SIN REFRESH ",
                        Style::default().fg(Color::Black).bg(Color::DarkGray),
                    ),
                };
                let last_sync_label = state
                    .and_then(|s| s.last_success_ts)
                    .map(|ts| format!("hace {}", humanize_elapsed(now_ts.saturating_sub(ts))))
                    .unwrap_or_else(|| "sin registros".to_string());
                let branch_label = refresh
                    .and_then(|s| s.branch.clone())
                    .or_else(|| state.and_then(|s| s.last_branch.clone()))
                    .unwrap_or_else(|| "?".to_string());
                let behind_label = refresh
                    .and_then(|s| s.commits_behind)
                    .map(|count| count.to_string())
                    .unwrap_or_else(|| "?".to_string());
                let label = Line::from(vec![
                    Span::styled(
                        format!("{:>2}. ", i + 1),
                        Style::default().fg(Color::DarkGray),
                    ),
                    Span::styled("SYNC ", Style::default().fg(Color::Black).bg(Color::Blue)),
                    Span::styled(format!(" {}", repo.repo_path), base_style),
                    Span::raw("  | "),
                    Span::styled(status_label, status_style),
                    Span::raw(format!(
                        "  |  Rama: {}  |  Behind: {}  |  Últ. sync: {}",
                        branch_label, behind_label, last_sync_label
                    )),
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
    let paused_count = manager.paused_count();
    let outdated_count = manager.outdated_count();
    let selected_state = manager.selected_repo_state();
    let selected_refresh = manager.selected_refresh_status();
    let selected_last_sync = selected_state
        .and_then(|state| state.last_success_ts)
        .map(|ts| format!("hace {}", humanize_elapsed(now_ts.saturating_sub(ts))))
        .unwrap_or_else(|| "sin registros".to_string());
    let selected_last_attempt = selected_state
        .and_then(|state| state.last_attempt_ts)
        .map(|ts| format!("hace {}", humanize_elapsed(now_ts.saturating_sub(ts))))
        .unwrap_or_else(|| "sin intentos".to_string());
    let selected_status = match (selected_refresh, selected_state) {
        (Some(remote), _) if remote.last_error.is_some() => "Error en refresh remoto",
        (Some(remote), _) if remote.commits_behind.unwrap_or(0) > 0 => "Desactualizado",
        (_, Some(state)) if repo_has_active_error(state) => "Error en último intento",
        (Some(_), Some(_)) | (Some(_), None) => "Correcto",
        _ => "Sin refresh",
    };
    let selected_error = selected_state
        .and_then(|state| state.last_error.clone())
        .map(|err| truncate_message(&err, 72))
        .unwrap_or_else(|| "-".to_string());
    let selected_result = selected_state
        .and_then(|state| state.last_result.clone())
        .map(|result| truncate_message(&result, 72))
        .unwrap_or_else(|| "-".to_string());
    let selected_branch = selected_refresh
        .and_then(|state| state.branch.clone())
        .or_else(|| selected_state.and_then(|state| state.last_branch.clone()))
        .unwrap_or_else(|| "-".to_string());
    let selected_behind = selected_refresh
        .and_then(|state| state.commits_behind)
        .map(|count| count.to_string())
        .unwrap_or_else(|| "-".to_string());
    let selected_refresh_at = selected_refresh
        .and_then(|state| state.last_refresh_ts)
        .map(|ts| format!("hace {}", humanize_elapsed(now_ts.saturating_sub(ts))))
        .unwrap_or_else(|| "-".to_string());
    let selected_refresh_error = selected_refresh
        .and_then(|state| state.last_error.clone())
        .map(|value| truncate_message(&value, 72))
        .unwrap_or_else(|| "-".to_string());
    let selected_repo_enabled = manager
        .selected_repo()
        .map(|repo| repo.enabled)
        .unwrap_or(true);
    let selected_pulled_commit = selected_state
        .and_then(|state| state.last_pulled_commit.clone())
        .map(|commit| truncate_message(&commit, 72))
        .unwrap_or_else(|| "-".to_string());

    let panel_lines = vec![
        Line::from(vec![Span::styled(
            "Resumen",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )]),
        Line::from(format!("Total: {}", manager.repos.len())),
        Line::from(format!("Pausados: {}", paused_count)),
        Line::from(format!("Desactualizados: {}", outdated_count)),
        Line::from(format!("Con error: {}", error_count)),
        Line::from(""),
        Line::from(vec![Span::styled(
            "Sincronización",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )]),
        Line::from(format!(
            "Refresh auto secuencial: ~{:.1}s/repo",
            manager.refresh_step_interval.as_millis() as f64 / 1000.0
        )),
        Line::from("Acción rápida: u/U"),
        Line::from(format!("Fecha: {}", Local::now().format("%Y-%m-%d"))),
        Line::from(""),
        Line::from(vec![Span::styled(
            "Seleccionado",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )]),
        Line::from(format!("Rama: {}", selected_branch)),
        Line::from(format!(
            "Sync: {}",
            if selected_repo_enabled {
                "Activo"
            } else {
                "Desactivado"
            }
        )),
        Line::from(format!("Commits behind: {}", selected_behind)),
        Line::from(format!("Último refresh: {}", selected_refresh_at)),
        Line::from(format!("Error refresh: {}", selected_refresh_error)),
        Line::from(format!("Estado: {}", selected_status)),
        Line::from(format!("Último resultado: {}", selected_result)),
        Line::from(format!("Último commit pull: {}", selected_pulled_commit)),
        Line::from(format!("Último sync OK: {}", selected_last_sync)),
        Line::from(format!("Último intento: {}", selected_last_attempt)),
        Line::from(format!("Último error: {}", selected_error)),
    ];
    let panel = Paragraph::new(panel_lines)
        .style(Style::default().fg(Color::White))
        .block(Block::default().borders(Borders::ALL).title("Estado"));
    frame.render_widget(panel, body_chunks[1]);

    let details_lines: Vec<Line> = if manager.details_open {
        manager
            .details_lines
            .iter()
            .map(|line| Line::from(line.clone()))
            .collect()
    } else {
        vec![Line::from(
            "Pulse Espacio para ver detalles del repositorio seleccionado",
        )]
    };
    let details = Paragraph::new(details_lines)
        .style(Style::default().fg(Color::White))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Detalles (Espacio)"),
        );
    frame.render_widget(details, chunks[2]);

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
    frame.render_widget(input_block, chunks[3]);

    if matches!(
        manager.input_mode,
        InputMode::AddingSource | InputMode::EditingSource(_)
    ) {
        frame.set_cursor(
            chunks[3].x + manager.input.len() as u16 + 1,
            chunks[3].y + 1,
        );
    }

    let (status_text, status_color) = manager.message.clone().unwrap_or((
        "Listo para editar repositorios".to_string(),
        Color::DarkGray,
    ));
    let status = Paragraph::new(status_text)
        .style(Style::default().fg(status_color))
        .block(Block::default().borders(Borders::ALL).title("Estado"));
    frame.render_widget(status, chunks[4]);

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
            " V ",
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" visitar ruta  "),
        Span::styled(
            " S ",
            Style::default()
                .fg(Color::Black)
                .bg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" activar/pausar  "),
        Span::styled(
            " U/u ",
            Style::default()
                .fg(Color::Black)
                .bg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" sincronizar  "),
        Span::styled(
            " Espacio ",
            Style::default()
                .fg(Color::Black)
                .bg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" detalles  "),
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
    frame.render_widget(shortcuts, chunks[5]);
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

fn wrap_text(message: &str, max_chars: usize, max_lines: usize) -> Vec<String> {
    if max_chars == 0 || max_lines == 0 {
        return Vec::new();
    }

    let mut out = Vec::new();
    let mut current = String::new();

    for word in message.split_whitespace() {
        let projected_len = if current.is_empty() {
            word.chars().count()
        } else {
            current.chars().count() + 1 + word.chars().count()
        };

        if projected_len > max_chars {
            if !current.is_empty() {
                out.push(current);
                current = String::new();
                if out.len() >= max_lines {
                    break;
                }
            }

            if word.chars().count() > max_chars {
                out.push(truncate_message(word, max_chars));
                if out.len() >= max_lines {
                    break;
                }
            } else {
                current.push_str(word);
            }
        } else {
            if !current.is_empty() {
                current.push(' ');
            }
            current.push_str(word);
        }
    }

    if out.len() < max_lines && !current.is_empty() {
        out.push(current);
    }

    if out.len() > max_lines {
        out.truncate(max_lines);
    }

    out
}
