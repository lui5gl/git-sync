use crate::settings::Settings;
use std::fs::{self, File};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

pub struct Config {
    pub config_dir: String,
    pub repos_file: String,
    pub settings_file: String,
    pub log_dir: String,
    pub log_file: String,
}

impl Config {
    pub fn new() -> Self {
        let config_dir = "/etc/git-sync".to_string();
        let repos_file = format!("{}/repositories.txt", config_dir);
        let settings_file = format!("{}/config.toml", config_dir);
        let log_dir = "/var/log/git-sync".to_string();
        let log_file = format!("{}/git-sync.log", log_dir);

        Config {
            config_dir,
            repos_file,
            settings_file,
            log_dir,
            log_file,
        }
    }

    pub fn ensure_exists(&self) -> Result<bool, String> {
        self.ensure_directory(&self.config_dir, 0o755)?;
        self.ensure_directory(&self.log_dir, 0o755)?;

        let repos_created = self.ensure_repos_file()?;
        self.ensure_settings_file()?;
        self.ensure_log_file()?;

        if repos_created {
            println!(
                "\n📝 Please add repository paths to {} and restart the service.\n",
                self.repos_file
            );
        }

        Ok(repos_created)
    }

    fn ensure_directory(&self, path: &str, mode: u32) -> Result<(), String> {
        if !Path::new(path).exists() {
            fs::create_dir_all(path)
                .map_err(|e| format!("Failed to create directory {}: {}", path, e))?;
            let permissions = fs::Permissions::from_mode(mode);
            fs::set_permissions(path, permissions)
                .map_err(|e| format!("Failed to set permissions on {}: {}", path, e))?;
            println!("✅ Created directory: {}", path);
        }
        Ok(())
    }

    fn ensure_repos_file(&self) -> Result<bool, String> {
        if !Path::new(&self.repos_file).exists() {
            let default_content = "# Add absolute paths to your git repositories, one per line\n\
                                   # Example:\n\
                                   # /home/git/repos/my-repo\n";
            fs::write(&self.repos_file, default_content)
                .map_err(|e| format!("Failed to create repositories file {}: {}", self.repos_file, e))?;
            let permissions = fs::Permissions::from_mode(0o644);
            fs::set_permissions(&self.repos_file, permissions)
                .map_err(|e| format!("Failed to set permissions on {}: {}", self.repos_file, e))?;
            println!("✅ Created repositories file: {}", self.repos_file);
            return Ok(true);
        }

        Ok(false)
    }

    fn ensure_settings_file(&self) -> Result<(), String> {
        if !Path::new(&self.settings_file).exists() {
            let default_settings = Settings::default();
            let toml_string = toml::to_string_pretty(&default_settings)
                .map_err(|e| format!("Failed to serialize default settings: {}", e))?;
            fs::write(&self.settings_file, toml_string)
                .map_err(|e| format!("Failed to create config file {}: {}", self.settings_file, e))?;
            let permissions = fs::Permissions::from_mode(0o644);
            fs::set_permissions(&self.settings_file, permissions)
                .map_err(|e| format!("Failed to set permissions on {}: {}", self.settings_file, e))?;
            println!("✅ Created config file: {}", self.settings_file);
        }

        Ok(())
    }

    fn ensure_log_file(&self) -> Result<(), String> {
        if !Path::new(&self.log_file).exists() {
            File::create(&self.log_file)
                .map_err(|e| format!("Failed to create log file {}: {}", self.log_file, e))?;
            let permissions = fs::Permissions::from_mode(0o644);
            fs::set_permissions(&self.log_file, permissions)
                .map_err(|e| format!("Failed to set permissions on {}: {}", self.log_file, e))?;
            println!("✅ Created log file: {}", self.log_file);
        }

        Ok(())
    }

    pub fn read_repos(&self) -> Vec<String> {
        let contents = fs::read_to_string(&self.repos_file)
            .unwrap_or_else(|e| panic!("Failed to read repositories file {}: {}", self.repos_file, e));

        contents
            .lines()
            .map(|line| line.trim())
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
            .map(|line| line.to_string())
            .collect()
    }

    pub fn write_repos(&self, repos: &[String]) -> Result<(), String> {
        let mut content = String::from("# Repository list managed by git-sync\n");
        content.push_str("# One absolute path per line\n");
        if !repos.is_empty() {
            for repo in repos {
                content.push_str(repo.trim());
                content.push('\n');
            }
        }

        fs::write(&self.repos_file, content)
            .map_err(|e| format!("Failed to write repositories file {}: {}", self.repos_file, e))?;

        let permissions = fs::Permissions::from_mode(0o644);
        fs::set_permissions(&self.repos_file, permissions)
            .map_err(|e| format!("Failed to set permissions on {}: {}", self.repos_file, e))?;

        Ok(())
    }
}
