mod config;
mod git;
mod logger;
mod processor;
mod service;
mod settings;
mod tui;

use config::{Config, RepoDefinition};
use logger::Logger;
use processor::RepoProcessor;
use service::{install_service, uninstall_service};
use settings::{AppMode, Settings};
use std::env;
use std::thread;
use std::time::Duration;
use tui::run_repo_manager;

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn print_version() {
    println!("ℹ️ git-sync v{}", VERSION);
}

fn print_help() {
    let help = format!(
        r#"
ℹ️ git-sync v{version}

🧭 Servicio de sincronización de repositorios Git.
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

📘 Uso rápido
  • git-sync
      Abre la interfaz interactiva para gestionar repositorios
      (instala el servicio si es necesario).
  • git-sync daemon
      Ejecuta el daemon de sincronización (pensado para systemd).
  • git-sync uninstall-service
      Detiene y elimina el servicio systemd.
  • git-sync update
      Actualiza git-sync a la última versión desde GitHub.
  • git-sync --help
      Muestra esta ayuda.
  • git-sync --version
      Muestra la versión actual.

🗂️ Archivos de configuración
  • Configuración  → /etc/git-sync/config.toml
  • Repositorios   → /etc/git-sync/repositories.txt
  • Registros      → /var/log/git-sync/git-sync.log

🛠️ Recuerde
    • Utilice rutas locales del servidor (no URLs remotas).
    • En modo Development se usa el repo actual y .env/.env.production (GIT_SYNC_DEPLOY_SERVER y GIT_SYNC_DEPLOY_PATH).
    • Proyectos con compilación: fuente en /root/proyects y despliegue en /var/www/html/...
  • Revise los permisos de archivos si ejecuta como otro usuario.
"#,
        version = VERSION
    );

    println!("{}", help.trim_start());
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let config = Config::new();

    match args.get(1).map(|s| s.as_str()) {
        Some("--version") | Some("-v") => {
            print_version();
            return;
        }
        Some("--help") | Some("-h") => {
            print_help();
            return;
        }
        Some("daemon") => {
            run_daemon(config);
            return;
        }
        Some("uninstall-service") => {
            if let Err(err) = uninstall_service() {
                eprintln!("❌ No se pudo desinstalar el servicio: {}", err);
                std::process::exit(1);
            }
            return;
        }
        Some("update") => {
            update_self();
            return;
        }
        Some(other) => {
            eprintln!("⚠️ Opción desconocida: {}", other);
            eprintln!("👉 Utilice --help para consultar los comandos disponibles.");
            std::process::exit(1);
        }
        None => {}
    }

    match config.ensure_exists(true) {
        Ok(_) => {}
        Err(err) => {
            eprintln!("❌ {}", err);
            std::process::exit(1);
        }
    }

    let settings = Settings::load_or_create(&config.settings_file);

    if settings.mode == AppMode::Development {
        if let Err(err) = run_dev_local(&config, &settings, true) {
            eprintln!("❌ {}", err);
            std::process::exit(1);
        }
        return;
    }

    // Sin argumentos: instalar el servicio y abrir la TUI
    if let Err(err) = install_service() {
        eprintln!(
            "⚠️ No fue posible instalar o habilitar el servicio automáticamente: {}",
            err
        );
        eprintln!("👉 Ejecute `sudo git-sync daemon` o complete la instalación de forma manual.");
    }

    if let Err(err) = run_repo_manager(&config, &settings) {
        eprintln!("❌ Error al ejecutar el gestor de repositorios: {}", err);
        std::process::exit(1);
    }
}

fn run_daemon(config: Config) {
    let repos_created = match config.ensure_exists(false) {
        Ok(created) => created,
        Err(err) => {
            eprintln!("❌ {}", err);
            std::process::exit(1);
        }
    };

    if repos_created {
        return;
    }

    let mut settings = Settings::load_or_create(&config.settings_file);
    let logger = Logger::new(config.log_file.clone());

    if settings.mode == AppMode::Development {
        if let Err(err) = run_dev_local(&config, &settings, false) {
            logger.log_error(&err);
            std::process::exit(1);
        }
        return;
    }

    if settings.verbose {
        logger.log_line("=================================================");
        logger.log_line("🚀 Git Sync - Daemon de sincronización de repositorios");
        logger.log_line("=================================================");
        logger.log_line(&format!(
            "🚀 Modo de ejecución: {:?}",
            settings.mode
        ));
        logger.log_line(&format!(
            "⏱️ Intervalo de sincronización: {} segundos",
            settings.sync_interval
        ));
        logger.log_line(&format!(
            "🛑 Detener ante error: {}",
            settings.stop_on_error
        ));
        logger.log_line(&format!(
            "⌛ Tiempo de espera para Git: {} segundos",
            settings.git_timeout
        ));
        logger.log_line(&format!("🔁 Reintentos máximos: {}", settings.max_retries));
        logger.log_line(&format!("♾️ Modo continuo: {}\n", settings.continuous_mode));
    }

    if !settings.continuous_mode {
        run_sync_cycle(&config, &logger, &settings);
        return;
    }

    loop {
        run_sync_cycle(&config, &logger, &settings);

        if settings.verbose {
            logger.log_line(&format!(
                "\n⏳ En espera de {} segundos antes del siguiente ciclo...\n",
                settings.sync_interval
            ));
        }

        thread::sleep(Duration::from_secs(settings.sync_interval));
        settings.reload(&config.settings_file);
    }
}

fn run_sync_cycle(config: &Config, logger: &Logger, settings: &Settings) {
    let repos = config.read_repos();
    let processor = RepoProcessor::new(logger, settings.verbose, settings.mode);

    match processor.process_all(repos) {
        Ok(_) => {
            if settings.verbose {
                logger.log_line("\n✅ Ciclo completado correctamente.");
            }
        }
        Err(e) => {
            logger.log_error(&e.to_string());
            if settings.stop_on_error {
                logger.log_error("🛑 Finalización por error (stop_on_error=true)");
                std::process::exit(1);
            }
        }
    }
}

fn run_dev_local(config: &Config, settings: &Settings, interactive: bool) -> Result<(), String> {
    let logger = Logger::new(config.log_file.clone());
    let repo_path = env::current_dir()
        .map_err(|e| format!("No se pudo obtener el directorio actual: {}", e))?
        .to_string_lossy()
        .to_string();

    if interactive {
        ensure_env_for_repo(&repo_path)?;
    }

    if !settings.continuous_mode {
        run_dev_cycle(&logger, settings, &repo_path);
        return Ok(());
    }

    loop {
        run_dev_cycle(&logger, settings, &repo_path);

        if settings.verbose {
            logger.log_line(&format!(
                "\n⏳ En espera de {} segundos antes del siguiente ciclo...\n",
                settings.sync_interval
            ));
        }

        thread::sleep(Duration::from_secs(settings.sync_interval));
    }
}

fn run_dev_cycle(logger: &Logger, settings: &Settings, repo_path: &str) {
    let repos = vec![RepoDefinition::new(repo_path, Option::<String>::None)];
    let processor = RepoProcessor::new(logger, settings.verbose, settings.mode);

    if let Err(e) = processor.process_all(repos) {
        logger.log_error(&e);
        if settings.stop_on_error {
            logger.log_error("🛑 Finalización por error (stop_on_error=true)");
            std::process::exit(1);
        }
    }
}

fn ensure_env_for_repo(repo_path: &str) -> Result<(), String> {
    use std::io::{self, Write};
    use std::path::Path;

    let repo_dir = Path::new(repo_path);
    if !repo_dir.is_dir() {
        return Err("El directorio actual no es valido".to_string());
    }

    let env_production = repo_dir.join(".env.production");
    let env_default = repo_dir.join(".env");
    let env_path = if env_production.exists() {
        env_production
    } else {
        env_default
    };

    let existing = if env_path.exists() {
        std::fs::read_to_string(&env_path).unwrap_or_default()
    } else {
        String::new()
    };

    let mut server: Option<String> = None;
    let mut path: Option<String> = None;

    for line in existing.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = trimmed.split_once('=') {
            let key = key.trim();
            let value = value
                .trim()
                .trim_matches(|c| c == '"' || c == '\'');
            match key {
                "GIT_SYNC_DEPLOY_SERVER" => server = Some(value.to_string()),
                "GIT_SYNC_DEPLOY_PATH" => path = Some(value.to_string()),
                _ => {}
            }
        }
    }

    if server.as_deref().unwrap_or("").is_empty() {
        print!("🌐 GIT_SYNC_DEPLOY_SERVER (usuario@host): ");
        io::stdout().flush().map_err(|e| e.to_string())?;
        let mut input = String::new();
        io::stdin().read_line(&mut input).map_err(|e| e.to_string())?;
        let input = input.trim().to_string();
        if input.is_empty() || !input.contains('@') {
            return Err("GIT_SYNC_DEPLOY_SERVER invalido".to_string());
        }
        server = Some(input);
    }

    if path.as_deref().unwrap_or("").is_empty() {
        print!("📦 GIT_SYNC_DEPLOY_PATH (ej: /var/www/html/app): ");
        io::stdout().flush().map_err(|e| e.to_string())?;
        let mut input = String::new();
        io::stdin().read_line(&mut input).map_err(|e| e.to_string())?;
        let input = input.trim().to_string();
        if input.is_empty() {
            return Err("GIT_SYNC_DEPLOY_PATH no puede estar vacio".to_string());
        }
        path = Some(input);
    }

    let mut lines: Vec<String> = existing.lines().map(|l| l.to_string()).collect();
    lines.retain(|l| {
        !l.starts_with("GIT_SYNC_DEPLOY_SERVER=") && !l.starts_with("GIT_SYNC_DEPLOY_PATH=")
    });

    if let Some(value) = server {
        lines.push(format!("GIT_SYNC_DEPLOY_SERVER={}", value));
    }
    if let Some(value) = path {
        lines.push(format!("GIT_SYNC_DEPLOY_PATH={}", value));
    }

    std::fs::write(&env_path, lines.join("\n"))
        .map_err(|e| format!("No se pudo escribir en {}: {}", env_path.display(), e))?;

    Ok(())
}

fn update_self() {
    println!("🔄 Buscando actualizaciones para git-sync...");

    // 1. Detectar el sistema operativo
    let os = std::env::consts::OS;
    if os != "linux" {
        println!("❌ El comando de actualización automática solo está disponible para Linux.");
        return;
    }

    // 2. Ejecutar el script de instalación/actualización oficial
    // Asumimos que el usuario tiene acceso a internet y el script está disponible
    let status = std::process::Command::new("sh")
        .arg("-c")
        .arg("curl -fsSL https://raw.githubusercontent.com/lui5gl/git-sync/main/install.sh | bash")
        .status();

    match status {
        Ok(s) if s.success() => {
            println!("\n✅ ¡git-sync ha sido actualizado correctamente!");
            println!("👉 Reinicie el servicio si es necesario: `sudo systemctl restart git-sync`.");
        }
        Ok(s) => {
            println!("\n❌ Error al actualizar: el script finalizó con estado {}.", s);
        }
        Err(e) => {
            println!("\n❌ Error al ejecutar el comando de actualización: {}.", e);
            println!("💡 Asegúrese de tener `curl` instalado.");
        }
    }
}
