use crate::config::Config;
use std::env;
use std::fs::{self, File};
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;

const SERVICE_NAME: &str = "git-sync.service";
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

    Err("Unable to determine user information for service installation".to_string())
}

fn write_service_file(content: &str) -> Result<(), String> {
    let parent = Path::new(SERVICE_PATH)
        .parent()
        .ok_or_else(|| "Invalid service path".to_string())?;

    if !parent.exists() {
        return Err(format!(
            "Service directory {} does not exist. Are you on a systemd-based system?",
            parent.display()
        ));
    }

    let mut file =
        File::create(SERVICE_PATH).map_err(|e| format!("Failed to create service file: {}", e))?;
    file.write_all(content.as_bytes())
        .map_err(|e| format!("Failed to write service file: {}", e))?;
    file.sync_all()
        .map_err(|e| format!("Failed to flush service file to disk: {}", e))?;

    let permissions = fs::Permissions::from_mode(0o644);
    fs::set_permissions(SERVICE_PATH, permissions)
        .map_err(|e| format!("Failed to set service file permissions: {}", e))?;

    Ok(())
}

fn run_systemctl(args: &[&str]) {
    match Command::new("systemctl").args(args).status() {
        Ok(status) if status.success() => {
            println!("✅ systemctl {} executed successfully", args.join(" "));
        }
        Ok(status) => {
            eprintln!(
                "⚠️  systemctl {} exited with status {}. You may need to run it manually.",
                args.join(" "),
                status
            );
        }
        Err(e) => {
            eprintln!(
                "⚠️  Failed to execute systemctl {}: {}. You may need to run it manually.",
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
        .map_err(|e| format!("Failed to change ownership for {}: {}", path, e))?;

    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "chown command for {} exited with status {}",
            path, status
        ))
    }
}

pub fn install_service() -> Result<(), String> {
    if Path::new(SERVICE_PATH).exists() {
        return Ok(());
    }

    let exe_path = env::current_exe()
        .map_err(|e| format!("Unable to determine current executable path: {}", e))?;
    let exec_display = exe_path
        .to_str()
        .ok_or_else(|| "Executable path contains invalid UTF-8".to_string())?;

    let (username, home_dir) = resolve_service_user()?;
    let config = Config::new();

    let _ = config
        .ensure_exists()
        .map_err(|e| format!("Failed to initialize configuration layout: {}", e))?;

    chown_path(&config.log_dir, &username)?;
    chown_path(&config.log_file, &username)?;

    let service_content = format!(
        "[Unit]\nDescription=Git Sync daemon\nAfter=network-online.target\nWants=network-online.target\n\n[Service]\nType=simple\nUser={username}\nWorkingDirectory={home_dir}\nEnvironment=HOME={home_dir}\nExecStart={exec_display} daemon\nRestart=on-failure\nRestartSec=60\n\n[Install]\nWantedBy=multi-user.target\n"
    );

    write_service_file(&service_content)?;

    run_systemctl(&["daemon-reload"]);
    run_systemctl(&["enable", "--now", SERVICE_NAME]);

    Ok(())
}

pub fn uninstall_service() -> Result<(), String> {
    if !Path::new(SERVICE_PATH).exists() {
        return Err("git-sync service is not installed".to_string());
    }

    run_systemctl(&["disable", "--now", SERVICE_NAME]);

    fs::remove_file(SERVICE_PATH).map_err(|e| format!("Failed to remove service file: {}", e))?;

    run_systemctl(&["daemon-reload"]);

    println!("✅ Removed service file {}", SERVICE_PATH);
    println!(
        "ℹ️  You can verify the service removal with: sudo systemctl status {}",
        SERVICE_NAME
    );

    Ok(())
}
