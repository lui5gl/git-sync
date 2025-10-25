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
            self.logger.log_line("No repositories found in config file.");
            self.logger.log_line("Please add repository paths (one per line).");
            return Err("No repositories configured".to_string());
        }

        if self.verbose {
            self.logger.log_line(&format!("Found {} repository/repositories to check\n", repo_paths.len()));
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
                        "⚠️  Omitted repository {} due to error: {}",
                        repo_path, err
                    ));
                }
            }
        }

        if self.verbose {
            self.logger.log_line("All repositories processed.");
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
                "{} repositories failed during sync:\n{}",
                errors.len(),
                details
            ))
        }
    }

    fn process_single(&self, repo_path: &str) -> Result<(), String> {
        if self.verbose {
            self.logger.log_line("==========================================");
            self.logger.log_line(&format!("Processing: {}", repo_path));
            self.logger.log_line("==========================================");
        }

        self.validate_repo(repo_path)?;
        self.check_and_pull(repo_path)?;
        
        Ok(())
    }

    fn validate_repo(&self, repo_path: &str) -> Result<(), String> {
        if !Path::new(repo_path).exists() {
            let msg = format!("❌ Path does not exist: {}", repo_path);
            self.logger.log_error(&msg);
            return Err(msg);
        }

        if !Path::new(&format!("{}/.git", repo_path)).exists() {
            let msg = format!("❌ Not a git repository: {}", repo_path);
            self.logger.log_error(&msg);
            return Err(msg);
        }

        Ok(())
    }

    fn check_and_pull(&self, repo_path: &str) -> Result<(), String> {
        let repo = GitRepo::new(repo_path.to_string());

        if self.verbose {
            self.logger.log_line("Checking remote status...");
        }

        // Fetch remote changes
        if let Err(e) = repo.fetch() {
            let msg = format!("Failed to fetch: {}", e);
            self.logger.log_error(&msg);
            return Err(msg);
        }

        // Get default branch
        let branch = repo.get_default_branch();
        if self.verbose {
            self.logger.log_line(&format!("Using branch: {}", branch));
        }

        // Check if behind
        match repo.count_commits_behind(&branch) {
            Ok(0) => {
                if self.verbose {
                    self.logger.log_line("✅ Already up to date.");
                }
            }
            Ok(count) => {
                if self.verbose {
                    self.logger.log_line(&format!("Remote has {} new commit(s). Pulling changes...", count));
                }

                match repo.pull(&branch) {
                    Ok(output) => {
                        if self.verbose {
                            self.logger.log_line(&format!("✅ {}", output.trim()));
                        }
                    }
                    Err(e) => {
                        let msg = format!("Failed to pull: {}", e);
                        self.logger.log_error(&msg);
                        return Err(msg);
                    }
                }
            }
            Err(e) => {
                let msg = format!("Failed to check status: {}", e);
                self.logger.log_error(&msg);
                return Err(msg);
            }
        }

        Ok(())
    }
}
