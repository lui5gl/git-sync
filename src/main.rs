mod config;
mod git;
mod logger;
mod processor;
mod service;
mod settings;

use config::Config;
use logger::Logger;
use processor::RepoProcessor;
use service::{install_service, uninstall_service};
use settings::Settings;
use std::env;
use std::thread;
use std::time::Duration;

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn print_version() {
    println!("git-sync v{}", VERSION);
}

fn print_help() {
    println!("git-sync v{}", VERSION);
    println!("\nA daemon to automatically synchronize multiple Git repositories.");
    println!("\nUSAGE:");
    println!("    git-sync [OPTIONS]");
    println!("\nOPTIONS:");
    println!("    -h, --help       Print help information");
    println!("    -v, --version    Print version information");
    println!("    -q, --quiet      Run in quiet mode (minimal output, overrides config)");
    println!("        --install-service  Create and enable the git-sync systemd service");
    println!("        --uninstall-service  Remove the git-sync systemd service");
    println!("\nCONFIGURATION:");
    println!("    Config file: ~/.config/git-sync/config.toml");
    println!("    Repos file:  ~/.config/git-sync/repositories.txt");
    println!("    Log file:    ~/.config/git-sync/.log");
    println!("\nFor more information, visit: https://github.com/lui5gl/git-sync");
}

fn main() {
    // Parse command line arguments
    let args: Vec<String> = env::args().collect();
    let mut quiet_mode = false;

    #[derive(PartialEq)]
    enum Action {
        Run,
        InstallService,
        UninstallService,
    }

    let mut action = Action::Run;

    for arg in args.iter().skip(1) {
        match arg.as_str() {
            "-v" | "--version" => {
                print_version();
                return;
            }
            "-h" | "--help" => {
                print_help();
                return;
            }
            "-q" | "--quiet" => {
                quiet_mode = true;
            }
            "--install-service" => {
                if action != Action::Run {
                    eprintln!("Multiple actions specified. Please choose only one.");
                    std::process::exit(1);
                }
                action = Action::InstallService;
            }
            "--uninstall-service" => {
                if action != Action::Run {
                    eprintln!("Multiple actions specified. Please choose only one.");
                    std::process::exit(1);
                }
                action = Action::UninstallService;
            }
            other => {
                eprintln!("Unknown option: {}", other);
                eprintln!("Use --help for usage information");
                std::process::exit(1);
            }
        }
    }

    match action {
        Action::InstallService => {
            if let Err(err) = install_service() {
                eprintln!("❌ Failed to install service: {}", err);
                std::process::exit(1);
            }
            return;
        }
        Action::UninstallService => {
            if let Err(err) = uninstall_service() {
                eprintln!("❌ Failed to uninstall service: {}", err);
                std::process::exit(1);
            }
            return;
        }
        Action::Run => {}
    }

    let config = Config::new();
    let repos_created = config.ensure_exists();

    // Exit early if repos file was just created
    if repos_created {
        return;
    }

    // Load or create settings
    let mut settings = Settings::load_or_create(&config.settings_file);

    // Override verbose setting if quiet mode is enabled
    if quiet_mode {
        settings.verbose = false;
    }

    let logger = Logger::new(config.log_file.clone());

    if settings.verbose {
        logger.log_line("=================================================");
        logger.log_line("Git Sync - Repository synchronization daemon");
        logger.log_line("=================================================");
        logger.log_line(&format!(
            "⚙️  Sync interval: {} seconds",
            settings.sync_interval
        ));
        logger.log_line(&format!("⚙️  Stop on error: {}", settings.stop_on_error));
        logger.log_line(&format!(
            "⚙️  Git timeout: {} seconds",
            settings.git_timeout
        ));
        logger.log_line(&format!("⚙️  Max retries: {}", settings.max_retries));
        logger.log_line(&format!(
            "⚙️  Continuous mode: {}\n",
            settings.continuous_mode
        ));
    }

    if !settings.continuous_mode {
        // Ejecutar una sola vez
        run_sync_cycle(&config, &logger, &settings);
        return;
    }

    // Loop continuo
    loop {
        run_sync_cycle(&config, &logger, &settings);

        if settings.verbose {
            logger.log_line(&format!(
                "\n⏳ Waiting {} seconds before next cycle...\n",
                settings.sync_interval
            ));
        }
        thread::sleep(Duration::from_secs(settings.sync_interval));

        // Recargar configuración en cada ciclo (permite cambios en caliente)
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
