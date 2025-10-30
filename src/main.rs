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
    println!("ℹ️ git-sync v{}", VERSION);
}

fn print_help() {
    println!("ℹ️ git-sync v{}", VERSION);
    println!("\n🧭 Servicio de sincronización de repositorios Git.");
    println!("\n📘 USO:");
    println!(
        "    git-sync             # 🖥️ Abre la interfaz interactiva para gestionar repositorios (instala el servicio si es necesario)"
    );
    println!(
        "    git-sync daemon      # 🔁 Ejecuta el daemon de sincronización (utilizado por systemd)"
    );
    println!("    git-sync uninstall-service  # 🧹 Detiene y elimina el servicio systemd");
    println!("    git-sync --help      # ❓ Muestra esta ayuda");
    println!("    git-sync --version   # 🔖 Muestra la versión actual");
    println!("\n🗂️ ARCHIVOS DE CONFIGURACIÓN:");
    println!("    Configuración: 📄 /etc/git-sync/config.toml");
    println!("    Repositorios:  📂 /etc/git-sync/repositories.txt");
    println!("    Registros:     📝 /var/log/git-sync/git-sync.log");
    println!("    ➤ Las rutas deben ser locales en el servidor (no URLs remotas).");
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
        Some(other) => {
            eprintln!("⚠️ Opción desconocida: {}", other);
            eprintln!("👉 Utilice --help para consultar los comandos disponibles.");
            std::process::exit(1);
        }
        None => {}
    }

    // Sin argumentos: instalar el servicio y abrir la TUI
    if let Err(err) = install_service() {
        eprintln!(
            "⚠️ No fue posible instalar o habilitar el servicio automáticamente: {}",
            err
        );
        eprintln!("👉 Ejecute `sudo git-sync daemon` o complete la instalación de forma manual.");
    }

    match config.ensure_exists() {
        Ok(_) => {}
        Err(err) => {
            eprintln!("❌ {}", err);
            std::process::exit(1);
        }
    }

    if let Err(err) = run_repo_manager(&config) {
        eprintln!("❌ Error al ejecutar el gestor de repositorios: {}", err);
        std::process::exit(1);
    }
}

fn run_daemon(config: Config) {
    let repos_created = match config.ensure_exists() {
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

    if settings.verbose {
        logger.log_line("=================================================");
        logger.log_line("🚀 Git Sync - Daemon de sincronización de repositorios");
        logger.log_line("=================================================");
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
    let processor = RepoProcessor::new(logger, settings.verbose);

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
