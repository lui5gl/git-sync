use crate::git::GitRepo;
use std::path::Path;

pub struct RepoProcessor;

impl RepoProcessor {
    pub fn process_all(repo_paths: Vec<String>) {
        if repo_paths.is_empty() {
            println!("No repositories found in config file.");
            println!("Please add repository paths (one per line).");
            return;
        }

        println!("Found {} repository/repositories to check\n", repo_paths.len());

        for repo_path in repo_paths {
            Self::process_single(&repo_path);
            println!();
        }

        println!("All repositories processed.");
    }

    fn process_single(repo_path: &str) {
        println!("==========================================");
        println!("Processing: {}", repo_path);
        println!("==========================================");

        if !Self::validate_repo(repo_path) {
            return;
        }

        Self::check_and_pull(repo_path);
    }

    fn validate_repo(repo_path: &str) -> bool {
        if !Path::new(repo_path).exists() {
            println!("❌ Path does not exist: {}", repo_path);
            return false;
        }

        if !Path::new(&format!("{}/.git", repo_path)).exists() {
            println!("❌ Not a git repository: {}", repo_path);
            return false;
        }

        true
    }

    fn check_and_pull(repo_path: &str) {
        let repo = GitRepo::new(repo_path.to_string());

        println!("Checking remote status...");

        // Fetch remote changes
        if let Err(e) = repo.fetch() {
            println!("❌ Failed to fetch: {}", e);
            return;
        }

        // Get default branch
        let branch = repo.get_default_branch();
        println!("Using branch: {}", branch);

        // Check if behind
        match repo.count_commits_behind(&branch) {
            Ok(0) => {
                println!("✅ Already up to date.");
            }
            Ok(count) => {
                println!("Remote has {} new commit(s). Pulling changes...", count);

                match repo.pull(&branch) {
                    Ok(output) => println!("✅ {}", output.trim()),
                    Err(e) => println!("❌ Failed to pull: {}", e),
                }
            }
            Err(e) => {
                println!("❌ Failed to check status: {}", e);
            }
        }
    }
}
