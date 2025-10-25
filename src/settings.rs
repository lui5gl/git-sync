use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Settings {
    /// Tiempo de espera entre ciclos de sincronización (en segundos)
    pub sync_interval: u64,

    /// Detener el programa si hay algún error
    pub stop_on_error: bool,

    /// Timeout para operaciones git (en segundos)
    pub git_timeout: u64,

    /// Número máximo de reintentos en caso de fallo temporal
    pub max_retries: u32,

    /// Mostrar output detallado
    pub verbose: bool,

    /// Ejecutar en modo continuo (loop infinito)
    pub continuous_mode: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            sync_interval: 60,
            stop_on_error: true,
            git_timeout: 300,
            max_retries: 0,
            verbose: true,
            continuous_mode: true,
        }
    }
}

impl Settings {
    pub fn load_or_create(config_file: &str) -> Self {
        if Path::new(config_file).exists() {
            // Intentar cargar el archivo existente
            match fs::read_to_string(config_file) {
                Ok(contents) => match toml::from_str(&contents) {
                    Ok(settings) => return settings,
                    Err(e) => {
                        eprintln!("⚠️  Error parsing config.toml: {}. Using defaults.", e);
                    }
                },
                Err(e) => {
                    eprintln!("⚠️  Error reading config.toml: {}. Using defaults.", e);
                }
            }
        }

        // Crear archivo con valores por defecto
        let default_settings = Settings::default();
        let toml_string = toml::to_string_pretty(&default_settings)
            .expect("Failed to serialize default settings");

        if let Err(e) = fs::write(config_file, &toml_string) {
            eprintln!("⚠️  Failed to create config.toml: {}", e);
        } else {
            println!("✅ Created config file: {}", config_file);
        }

        default_settings
    }

    pub fn reload(&mut self, config_file: &str) {
        if let Ok(contents) = fs::read_to_string(config_file) {
            if let Ok(new_settings) = toml::from_str(&contents) {
                let was_verbose = self.verbose;
                *self = new_settings;
                if was_verbose && self.verbose {
                    println!("🔄 Configuration reloaded");
                }
            }
        }
    }
}
