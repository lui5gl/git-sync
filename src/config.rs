use std::env;
use std::fs;
use std::path::Path;

pub struct Config {
    pub config_dir: String,
    pub repos_file: String,
    pub settings_file: String,
    pub log_file: String,
}

impl Config {
    pub fn new() -> Self {
        let home_dir = env::var("HOME").expect("Could not get HOME directory");
        let config_dir = format!("{}/.gitlab-cd-ci", home_dir);
        let repos_file = format!("{}/repos.txt", config_dir);
        let settings_file = format!("{}/config.toml", config_dir);
        let log_file = format!("{}/.log", config_dir);

        Config {
            config_dir,
            repos_file,
            settings_file,
            log_file,
        }
    }

    pub fn ensure_exists(&self) -> bool {
        // Create config directory if it doesn't exist
        if !Path::new(&self.config_dir).exists() {
            fs::create_dir_all(&self.config_dir).expect("Failed to create config directory");
            println!("✅ Created config directory: {}", self.config_dir);
        }

        // Create repos file if it doesn't exist
        let repos_created = if !Path::new(&self.repos_file).exists() {
            let default_content = "# Add absolute paths to your git repositories, one per line\n\
                                   # Example:\n\
                                   # /home/user/projects/my-repo\n";
            fs::write(&self.repos_file, default_content)
                .expect("Failed to create repos file");
            println!("✅ Created repos file: {}", self.repos_file);
            true
        } else {
            false
        };

        if repos_created {
            println!("\n📝 Please add repository paths to {} and run again.\n", self.repos_file);
        }

        repos_created
    }

    pub fn read_repos(&self) -> Vec<String> {
        let contents = fs::read_to_string(&self.repos_file)
            .expect("Failed to read repos file");

        contents
            .lines()
            .map(|line| line.trim())
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
            .map(|line| line.to_string())
            .collect()
    }
}
