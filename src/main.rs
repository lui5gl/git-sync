mod config;
mod git;
mod logger;
mod processor;

use config::Config;
use logger::Logger;
use processor::RepoProcessor;
use std::path::Path;
use std::thread;
use std::time::Duration;

fn main() {
    let config = Config::new();
    config.ensure_exists();

    // Exit early if config file was just created
    if !Path::new(&config.config_file).exists() {
        return;
    }

    let logger = Logger::new(config.log_file.clone());
    
    logger.log_line("=================================================");
    logger.log_line("GitLab CD/CI - Starting repository sync daemon");
    logger.log_line("=================================================\n");

    loop {
        let repos = config.read_repos();
        let processor = RepoProcessor::new(&logger);
        
        match processor.process_all(repos) {
            Ok(_) => {
                logger.log_line("\n✅ Cycle completed successfully. Waiting 60 seconds...\n");
                thread::sleep(Duration::from_secs(60));
            }
            Err(e) => {
                logger.log_error(&format!("Critical error: {}. Exiting...", e));
                std::process::exit(1);
            }
        }
    }
}
