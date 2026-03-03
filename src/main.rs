mod config;
mod git;
mod logger;
mod processor;
mod service;
mod settings;
mod tui;

use config::Config;
use logger::Logger;
use processor::RepoProcessor;
use service::{install_service, uninstall_service};
use settings::Settings;
use std::env;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::Duration;
use tui::run_repo_manager;

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn print_version() {
    println!("ℹ️ git-sync v{}", VERSION);
}

fn print_help() {
    let help = format!(
        r#"
ℹ️ git-sync v{version}

🧭 Servicio de sincronización de repositorios Git.
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

📘 Uso rápido
  • git-sync
      Abre la interfaz interactiva para gestionar repositorios
      (instala el servicio si es necesario).
  • git-sync daemon
      Ejecuta el daemon de sincronización (pensado para systemd).
  • git-sync uninstall-service
      Detiene y elimina el servicio systemd.
  • git-sync update
      Actualiza a la última versión estable.
  • git-sync --help
      Muestra esta ayuda.
  • git-sync --version
      Muestra la versión actual.

🗂️ Archivos de configuración
  • Configuración  → /etc/git-sync/config.toml
  • Repositorios   → /etc/git-sync/repositories.txt
  • Registros      → /var/log/git-sync/git-sync.log

🛠️ Recuerde
    • Utilice rutas locales del servidor (no URLs remotas).
    • Proyectos con compilación: fuente en /root/proyects y despliegue en /var/www/html/...
  • Revise los permisos de archivos si ejecuta como otro usuario.
"#,
        version = VERSION
    );

    println!("{}", help.trim_start());
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let config = Config::new();

    match args.get(1).map(|s| s.as_str()) {
        Some("--version") | Some("-v") => {
            print_version();
            return;
        }
        Some("--help") | Some("-h") => {
            print_help();
            return;
        }
        Some("daemon") => {
            run_daemon(config);
            return;
        }
        Some("uninstall-service") => {
            if let Err(err) = uninstall_service() {
                eprintln!("❌ No se pudo desinstalar el servicio: {}", err);
                std::process::exit(1);
            }
            return;
        }
        Some("update") => {
            if args.len() > 2 {
                eprintln!("❌ Uso inválido: `git-sync update` no acepta parámetros.");
                std::process::exit(1);
            }

            if let Err(err) = update_self() {
                eprintln!("❌ {}", err);
                std::process::exit(1);
            }
            return;
        }
        Some(other) => {
            eprintln!("⚠️ Opción desconocida: {}", other);
            eprintln!("👉 Utilice --help para consultar los comandos disponibles.");
            std::process::exit(1);
        }
        None => {}
    }

    match config.ensure_exists() {
        Ok(_) => {}
        Err(err) => {
            eprintln!("❌ {}", err);
            std::process::exit(1);
        }
    }

    // Sin argumentos: instalar el servicio y abrir la TUI
    if let Err(err) = install_service() {
        eprintln!(
            "⚠️ No fue posible instalar o habilitar el servicio automáticamente: {}",
            err
        );
        eprintln!("👉 Ejecute `sudo git-sync daemon` o complete la instalación de forma manual.");
    }

    if let Err(err) = run_repo_manager(&config) {
        eprintln!("❌ Error al ejecutar el gestor de repositorios: {}", err);
        std::process::exit(1);
    }
}

fn run_daemon(config: Config) {
    let repos_created = match config.ensure_exists() {
        Ok(created) => created,
        Err(err) => {
            eprintln!("❌ {}", err);
            std::process::exit(1);
        }
    };

    if repos_created {
        return;
    }

    let mut settings = Settings::load_or_create(&config.settings_file);
    let logger = Logger::new(config.log_file.clone());

    if settings.verbose {
        logger.log_line("=================================================");
        logger.log_line("🚀 Git Sync - Daemon de sincronización de repositorios");
        logger.log_line("=================================================");
        logger.log_line(&format!(
            "⏱️ Intervalo de sincronización: {} segundos",
            settings.sync_interval
        ));
        logger.log_line(&format!(
            "🛑 Detener ante error: {}",
            settings.stop_on_error
        ));
        logger.log_line(&format!(
            "⌛ Tiempo de espera para Git: {} segundos",
            settings.git_timeout
        ));
        logger.log_line(&format!("🔁 Reintentos máximos: {}", settings.max_retries));
        logger.log_line(&format!("♾️ Modo continuo: {}\n", settings.continuous_mode));
    }

    if !settings.continuous_mode {
        run_sync_cycle(&config, &logger, &settings);
        return;
    }

    loop {
        run_sync_cycle(&config, &logger, &settings);

        if settings.verbose {
            logger.log_line(&format!(
                "\n⏳ En espera de {} segundos antes del siguiente ciclo...\n",
                settings.sync_interval
            ));
        }

        thread::sleep(Duration::from_secs(settings.sync_interval));
        settings.reload(&config.settings_file);
    }
}

fn run_sync_cycle(config: &Config, logger: &Logger, settings: &Settings) {
    let repos = config.read_repos();
    let processor = RepoProcessor::new(logger, settings.verbose);

    match processor.process_all(repos) {
        Ok(_) => {
            if settings.verbose {
                logger.log_line("\n✅ Ciclo completado correctamente.");
            }
        }
        Err(e) => {
            logger.log_error(&e.to_string());
            if settings.stop_on_error {
                logger.log_error("🛑 Finalización por error (stop_on_error=true)");
                std::process::exit(1);
            }
        }
    }
}

fn update_self() -> Result<(), String> {
    println!("🔄 Buscando la última versión en GitHub Releases...");

    if env::consts::OS != "linux" {
        return Err("La actualización automática solo está disponible para Linux.".to_string());
    }

    if env::consts::ARCH != "x86_64" {
        return Err(format!(
            "Arquitectura no soportada para auto-actualización: {} (se espera x86_64).",
            env::consts::ARCH
        ));
    }

    let latest_tag = fetch_latest_release_tag()?;
    install_release(&latest_tag)?;

    println!("\n✅ ¡git-sync se actualizó correctamente a {}!", latest_tag);
    println!("👉 Reinicie el servicio: `sudo systemctl restart git-sync`.");
    Ok(())
}

fn fetch_latest_release_tag() -> Result<String, String> {
    let output = Command::new("curl")
        .args([
            "-fsSL",
            "-H",
            "Accept: application/vnd.github+json",
            "-H",
            "User-Agent: git-sync-updater",
            "https://api.github.com/repos/lui5gl/git-sync/releases/latest",
        ])
        .output()
        .map_err(|e| format!("No se pudo ejecutar `curl`: {}", e))?;

    if !output.status.success() {
        return Err(format!(
            "No se pudo consultar releases en GitHub (estado {}).",
            output.status
        ));
    }

    let body = String::from_utf8(output.stdout)
        .map_err(|e| format!("La respuesta de GitHub no es UTF-8 válida: {}", e))?;

    extract_json_string_value(&body, "tag_name")
        .ok_or("No se pudo obtener el tag de la última release.".to_string())
}

fn extract_json_string_value(json: &str, key: &str) -> Option<String> {
    let pattern = format!("\"{}\":\"", key);
    let index = json.find(&pattern)?;
    let value_start = index + pattern.len();
    let value_end_relative = json[value_start..].find('"')?;
    let value_end = value_start + value_end_relative;
    Some(json[value_start..value_end].to_string())
}

fn detect_asset_name() -> String {
    let output = Command::new("ldd").arg("--version").output();
    let is_musl = match output {
        Ok(out) => {
            let text = format!(
                "{}\n{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            )
            .to_lowercase();
            text.contains("musl")
        }
        Err(_) => false,
    };

    if is_musl {
        "git-sync-linux-x86_64-musl.tar.gz".to_string()
    } else {
        "git-sync-linux-x86_64-glibc.tar.gz".to_string()
    }
}

fn install_release(tag: &str) -> Result<(), String> {
    let asset = detect_asset_name();
    let url = format!(
        "https://github.com/lui5gl/git-sync/releases/download/{}/{}",
        tag, asset
    );

    let temp_dir = env::temp_dir().join(format!("git-sync-update-{}", std::process::id()));
    if temp_dir.exists() {
        fs::remove_dir_all(&temp_dir)
            .map_err(|e| format!("No se pudo limpiar el directorio temporal: {}", e))?;
    }
    fs::create_dir_all(&temp_dir)
        .map_err(|e| format!("No se pudo crear el directorio temporal: {}", e))?;

    let archive_path = temp_dir.join(&asset);
    let archive_path_str = path_to_str(&archive_path)?;

    println!("⬇️ Descargando {} ...", tag);
    let download_status = Command::new("curl")
        .args(["-fL", &url, "-o", archive_path_str])
        .status()
        .map_err(|e| format!("No se pudo ejecutar `curl`: {}", e))?;
    if !download_status.success() {
        return Err(format!(
            "No se pudo descargar {} (estado {}).",
            url, download_status
        ));
    }

    let temp_dir_str = path_to_str(&temp_dir)?;
    let extract_status = Command::new("tar")
        .args(["-xzf", archive_path_str, "-C", temp_dir_str])
        .status()
        .map_err(|e| format!("No se pudo ejecutar `tar`: {}", e))?;
    if !extract_status.success() {
        return Err(format!(
            "No se pudo extraer el archivo descargado (estado {}).",
            extract_status
        ));
    }

    let new_binary = find_binary_in_dir(&temp_dir)
        .ok_or("No se encontró el binario `git-sync` dentro del release descargado.")?;
    let current_binary = env::current_exe()
        .map_err(|e| format!("No se pudo detectar la ruta del binario actual: {}", e))?;
    let staged_binary = staged_path(&current_binary)?;

    fs::copy(&new_binary, &staged_binary).map_err(|e| {
        format!(
            "No se pudo copiar el nuevo binario desde {} a {}: {}",
            new_binary.display(),
            staged_binary.display(),
            e
        )
    })?;

    let permissions = fs::Permissions::from_mode(0o755);
    fs::set_permissions(&staged_binary, permissions)
        .map_err(|e| format!("No se pudieron ajustar permisos del nuevo binario: {}", e))?;

    fs::rename(&staged_binary, &current_binary).map_err(|e| {
        format!(
            "No se pudo reemplazar el binario actual en {}: {}",
            current_binary.display(),
            e
        )
    })?;

    let _ = fs::remove_dir_all(&temp_dir);
    Ok(())
}

fn path_to_str(path: &Path) -> Result<&str, String> {
    path.to_str()
        .ok_or_else(|| format!("Ruta con caracteres UTF-8 inválidos: {}", path.display()))
}

fn staged_path(current_binary: &Path) -> Result<PathBuf, String> {
    let file_name = current_binary
        .file_name()
        .and_then(|f| f.to_str())
        .ok_or_else(|| "No se pudo resolver el nombre del binario actual.".to_string())?;
    Ok(current_binary.with_file_name(format!("{}.new", file_name)))
}

fn find_binary_in_dir(root: &Path) -> Option<PathBuf> {
    let entries = fs::read_dir(root).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if let Some(found) = find_binary_in_dir(&path) {
                return Some(found);
            }
            continue;
        }

        let name = path.file_name().and_then(|n| n.to_str());
        if name == Some("git-sync") {
            return Some(path);
        }
    }
    None
}
