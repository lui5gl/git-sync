mod config;
mod git;
mod logger;
mod processor;
mod service;
mod settings;
mod tui;

use config::Config;
use logger::Logger;
use processor::RepoProcessor;
use service::{install_service, uninstall_service};
use settings::Settings;
use std::env;
use std::thread;
use std::time::Duration;
use tui::run_repo_manager;

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn print_version() {
    println!("git-sync v{}", VERSION);
}

fn print_help() {
    println!("git-sync v{}", VERSION);
    println!("\nGit repository synchronization service.");
    println!("\nUSO:");
    println!("    git-sync             # Abre la interfaz TUI para gestionar repositorios (instala el servicio si falta)");
    println!("    git-sync daemon      # Ejecuta el daemon de sincronización (usado por systemd)");
    println!("    git-sync uninstall-service  # Detiene y elimina el servicio systemd");
    println!("    git-sync --help      # Muestra esta ayuda");
    println!("    git-sync --version   # Muestra la versión actual");
    println!("\nCONFIGURACIÓN:");
    println!("    Config file: /etc/git-sync/config.toml");
    println!("    Repos file:  /etc/git-sync/repositories.txt");
    println!("    Log file:    /var/log/git-sync/git-sync.log");
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
                eprintln!("Failed to uninstall service: {}", err);
                std::process::exit(1);
            }
            return;
        }
        Some(other) => {
            eprintln!("Unknown option: {}", other);
            eprintln!("Use --help to see available commands.");
            std::process::exit(1);
        }
        None => {}
    }

    // Without arguments: ensure service installed and open TUI
    if let Err(err) = install_service() {
        eprintln!("⚠️  Could not install or enable service automatically: {}", err);
        eprintln!("    Ejecuta `sudo git-sync daemon` o instala el servicio manualmente.");
    }

    match config.ensure_exists() {
        Ok(_) => {}
        Err(err) => {
            eprintln!("{}", err);
            std::process::exit(1);
        }
    }

    if let Err(err) = run_repo_manager(&config) {
        eprintln!("Error while running repository manager: {}", err);
        std::process::exit(1);
    }
}

fn run_daemon(config: Config) {
    let repos_created = match config.ensure_exists() {
        Ok(created) => created,
        Err(err) => {
            eprintln!("{}", err);
            std::process::exit(1);
        }
    };

    if repos_created {
        return;
    }

    let mut settings = Settings::load_or_create(&config.settings_file);
    let logger = Logger::new(config.log_file.clone());

    if settings.verbose {
        logger.log_line("=================================================");
        logger.log_line("Git Sync - Repository synchronization daemon");
        logger.log_line("=================================================");
        logger.log_line(&format!("⚙️  Sync interval: {} seconds", settings.sync_interval));
        logger.log_line(&format!("⚙️  Stop on error: {}", settings.stop_on_error));
        logger.log_line(&format!("⚙️  Git timeout: {} seconds", settings.git_timeout));
        logger.log_line(&format!("⚙️  Max retries: {}", settings.max_retries));
        logger.log_line(&format!("⚙️  Continuous mode: {}\n", settings.continuous_mode));
    }

    if !settings.continuous_mode {
        run_sync_cycle(&config, &logger, &settings);
        return;
    }

    loop {
        run_sync_cycle(&config, &logger, &settings);

        if settings.verbose {
            logger.log_line(&format!("\n⏳ Waiting {} seconds before next cycle...\n", settings.sync_interval));
        }

        thread::sleep(Duration::from_secs(settings.sync_interval));
        settings.reload(&config.settings_file);
    }
}

fn run_sync_cycle(config: &Config, logger: &Logger, settings: &Settings) {
    let repos = config.read_repos();
    let processor = RepoProcessor::new(logger, settings.verbose);

    match processor.process_all(repos) {
        Ok(_) => {
            if settings.verbose {
                logger.log_line("\n✅ Cycle completed successfully.");
            }
        }
        Err(e) => {
            logger.log_error(&format!("Error: {}", e));
            if settings.stop_on_error {
                logger.log_error("Exiting due to error (stop_on_error=true)");
                std::process::exit(1);
            }
        }
    }
}
