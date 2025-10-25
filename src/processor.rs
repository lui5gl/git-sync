use crate::git::GitRepo;
use crate::logger::Logger;
use std::path::Path;

pub struct RepoProcessor<'a> {
    logger: &'a Logger,
    verbose: bool,
}

impl<'a> RepoProcessor<'a> {
    pub fn new(logger: &'a Logger, verbose: bool) -> Self {
        RepoProcessor { logger, verbose }
    }

    pub fn process_all(&self, repo_paths: Vec<String>) -> Result<(), String> {
        if repo_paths.is_empty() {
            self.logger.log_line("No se encontraron repositorios en el archivo de configuración.");
            self.logger
                .log_line("Agregue las rutas de los repositorios, una por línea.");
            return Err("No hay repositorios configurados".to_string());
        }

        if self.verbose {
            self.logger
                .log_line(&format!("Se analizarán {} repositorios\n", repo_paths.len()));
        }

        let mut errors: Vec<(String, String)> = Vec::new();

        for repo_path in repo_paths {
            match self.process_single(&repo_path) {
                Ok(_) => {
                    if self.verbose {
                        self.logger.log("\n");
                    }
                }
                Err(err) => {
                    errors.push((repo_path.clone(), err.clone()));
                    self.logger.log_line(&format!(
                        "Repositorio omitido {} debido a un error: {}",
                        repo_path, err
                    ));
                }
            }
        }

        if self.verbose {
            self.logger.log_line("Todos los repositorios fueron procesados.");
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

    fn process_single(&self, repo_path: &str) -> Result<(), String> {
        if self.verbose {
            self.logger.log_line("==========================================");
            self.logger
                .log_line(&format!("Procesando repositorio: {}", repo_path));
            self.logger.log_line("==========================================");
        }

        self.validate_repo(repo_path)?;
        self.check_and_pull(repo_path)?;

        Ok(())
    }

    fn validate_repo(&self, repo_path: &str) -> Result<(), String> {
        if !Path::new(repo_path).exists() {
            let msg = format!("La ruta no existe: {}", repo_path);
            self.logger.log_error(&msg);
            return Err(msg);
        }

        if !Path::new(&format!("{}/.git", repo_path)).exists() {
            let msg = format!("El directorio no es un repositorio Git válido: {}", repo_path);
            self.logger.log_error(&msg);
            return Err(msg);
        }

        Ok(())
    }

    fn check_and_pull(&self, repo_path: &str) -> Result<(), String> {
        let repo = GitRepo::new(repo_path.to_string());

        if self.verbose {
            self.logger.log_line("Verificando el estado del remoto...");
        }

        // Obtener cambios del remoto
        if let Err(e) = repo.fetch() {
            let msg = format!("No se pudo ejecutar `git fetch`: {}", e);
            self.logger.log_error(&msg);
            return Err(msg);
        }

        // Determinar la rama predeterminada
        let branch = repo.get_default_branch();
        if self.verbose {
            self.logger.log_line(&format!("Se utilizará la rama: {}", branch));
        }

        // Revisar si el repositorio local está desfasado
        match repo.count_commits_behind(&branch) {
            Ok(0) => {
                if self.verbose {
                    self.logger.log_line("El repositorio ya está actualizado.");
                }
            }
            Ok(count) => {
                if self.verbose {
                    self.logger
                        .log_line(&format!("El remoto tiene {} confirmaciones nuevas. Aplicando cambios...", count));
                }

                match repo.pull(&branch) {
                    Ok(output) => {
                        if self.verbose {
                            self.logger.log_line(&format!("Resultado de `git pull`:\n{}", output.trim()));
                        }
                    }
                    Err(e) => {
                        let msg = format!("No se pudo ejecutar `git pull`: {}", e);
                        self.logger.log_error(&msg);
                        return Err(msg);
                    }
                }
            }
            Err(e) => {
                let msg = format!("No se pudo consultar el estado del repositorio: {}", e);
                self.logger.log_error(&msg);
                return Err(msg);
            }
        }

        Ok(())
    }
}
