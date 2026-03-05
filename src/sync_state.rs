use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::fs;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RepoSyncState {
    pub repo_path: String,
    pub last_branch: Option<String>,
    pub last_attempt_ts: Option<i64>,
    pub last_success_ts: Option<i64>,
    pub last_error_ts: Option<i64>,
    pub last_error: Option<String>,
    pub last_result: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SyncStateSnapshot {
    pub repos: Vec<RepoSyncState>,
}

impl SyncStateSnapshot {
    pub fn load(path: &str) -> Self {
        let Ok(contents) = fs::read_to_string(path) else {
            return SyncStateSnapshot::default();
        };

        toml::from_str(&contents).unwrap_or_default()
    }

    pub fn save(&self, path: &str) -> Result<(), String> {
        let serialized = toml::to_string_pretty(self)
            .map_err(|e| format!("No se pudo serializar el estado de sincronización: {}", e))?;
        fs::write(path, serialized).map_err(|e| {
            format!(
                "No se pudo guardar el estado de sincronización en {}: {}",
                path, e
            )
        })
    }

    pub fn get(&self, repo_path: &str) -> Option<&RepoSyncState> {
        self.repos.iter().find(|repo| repo.repo_path == repo_path)
    }

    pub fn mark_attempt(&mut self, repo_path: &str) {
        let now = Utc::now().timestamp();
        let repo = self.upsert_repo_mut(repo_path);
        repo.last_attempt_ts = Some(now);
    }

    pub fn mark_success(&mut self, repo_path: &str, branch: String, result: String) {
        let now = Utc::now().timestamp();
        let repo = self.upsert_repo_mut(repo_path);
        repo.last_branch = Some(branch);
        repo.last_success_ts = Some(now);
        repo.last_result = Some(result);
        repo.last_error = None;
    }

    pub fn mark_error(&mut self, repo_path: &str, error: String) {
        let now = Utc::now().timestamp();
        let repo = self.upsert_repo_mut(repo_path);
        repo.last_error_ts = Some(now);
        repo.last_error = Some(error);
    }

    fn upsert_repo_mut(&mut self, repo_path: &str) -> &mut RepoSyncState {
        if let Some(index) = self
            .repos
            .iter()
            .position(|repo| repo.repo_path == repo_path)
        {
            return &mut self.repos[index];
        }

        self.repos.push(RepoSyncState {
            repo_path: repo_path.to_string(),
            ..RepoSyncState::default()
        });
        let len = self.repos.len();
        &mut self.repos[len - 1]
    }
}
