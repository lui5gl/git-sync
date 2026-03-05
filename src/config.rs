use crate::settings::Settings;
use crate::sync_state::SyncStateSnapshot;
use std::fs::{self, File};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RepoDefinition {
    pub repo_path: String,
    pub enabled: bool,
}

impl RepoDefinition {
    pub fn new<P: Into<String>>(repo_path: P) -> Self {
        let repo_path = repo_path.into();
        RepoDefinition {
            repo_path,
            enabled: true,
        }
    }

    pub fn from_line(line: &str) -> Option<Self> {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            return None;
        }

        let (enabled, raw_path) = if let Some(rest) = trimmed.strip_prefix('!') {
            (false, rest.trim())
        } else {
            (true, trimmed)
        };

        if let Some((source, target)) = raw_path.split_once("=>") {
            let source = source.trim();
            if source.is_empty() {
                return None;
            }
            if !target.trim().is_empty() {
                eprintln!(
                    "⚠️ Se ignoró el destino de despliegue en {}. El modo compilación fue eliminado.",
                    source
                );
            }
            let mut repo = RepoDefinition::new(source.to_string());
            repo.enabled = enabled;
            Some(repo)
        } else {
            let mut repo = RepoDefinition::new(raw_path.to_string());
            repo.enabled = enabled;
            Some(repo)
        }
    }

    pub fn to_line(&self) -> String {
        if self.enabled {
            self.repo_path.trim().to_string()
        } else {
            format!("! {}", self.repo_path.trim())
        }
    }
}

pub struct Config {
    pub config_dir: String,
    pub repos_file: String,
    pub settings_file: String,
    pub log_dir: String,
    pub log_file: String,
    pub state_file: String,
}

impl Config {
    pub fn new() -> Self {
        let (config_dir, log_dir) = ("/etc/git-sync".to_string(), "/var/log/git-sync".to_string());

        let repos_file = format!("{}/repositories.txt", config_dir);
        let settings_file = format!("{}/config.toml", config_dir);
        let log_file = format!("{}/git-sync.log", log_dir);
        let state_file = format!("{}/state.toml", log_dir);

        Config {
            config_dir,
            repos_file,
            settings_file,
            log_dir,
            log_file,
            state_file,
        }
    }

    pub fn ensure_exists(&self) -> Result<bool, String> {
        self.ensure_directory(&self.config_dir, 0o755)?;
        self.ensure_directory(&self.log_dir, 0o755)?;

        let repos_created = self.ensure_repos_file()?;
        self.ensure_settings_file()?;
        self.ensure_log_file()?;
        self.ensure_state_file()?;

        if repos_created {
            println!(
                "\n📌 Agregue las rutas de los repositorios en {} y reinicie el servicio.\n",
                self.repos_file
            );
        }

        Ok(repos_created)
    }

    fn ensure_directory(&self, path: &str, _mode: u32) -> Result<(), String> {
        if !Path::new(path).exists() {
            fs::create_dir_all(path)
                .map_err(|e| format!("❌ No se pudo crear el directorio {}: {}", path, e))?;

            let permissions = fs::Permissions::from_mode(_mode);
            fs::set_permissions(path, permissions)
                .map_err(|e| format!("❌ No se pudieron asignar permisos a {}: {}", path, e))?;

            println!("📁 Directorio creado: {}", path);
        }
        Ok(())
    }

    fn ensure_repos_file(&self) -> Result<bool, String> {
        if !Path::new(&self.repos_file).exists() {
            let default_content = "# Añada rutas absolutas de repositorios Git, una por línea\n\
                                    # Use rutas locales del servidor (no URLs de GitHub/GitLab)\n\
                                    # Para desactivar un repo temporalmente, use prefijo !\n\
                                    # Ejemplo:\n\
                                    # /var/www/html/mi-app\n";
            fs::write(&self.repos_file, default_content).map_err(|e| {
                format!(
                    "❌ No se pudo crear el archivo de repositorios {}: {}",
                    self.repos_file, e
                )
            })?;

            let permissions = fs::Permissions::from_mode(0o644);
            fs::set_permissions(&self.repos_file, permissions).map_err(|e| {
                format!(
                    "❌ No se pudieron asignar permisos a {}: {}",
                    self.repos_file, e
                )
            })?;

            println!("🗂️ Archivo de repositorios creado: {}", self.repos_file);
            return Ok(true);
        }

        Ok(false)
    }

    fn ensure_settings_file(&self) -> Result<(), String> {
        if !Path::new(&self.settings_file).exists() {
            let default_settings = Settings::default();

            let toml_string = toml::to_string_pretty(&default_settings).map_err(|e| {
                format!(
                    "❌ No se pudo serializar la configuración predeterminada: {}",
                    e
                )
            })?;
            fs::write(&self.settings_file, toml_string).map_err(|e| {
                format!(
                    "❌ No se pudo crear el archivo de configuración {}: {}",
                    self.settings_file, e
                )
            })?;

            let permissions = fs::Permissions::from_mode(0o644);
            fs::set_permissions(&self.settings_file, permissions).map_err(|e| {
                format!(
                    "❌ No se pudieron asignar permisos a {}: {}",
                    self.settings_file, e
                )
            })?;

            println!("⚙️ Archivo de configuración creado: {}", self.settings_file);
        }

        Ok(())
    }

    fn ensure_log_file(&self) -> Result<(), String> {
        if !Path::new(&self.log_file).exists() {
            File::create(&self.log_file).map_err(|e| {
                format!(
                    "❌ No se pudo crear el archivo de registro {}: {}",
                    self.log_file, e
                )
            })?;

            let permissions = fs::Permissions::from_mode(0o644);
            fs::set_permissions(&self.log_file, permissions).map_err(|e| {
                format!(
                    "❌ No se pudieron asignar permisos a {}: {}",
                    self.log_file, e
                )
            })?;

            println!("📝 Archivo de registro creado: {}", self.log_file);
        }

        Ok(())
    }

    fn ensure_state_file(&self) -> Result<(), String> {
        if !Path::new(&self.state_file).exists() {
            let default_state = SyncStateSnapshot::default();
            let content = toml::to_string_pretty(&default_state).map_err(|e| {
                format!(
                    "❌ No se pudo serializar el estado inicial de sincronización: {}",
                    e
                )
            })?;

            fs::write(&self.state_file, content).map_err(|e| {
                format!(
                    "❌ No se pudo crear el archivo de estado {}: {}",
                    self.state_file, e
                )
            })?;

            let permissions = fs::Permissions::from_mode(0o644);
            fs::set_permissions(&self.state_file, permissions).map_err(|e| {
                format!(
                    "❌ No se pudieron asignar permisos a {}: {}",
                    self.state_file, e
                )
            })?;

            println!("📊 Archivo de estado creado: {}", self.state_file);
        }

        Ok(())
    }

    pub fn read_repos(&self) -> Vec<RepoDefinition> {
        let contents = fs::read_to_string(&self.repos_file).unwrap_or_else(|e| {
            panic!(
                "❌ No se pudo leer el archivo de repositorios {}: {}",
                self.repos_file, e
            )
        });

        contents
            .lines()
            .filter_map(RepoDefinition::from_line)
            .collect()
    }

    pub fn write_repos(&self, repos: &[RepoDefinition]) -> Result<(), String> {
        let mut content = String::from("# Lista de repositorios administrada por git-sync\n");
        content.push_str("# Especifique una ruta absoluta por línea (ruta local, no URL remota)\n");
        content.push_str("# Para desactivar un repo temporalmente use: ! /ruta/al/repo\n");
        content.push_str("# Ejemplo:\n");
        content.push_str("#   /var/www/html/mi-app\n");
        for repo in repos {
            content.push_str(&repo.to_line());
            content.push('\n');
        }

        fs::write(&self.repos_file, content).map_err(|e| {
            format!(
                "❌ No se pudo escribir en el archivo de repositorios {}: {}",
                self.repos_file, e
            )
        })?;

        let permissions = fs::Permissions::from_mode(0o644);
        fs::set_permissions(&self.repos_file, permissions).map_err(|e| {
            format!(
                "❌ No se pudieron asignar permisos a {}: {}",
                self.repos_file, e
            )
        })?;

        Ok(())
    }
}
