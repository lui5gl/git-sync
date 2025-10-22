use std::env;
use std::fs;
use std::path::Path;

pub struct Config {
    pub config_dir: String,
    pub config_file: String,
    pub log_file: String,
}

impl Config {
    pub fn new() -> Self {
        let home_dir = env::var("HOME").expect("Could not get HOME directory");
        let config_dir = format!("{}/.gitlab-cd-ci", home_dir);
        let config_file = format!("{}/repos.txt", config_dir);
        let log_file = format!("{}/.log", config_dir);

        Config {
            config_dir,
            config_file,
            log_file,
        }
    }

    pub fn ensure_exists(&self) {
        // Create config directory if it doesn't exist
        if !Path::new(&self.config_dir).exists() {
            fs::create_dir_all(&self.config_dir).expect("Failed to create config directory");
            println!("Created config directory: {}", self.config_dir);
        }

        // Create config file if it doesn't exist
        if !Path::new(&self.config_file).exists() {
            let default_content = "# Add absolute paths to your git repositories, one per line\n\
                                   # Example:\n\
                                   # /home/user/projects/my-repo\n";
            fs::write(&self.config_file, default_content)
                .expect("Failed to create config file");
            println!("Created config file: {}", self.config_file);
            println!("Please add repository paths to this file and run again.");
        }
    }

    pub fn read_repos(&self) -> Vec<String> {
        let contents = fs::read_to_string(&self.config_file)
            .expect("Failed to read config file");

        contents
            .lines()
            .map(|line| line.trim())
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
            .map(|line| line.to_string())
            .collect()
    }
}
