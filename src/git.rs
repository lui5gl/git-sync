use std::process::Command;

pub struct GitRepo {
    pub path: String,
}

impl GitRepo {
    pub fn new(path: String) -> Self {
        GitRepo { path }
    }

    pub fn fetch(&self) -> Result<(), String> {
        let output = Command::new("git")
            .current_dir(&self.path)
            .arg("fetch")
            .output()
            .map_err(|e| format!("No se pudo ejecutar `git fetch`: {}", e))?;

        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr).to_string());
        }

        Ok(())
    }

    pub fn get_default_branch(&self) -> String {
        // Intentar detectar la rama predeterminada
        let branch_output = Command::new("git")
            .current_dir(&self.path)
            .args(&["symbolic-ref", "refs/remotes/origin/HEAD"])
            .output();

        if let Ok(output) = branch_output {
            let default_branch = String::from_utf8_lossy(&output.stdout)
                .trim()
                .replace("refs/remotes/origin/", "");

            if !default_branch.is_empty() {
                return default_branch;
            }
        }

        // Alternativa: verificar qué rama está disponible
        let main_exists = Command::new("git")
            .current_dir(&self.path)
            .args(&["rev-parse", "--verify", "origin/main"])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);

        if main_exists {
            "main".to_string()
        } else {
            "master".to_string()
        }
    }

    pub fn count_commits_behind(&self, branch: &str) -> Result<usize, String> {
        let output = Command::new("git")
            .current_dir(&self.path)
            .args(&["rev-list", "--count", &format!("HEAD..origin/{}", branch)])
            .output()
            .map_err(|e| format!("No se pudo comprobar el estado de Git: {}", e))?;

        let count = String::from_utf8_lossy(&output.stdout)
            .trim()
            .parse::<usize>()
            .unwrap_or(0);

        Ok(count)
    }

    pub fn pull(&self, branch: &str) -> Result<String, String> {
        let output = Command::new("git")
            .current_dir(&self.path)
            .args(&["pull", "origin", branch])
            .output()
            .map_err(|e| format!("No se pudo ejecutar `git pull`: {}", e))?;

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).to_string())
        } else {
            Err(String::from_utf8_lossy(&output.stderr).to_string())
        }
    }
}
