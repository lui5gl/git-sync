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
    let processor = RepoProcessor::new(
        logger,
        settings.verbose,
        settings.mode,
        settings.remote_host.clone(),
        settings.remote_user.clone(),
    );

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
