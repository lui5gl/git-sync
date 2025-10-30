use crate::config::Config;
use std::env;
use std::fs::{self, File};
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;

const SERVICE_NAME: &str = "git-sync";
const SERVICE_PATH: &str = "/etc/systemd/system/git-sync.service";

fn resolve_service_user() -> Result<(String, String), String> {
    fn home_for_user(username: &str) -> Option<String> {
        if let Ok(contents) = fs::read_to_string("/etc/passwd") {
            for line in contents.lines() {
                let parts: Vec<&str> = line.split(':').collect();
                if parts.len() >= 6 && parts[0] == username {
                    return Some(parts[5].to_string());
                }
            }
        }
        None
    }

    if let Ok(sudo_user) = env::var("SUDO_USER") {
        if let Some(home) = home_for_user(&sudo_user) {
            return Ok((sudo_user, home));
        }
    }

    if let Ok(user) = env::var("USER") {
        if let Some(home) = home_for_user(&user) {
            return Ok((user, home));
        }
    }

    if let Ok(home) = env::var("HOME") {
        if let Ok(user) = env::var("USER").or_else(|_| env::var("LOGNAME")) {
            return Ok((user, home));
        }
    }

    Err(
        "❌ No fue posible determinar la información del usuario para instalar el servicio"
            .to_string(),
    )
}

fn write_service_file(content: &str) -> Result<(), String> {
    let parent = Path::new(SERVICE_PATH)
        .parent()
        .ok_or_else(|| "❌ La ruta del servicio no es válida".to_string())?;

    if !parent.exists() {
        return Err(format!(
            "❌ El directorio del servicio {} no existe. ¿El sistema utiliza systemd?",
            parent.display()
        ));
    }

    let mut file = File::create(SERVICE_PATH)
        .map_err(|e| format!("❌ No se pudo crear el archivo de servicio: {}", e))?;
    file.write_all(content.as_bytes())
        .map_err(|e| format!("❌ No se pudo escribir el archivo de servicio: {}", e))?;
    file.sync_all().map_err(|e| {
        format!(
            "❌ No se pudo sincronizar el archivo de servicio en disco: {}",
            e
        )
    })?;

    let permissions = fs::Permissions::from_mode(0o644);
    fs::set_permissions(SERVICE_PATH, permissions).map_err(|e| {
        format!(
            "❌ No se pudieron asignar permisos al archivo de servicio: {}",
            e
        )
    })?;

    Ok(())
}

fn run_systemctl(args: &[&str]) {
    match Command::new("systemctl").args(args).status() {
        Ok(status) if status.success() => {
            println!("✅ systemctl {} se ejecutó correctamente", args.join(" "));
        }
        Ok(status) => {
            eprintln!(
                "⚠️ systemctl {} finalizó con el estado {}. Es posible que deba ejecutarlo manualmente.",
                args.join(" "),
                status
            );
        }
        Err(e) => {
            eprintln!(
                "❌ No se pudo ejecutar systemctl {}: {}. Ejecútelo manualmente si es necesario.",
                args.join(" "),
                e
            );
        }
    }
}

fn chown_path(path: &str, username: &str) -> Result<(), String> {
    let status = Command::new("chown")
        .arg(format!("{}:{}", username, username))
        .arg(path)
        .status()
        .map_err(|e| format!("❌ No se pudo cambiar la propiedad de {}: {}", path, e))?;

    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "❌ El comando chown para {} finalizó con el estado {}",
            path, status
        ))
    }
}

pub fn install_service() -> Result<(), String> {
    if Path::new(SERVICE_PATH).exists() {
        return Ok(());
    }

    let exe_path = env::current_exe().map_err(|e| {
        format!(
            "❌ No se pudo determinar la ruta del ejecutable actual: {}",
            e
        )
    })?;
    let exec_display = exe_path.to_str().ok_or_else(|| {
        "❌ La ruta del ejecutable contiene caracteres UTF-8 no válidos".to_string()
    })?;

    let (username, home_dir) = resolve_service_user()?;
    let config = Config::new();

    let _ = config.ensure_exists().map_err(|e| {
        format!(
            "❌ No se pudo inicializar la estructura de configuración: {}",
            e
        )
    })?;

    chown_path(&config.log_dir, &username)?;
    chown_path(&config.log_file, &username)?;

    let service_content = format!(
        "[Unit]\nDescription=Daemon de sincronización de Git Sync\nAfter=network-online.target\nWants=network-online.target\n\n[Service]\nType=simple\nUser={username}\nWorkingDirectory={home_dir}\nEnvironment=HOME={home_dir}\nExecStart={exec_display} daemon\nRestart=on-failure\nRestartSec=60\n\n[Install]\nWantedBy=multi-user.target\n"
    );

    write_service_file(&service_content)?;

    run_systemctl(&["daemon-reload"]);
    run_systemctl(&["enable", "--now", SERVICE_NAME]);

    Ok(())
}

pub fn uninstall_service() -> Result<(), String> {
    if !Path::new(SERVICE_PATH).exists() {
        return Err("ℹ️ El servicio git-sync no está instalado".to_string());
    }

    run_systemctl(&["disable", "--now", SERVICE_NAME]);

    fs::remove_file(SERVICE_PATH)
        .map_err(|e| format!("❌ No se pudo eliminar el archivo de servicio: {}", e))?;

    run_systemctl(&["daemon-reload"]);

    println!("🗑️ Archivo de servicio eliminado: {}", SERVICE_PATH);
    println!(
        "🔍 Verifique la eliminación del servicio con: sudo systemctl status {}",
        SERVICE_NAME
    );

    Ok(())
}
