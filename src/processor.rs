use crate::config::RepoDefinition;
use crate::git::GitRepo;
use crate::logger::Logger;
use std::fs;
use std::path::{Path, PathBuf};

pub struct RepoProcessor<'a> {
    logger: &'a Logger,
    verbose: bool,
}

impl<'a> RepoProcessor<'a> {
    pub fn new(logger: &'a Logger, verbose: bool) -> Self {
        RepoProcessor { logger, verbose }
    }

    pub fn process_all(&self, repo_defs: Vec<RepoDefinition>) -> Result<(), String> {
        if repo_defs.is_empty() {
            self.logger
                .log_line("No se encontraron repositorios en el archivo de configuración.");
            self.logger
                .log_line("Agregue las rutas de los repositorios, una por línea.");
            return Err("No hay repositorios configurados".to_string());
        }

        if self.verbose {
            self.logger
                .log_line(&format!("Se analizarán {} repositorios\n", repo_defs.len()));
        }

        let mut errors: Vec<(String, String)> = Vec::new();

        for repo in repo_defs {
            match self.process_single(&repo) {
                Ok(_) => {
                    if self.verbose {
                        self.logger.log("\n");
                    }
                }
                Err(err) => {
                    errors.push((repo.repo_path.clone(), err.clone()));
                    self.logger.log_line(&format!(
                        "Repositorio omitido {} debido a un error: {}",
                        repo.repo_path, err
                    ));
                }
            }
        }

        if self.verbose {
            self.logger
                .log_line("Todos los repositorios fueron procesados.");
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

    fn process_single(&self, repo: &RepoDefinition) -> Result<(), String> {
        if self.verbose {
            self.logger
                .log_line("==========================================");
            match &repo.deploy_target {
                Some(target) => self.logger.log_line(&format!(
                    "Procesando repositorio con build: {} -> {}",
                    repo.repo_path, target
                )),
                None => self
                    .logger
                    .log_line(&format!("Procesando repositorio: {}", repo.repo_path)),
            }
            self.logger
                .log_line("==========================================");
        }

        self.validate_repo(&repo.repo_path)?;
        self.check_and_pull(&repo.repo_path)?;

        if let Some(target) = &repo.deploy_target {
            self.build_and_deploy(&repo.repo_path, target)?;
        }

        Ok(())
    }

    fn validate_repo(&self, repo_path: &str) -> Result<(), String> {
        if !Path::new(repo_path).exists() {
            let msg = format!("La ruta no existe: {}", repo_path);
            self.logger.log_error(&msg);
            return Err(msg);
        }

        if !Path::new(&format!("{}/.git", repo_path)).exists() {
            let msg = format!(
                "El directorio no es un repositorio Git válido: {}",
                repo_path
            );
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
            self.logger
                .log_line(&format!("Se utilizará la rama: {}", branch));
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
                    self.logger.log_line(&format!(
                        "El remoto tiene {} confirmaciones nuevas. Aplicando cambios...",
                        count
                    ));
                }

                match repo.pull(&branch) {
                    Ok(output) => {
                        if self.verbose {
                            self.logger
                                .log_line(&format!("Resultado de `git pull`:\n{}", output.trim()));
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

    fn build_and_deploy(&self, repo_path: &str, destination: &str) -> Result<(), String> {
        if self.verbose {
            self.logger.log_line("Ejecutando `npm run build`...");
        }

        let status = std::process::Command::new("npm")
            .current_dir(repo_path)
            .arg("run")
            .arg("build")
            .status()
            .map_err(|e| format!("No se pudo ejecutar `npm run build`: {}", e))?;

        if !status.success() {
            return Err(format!(
                "`npm run build` finalizó con estado {}",
                status.code().unwrap_or(-1)
            ));
        }

        let dist_path = Path::new(repo_path).join("dist");
        if !dist_path.exists() {
            return Err(format!(
                "No se encontró el directorio de salida en {}",
                dist_path.display()
            ));
        }

        let destination_path = PathBuf::from(destination);
        if destination_path.exists() {
            fs::remove_dir_all(&destination_path).map_err(|e| {
                format!(
                    "No se pudo limpiar el destino {}: {}",
                    destination_path.display(),
                    e
                )
            })?;
        }

        fs::create_dir_all(&destination_path).map_err(|e| {
            format!(
                "No se pudo crear el directorio destino {}: {}",
                destination_path.display(),
                e
            )
        })?;

        self.copy_recursive(&dist_path, &destination_path)?;

        if self.verbose {
            self.logger.log_line(&format!(
                "Archivos desplegados en {}",
                destination_path.display()
            ));
        }

        Ok(())
    }

    fn copy_recursive(&self, source: &Path, destination: &Path) -> Result<(), String> {
        if source.is_dir() {
            for entry in fs::read_dir(source)
                .map_err(|e| format!("No se pudo leer el directorio {}: {}", source.display(), e))?
            {
                let entry = entry.map_err(|e| {
                    format!(
                        "No se pudo procesar una entrada en {}: {}",
                        source.display(),
                        e
                    )
                })?;
                let file_type = entry.file_type().map_err(|e| {
                    format!("No se pudo determinar el tipo de {:?}: {}", entry.path(), e)
                })?;

                let dest_path = destination.join(entry.file_name());
                if file_type.is_dir() {
                    fs::create_dir_all(&dest_path).map_err(|e| {
                        format!(
                            "No se pudo crear el directorio {}: {}",
                            dest_path.display(),
                            e
                        )
                    })?;
                    self.copy_recursive(&entry.path(), &dest_path)?;
                } else if file_type.is_file() {
                    fs::copy(entry.path(), &dest_path).map_err(|e| {
                        format!(
                            "No se pudo copiar {} a {}: {}",
                            entry.path().display(),
                            dest_path.display(),
                            e
                        )
                    })?;
                } else if file_type.is_symlink() {
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs as unix_fs;
                        let target = fs::read_link(entry.path()).map_err(|e| {
                            format!(
                                "No se pudo leer el enlace simbólico {}: {}",
                                entry.path().display(),
                                e
                            )
                        })?;
                        unix_fs::symlink(target, &dest_path).map_err(|e| {
                            format!(
                                "No se pudo recrear el enlace simbólico {}: {}",
                                dest_path.display(),
                                e
                            )
                        })?;
                    }

                    #[cfg(not(unix))]
                    {
                        return Err(format!(
                            "Los enlaces simbólicos no son compatibles en este sistema: {}",
                            entry.path().display()
                        ));
                    }
                }
            }
        }

        Ok(())
    }
}
