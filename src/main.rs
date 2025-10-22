mod config;
mod git;
mod processor;

use config::Config;
use processor::RepoProcessor;
use std::path::Path;

fn main() {
    let config = Config::new();
    config.ensure_exists();

    // Exit early if config file was just created
    if !Path::new(&config.config_file).exists() {
        return;
    }

    let repos = config.read_repos();
    RepoProcessor::process_all(repos);
}
