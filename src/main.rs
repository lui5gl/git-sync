mod config;
mod git;
mod logger;
mod processor;
mod settings;

use config::Config;
use logger::Logger;
use processor::RepoProcessor;
use settings::Settings;
use std::thread;
use std::time::Duration;

fn main() {
    let config = Config::new();
    let repos_created = config.ensure_exists();

    // Exit early if repos file was just created
    if repos_created {
        return;
    }

    // Load or create settings
    let mut settings = Settings::load_or_create(&config.settings_file);
    let logger = Logger::new(config.log_file.clone());
    
    logger.log_line("=================================================");
    logger.log_line("Git Sync - Repository synchronization daemon");
    logger.log_line("=================================================");
    logger.log_line(&format!("⚙️  Sync interval: {} seconds", settings.sync_interval));
    logger.log_line(&format!("⚙️  Stop on error: {}", settings.stop_on_error));
    logger.log_line(&format!("⚙️  Git timeout: {} seconds", settings.git_timeout));
    logger.log_line(&format!("⚙️  Max retries: {}", settings.max_retries));
    logger.log_line(&format!("⚙️  Continuous mode: {}\n", settings.continuous_mode));

    if !settings.continuous_mode {
        // Ejecutar una sola vez
        run_sync_cycle(&config, &logger, &settings);
        return;
    }

    // Loop continuo
    loop {
        run_sync_cycle(&config, &logger, &settings);
        
        logger.log_line(&format!("\n⏳ Waiting {} seconds before next cycle...\n", settings.sync_interval));
        thread::sleep(Duration::from_secs(settings.sync_interval));
        
        // Recargar configuración en cada ciclo (permite cambios en caliente)
        settings.reload(&config.settings_file);
    }
}

fn run_sync_cycle(config: &Config, logger: &Logger, settings: &Settings) {
    let repos = config.read_repos();
    let processor = RepoProcessor::new(logger);
    
    match processor.process_all(repos) {
        Ok(_) => {
            logger.log_line("\n✅ Cycle completed successfully.");
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
