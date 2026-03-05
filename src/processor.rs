use crate::config::RepoDefinition;
use crate::git::GitRepo;
use crate::logger::Logger;
use crate::sync_state::SyncStateSnapshot;
use std::path::Path;
use std::process::Command;

pub struct RepoProcessor<'a> {
    logger: &'a Logger,
    verbose: bool,
    state_file: String,
}

struct PullOutcome {
    branch: String,
    result: String,
    last_pulled_commit: Option<String>,
    did_pull: bool,
}

impl<'a> RepoProcessor<'a> {
    pub fn new(logger: &'a Logger, verbose: bool, state_file: String) -> Self {
        RepoProcessor {
            logger,
            verbose,
            state_file,
        }
    }

    pub fn process_all(&self, repo_defs: Vec<RepoDefinition>) -> Result<(), String> {
        if repo_defs.is_empty() {
            self.logger
                .log_line("⚠️ No se encontraron repositorios en el archivo de configuración.");
            self.logger
                .log_line("👉 Agregue las rutas de los repositorios, una por línea.");
            return Err("No hay repositorios configurados".to_string());
        }

        if self.verbose {
            self.logger.log_line(&format!(
                "📦 Se analizarán {} repositorios\n",
                repo_defs.len()
            ));
        }

        let mut sync_state = SyncStateSnapshot::load(&self.state_file);
        let mut errors: Vec<(String, String)> = Vec::new();

        for repo in repo_defs {
            if !repo.enabled {
                if self.verbose {
                    self.logger.log_line(&format!(
                        "⏸️ Repositorio pausado (sync desactivado): {}",
                        repo.repo_path
                    ));
                }
                continue;
            }

            sync_state.mark_attempt(&repo.repo_path);

            match self.process_single(&repo) {
                Ok((branch, result, last_pulled_commit)) => {
                    sync_state.mark_success(&repo.repo_path, branch, result, last_pulled_commit);
                    if self.verbose {
                        self.logger.log("\n");
                    }
                }
                Err(err) => {
                    sync_state.mark_error(&repo.repo_path, err.clone());
                    errors.push((repo.repo_path.clone(), err.clone()));
                    self.logger.log_line(&format!(
                        "⚠️ Repositorio omitido {} debido a un error: {}",
                        repo.repo_path, err
                    ));
                }
            }
        }

        if let Err(state_err) = sync_state.save(&self.state_file) {
            self.logger.log_line(&format!(
                "⚠️ No se pudo actualizar el archivo de estado de sincronización: {}",
                state_err
            ));
        }

        if self.verbose {
            self.logger
                .log_line("🎉 Todos los repositorios fueron procesados.");
        }

        if errors.is_empty() {
            Ok(())
        } else {
            let details = errors
                .iter()
                .map(|(repo, err)| format!("- {} => {}", repo, err))
                .collect::<Vec<_>>()
                .join("\n");
            Err(format!(
                "{} repositorios presentaron errores durante la sincronización:\n{}",
                errors.len(),
                details
            ))
        }
    }

    fn process_single(
        &self,
        repo: &RepoDefinition,
    ) -> Result<(String, String, Option<String>), String> {
        if self.verbose {
            self.logger
                .log_line("==========================================");
            self.logger
                .log_line(&format!("🔄 Procesando repositorio: {}", repo.repo_path));
            self.logger
                .log_line("==========================================");
        }

        self.validate_repo(&repo.repo_path)?;
        let mut outcome = self.check_and_pull(&repo.repo_path)?;

        if outcome.did_pull
            && let Some(command) = repo
                .post_sync_command
                .as_ref()
                .map(|value| value.trim())
                .filter(|value| !value.is_empty())
        {
            let command_result = self.run_post_sync_command(&repo.repo_path, command)?;
            outcome.result = format!("{} | post-sync: {}", outcome.result, command_result);
        }

        Ok((outcome.branch, outcome.result, outcome.last_pulled_commit))
    }

    fn validate_repo(&self, repo_path: &str) -> Result<(), String> {
        if !Path::new(repo_path).exists() {
            let msg = format!("❌ La ruta no existe: {}", repo_path);
            self.logger.log_error(&msg);
            return Err(msg);
        }

        if !Path::new(&format!("{}/.git", repo_path)).exists() {
            let msg = format!(
                "❌ El directorio no es un repositorio Git válido: {}",
                repo_path
            );
            self.logger.log_error(&msg);
            return Err(msg);
        }

        Ok(())
    }

    fn check_and_pull(&self, repo_path: &str) -> Result<PullOutcome, String> {
        let repo = GitRepo::new(repo_path.to_string());

        if self.verbose {
            self.logger
                .log_line("🔍 Verificando el estado del remoto...");
        }

        if let Err(e) = repo.fetch() {
            let msg = format!("❌ No se pudo ejecutar `git fetch`: {}", e);
            self.logger.log_error(&msg);
            return Err(msg);
        }

        let branch = repo.get_default_branch();
        if self.verbose {
            self.logger
                .log_line(&format!("Se utilizará la rama: {}", branch));
        }

        match repo.count_commits_behind(&branch) {
            Ok(0) => {
                if self.verbose {
                    self.logger
                        .log_line("✅ El repositorio ya está actualizado.");
                }
                Ok(PullOutcome {
                    branch,
                    result: "Sin cambios remotos".to_string(),
                    last_pulled_commit: None,
                    did_pull: false,
                })
            }
            Ok(count) => {
                if self.verbose {
                    self.logger.log_line(&format!(
                        "⬇️ El remoto tiene {} confirmaciones nuevas. Aplicando cambios...",
                        count
                    ));
                }

                match repo.pull(&branch) {
                    Ok(output) => {
                        if self.verbose {
                            self.logger.log_line(&format!(
                                "📥 Resultado de `git pull`:\n{}",
                                output.trim()
                            ));
                        }
                        let pulled_commit = repo.head_commit_summary().ok();
                        Ok(PullOutcome {
                            branch,
                            result: format!("Pull aplicado: {} commit(s)", count),
                            last_pulled_commit: pulled_commit,
                            did_pull: true,
                        })
                    }
                    Err(e) => {
                        let msg = format!("❌ No se pudo ejecutar `git pull`: {}", e);
                        self.logger.log_error(&msg);
                        Err(msg)
                    }
                }
            }
            Err(e) => {
                let msg = format!("❌ No se pudo consultar el estado del repositorio: {}", e);
                self.logger.log_error(&msg);
                Err(msg)
            }
        }
    }

    fn run_post_sync_command(&self, repo_path: &str, command: &str) -> Result<String, String> {
        if self.verbose {
            self.logger.log_line(&format!(
                "🧪 Ejecutando comando post-sync en {}: {}",
                repo_path, command
            ));
        }

        let output = Command::new("sh")
            .arg("-lc")
            .arg(command)
            .current_dir(repo_path)
            .output()
            .map_err(|e| {
                format!(
                    "❌ No se pudo ejecutar el comando post-sync `{}`: {}",
                    command, e
                )
            })?;

        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if stdout.is_empty() {
                Ok("ok".to_string())
            } else {
                Ok(truncate_single_line(&stdout, 80))
            }
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let details = if !stderr.is_empty() { stderr } else { stdout };
            Err(format!(
                "❌ Comando post-sync falló (`{}`): {}",
                command,
                truncate_single_line(&details, 200)
            ))
        }
    }
}

fn truncate_single_line(message: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }

    let normalized = message.replace('\n', " ");
    let mut out = String::new();
    for (count, ch) in normalized.chars().enumerate() {
        if count >= max_chars {
            out.push('…');
            return out;
        }
        out.push(ch);
    }
    out
}
