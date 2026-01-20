use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
pub enum AppMode {
    #[serde(rename = "production")]
    Production,
    #[serde(rename = "development")]
    Development,
}

impl Default for AppMode {
    fn default() -> Self {
        AppMode::Production
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Settings {
    /// Modo de aplicación: production (solo pull) o development (solo push/transfer)
    pub mode: AppMode,

    /// IP o Hostname del servidor remoto (solo para modo Development)
    pub remote_host: Option<String>,

    /// Usuario SSH para el servidor remoto (solo para modo Development)
    pub remote_user: Option<String>,

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
            mode: AppMode::Production,
            remote_host: None,
            remote_user: None,
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
                        eprintln!(
                            "⚠️ Error al interpretar config.toml: {}. Se utilizarán los valores predeterminados.",
                            e
                        );
                    }
                },
                Err(e) => {
                    eprintln!(
                        "⚠️ Error al leer config.toml: {}. Se utilizarán los valores predeterminados.",
                        e
                    );
                }
            }
        }

        // Si no existe, iniciamos el modo interactivo
        let (mode, remote_host, remote_user) = Self::interactive_init();

        // Crear archivo con los valores del modo seleccionado
        let mut default_settings = Settings::default();
        default_settings.mode = mode;
        default_settings.remote_host = remote_host;
        default_settings.remote_user = remote_user;

        let toml_string = toml::to_string_pretty(&default_settings)
            .expect("❌ No se pudo serializar la configuración");

        if let Err(e) = fs::write(config_file, &toml_string) {
            eprintln!("❌ No se pudo crear config.toml: {}", e);
        } else {
            println!("⚙️ Archivo de configuración creado: {}", config_file);
        }

        default_settings
    }

    pub fn reload(&mut self, config_file: &str) {
        if let Ok(contents) = fs::read_to_string(config_file) {
            if let Ok(new_settings) = toml::from_str(&contents) {
                let was_verbose = self.verbose;
                *self = new_settings;
                if was_verbose && self.verbose {
                    println!("🔄 Configuración recargada");
                }
            }
        }
    }

    pub fn interactive_init() -> (AppMode, Option<String>, Option<String>) {
        use std::io::{self, Write};

        println!("\n🚀 Bienvenido a git-sync!");
        println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        println!("Parece que es la primera vez que inicia la aplicación.");
        println!("Por favor, seleccione el modo de funcionamiento:");
        println!("\n1) 🚀 Producción (Servidor)");
        println!("   • Solo descarga cambios del remoto (git pull).");
        println!("   • Útil para servidores donde se despliega el código.");
        println!("\n2) 💻 Desarrollo (Local)");
        println!("   • Compila el proyecto localmente y sube los artefactos al servidor.");
        println!("   • Útil para su equipo de trabajo local.");
        println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

        loop {
            print!("\nSeleccione una opción (1 o 2): ");
            io::stdout().flush().unwrap();

            let mut input = String::new();
            io::stdin().read_line(&mut input).unwrap();

            match input.trim() {
                "1" => {
                    println!("✅ Modo Producción seleccionado.");
                    return (AppMode::Production, None, None);
                }
                "2" => {
                    println!("✅ Modo Desarrollo seleccionado.");
                    
                    print!("🌐 Ingrese la IP o Hostname del servidor: ");
                    io::stdout().flush().unwrap();
                    let mut host = String::new();
                    io::stdin().read_line(&mut host).unwrap();
                    let host = host.trim().to_string();

                    print!("👤 Ingrese el usuario SSH (ej: root): ");
                    io::stdout().flush().unwrap();
                    let mut user = String::new();
                    io::stdin().read_line(&mut user).unwrap();
                    let user = user.trim().to_string();

                    return (AppMode::Development, Some(host), Some(user));
                }
                _ => {
                    println!("⚠️ Opción no válida. Por favor, ingrese 1 o 2.");
                }
            }
        }
    }
}
