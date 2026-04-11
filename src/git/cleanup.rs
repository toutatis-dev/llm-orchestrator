use crate::git::branch::BranchManager;
use anyhow::{Context, Result};
use std::path::Path;

/// Report on a cleanup operation
#[derive(Debug, Clone)]
pub struct CleanupReport {
    pub session_id: String,
    pub branches_deleted: usize,
    pub worktrees_removed: usize,
    pub disk_space_reclaimed: u64, // in bytes
}

/// Manages cleanup of branches and worktrees after execution
pub struct BranchCleanup {
    branch_manager: BranchManager,
    worktree_base: std::path::PathBuf,
}

impl BranchCleanup {
    /// Create a new BranchCleanup for the given repository
    pub fn new(repo_path: &Path) -> Result<Self> {
        let branch_manager = BranchManager::new(repo_path)?;

        // Worktree base path (same as in WorktreeManager)
        let worktree_base = repo_path.join(".orchestrator-worktrees");

        Ok(Self {
            branch_manager,
            worktree_base,
        })
    }

    /// Clean up all session branches and worktrees on successful completion
    ///
    /// This deletes all branches matching `orchestrator/{session_id}/`
    /// and removes the worktree directory.
    pub fn cleanup_success(&self, session_id: &str) -> Result<CleanupReport> {
        let prefix = format!("orchestrator/{}/", session_id);

        // Find all session branches
        let branches = self
            .branch_manager
            .list_branches_with_prefix(&prefix)
            .with_context(|| format!("Failed to list branches for session {}", session_id))?;

        let mut branches_deleted = 0;
        let mut errors = Vec::new();

        // Delete each branch
        for branch in branches {
            match self.branch_manager.delete_branch(&branch) {
                Ok(_) => {
                    branches_deleted += 1;
                    tracing::info!("Deleted branch '{}'", branch);
                }
                Err(e) => {
                    tracing::error!("Failed to delete branch '{}': {}", branch, e);
                    errors.push(format!("{}: {}", branch, e));
                }
            }
        }

        // Calculate disk usage before removal
        let session_path = self.worktree_base.join(session_id);
        let disk_space_reclaimed = if session_path.exists() {
            self.calculate_dir_size(&session_path).unwrap_or(0)
        } else {
            0
        };

        // Remove worktree directory
        let worktrees_removed = if session_path.exists() {
            match std::fs::remove_dir_all(&session_path) {
                Ok(_) => {
                    tracing::info!("Removed worktree directory {:?}", session_path);
                    1
                }
                Err(e) => {
                    tracing::error!(
                        "Failed to remove worktree directory {:?}: {}",
                        session_path,
                        e
                    );
                    errors.push(format!("worktree: {}", e));
                    0
                }
            }
        } else {
            0
        };

        if !errors.is_empty() {
            tracing::warn!("Cleanup completed with {} errors", errors.len());
        }

        Ok(CleanupReport {
            session_id: session_id.to_string(),
            branches_deleted,
            worktrees_removed,
            disk_space_reclaimed,
        })
    }

    /// Clean up a failed session - keep branches for forensics but report them
    ///
    /// Unlike cleanup_success, this does NOT delete branches, only reports them
    /// for manual inspection. This allows debugging of failed executions.
    pub fn cleanup_failed_session(&self, session_id: &str) -> Result<CleanupReport> {
        let prefix = format!("orchestrator/{}/", session_id);

        // Find all session branches (but don't delete them)
        let branches = self
            .branch_manager
            .list_branches_with_prefix(&prefix)
            .with_context(|| format!("Failed to list branches for session {}", session_id))?;

        // Calculate disk usage
        let session_path = self.worktree_base.join(session_id);
        let disk_space = if session_path.exists() {
            self.calculate_dir_size(&session_path).unwrap_or(0)
        } else {
            0
        };

        tracing::info!(
            "Failed session {}: {} branches preserved for forensics",
            session_id,
            branches.len()
        );

        for branch in &branches {
            tracing::info!("  Preserved: {}", branch);
        }

        Ok(CleanupReport {
            session_id: session_id.to_string(),
            branches_deleted: 0, // Intentionally not deleted
            worktrees_removed: 0,
            disk_space_reclaimed: disk_space, // Just reporting, not reclaimed
        })
    }

    /// List all orchestrator branches with their status
    pub fn list_orchestrator_branches(&self) -> Result<Vec<(String, String)>> {
        let prefix = "orchestrator/";
        let branches = self.branch_manager.list_branches_with_prefix(prefix)?;

        let mut results = Vec::new();
        for branch in branches {
            // Extract session ID from branch name
            // Format: orchestrator/{session}/...
            let parts: Vec<&str> = branch.split('/').collect();
            let session_id = parts.get(1).unwrap_or(&"unknown").to_string();
            results.push((branch, session_id));
        }

        Ok(results)
    }

    /// Clean up old sessions (branches older than a certain age)
    ///
    /// Note: This requires access to commit dates, which we don't currently track.
    /// For now, this is a placeholder for future implementation.
    pub fn cleanup_old_sessions(&self, _older_than_days: u32) -> Result<Vec<CleanupReport>> {
        // TODO: Implement based on commit dates
        // For now, just return empty
        Ok(Vec::new())
    }

    /// Get disk usage summary for all orchestrator worktrees
    pub fn disk_usage_summary(&self) -> Result<(usize, u64)> {
        let mut count = 0;
        let mut total_size = 0u64;

        if self.worktree_base.exists() {
            for entry in std::fs::read_dir(&self.worktree_base)? {
                let entry = entry?;
                if entry.file_type()?.is_dir() {
                    count += 1;
                    total_size += self.calculate_dir_size(&entry.path()).unwrap_or(0);
                }
            }
        }

        Ok((count, total_size))
    }

    /// Calculate the total size of a directory
    fn calculate_dir_size(&self, path: &std::path::Path) -> Result<u64> {
        let mut total_size = 0u64;

        for entry in walkdir::WalkDir::new(path) {
            let entry = entry?;
            if entry.file_type().is_file() {
                total_size += entry.metadata()?.len();
            }
        }

        Ok(total_size)
    }
}

impl CleanupReport {
    /// Format disk space in human-readable form
    pub fn format_disk_space(&self) -> String {
        format_bytes(self.disk_space_reclaimed)
    }

    /// Print a summary of the cleanup
    pub fn print_summary(&self) {
        println!("╔════════════════════════════════════════════════════════════╗");
        println!("║                   Cleanup Summary                          ║");
        println!("╠════════════════════════════════════════════════════════════╣");
        println!("║  Session:         {:<40} ║", self.session_id);
        println!("║  Branches:        {:<40} ║", self.branches_deleted);
        println!("║  Worktrees:       {:<40} ║", self.worktrees_removed);
        println!("║  Disk Space:      {:<40} ║", self.format_disk_space());
        println!("╚════════════════════════════════════════════════════════════╝");
    }
}

/// Format bytes into human-readable string
fn format_bytes(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    let mut unit_index = 0;

    while size >= 1024.0 && unit_index < UNITS.len() - 1 {
        size /= 1024.0;
        unit_index += 1;
    }

    format!("{:.2} {}", size, UNITS[unit_index])
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup_test_repo() -> (TempDir, BranchCleanup) {
        let temp_dir = TempDir::new().unwrap();
        let repo = git2::Repository::init(&temp_dir).unwrap();

        // Create initial commit
        let sig = git2::Signature::now("Test", "test@example.com").unwrap();
        let tree_id = {
            let mut index = repo.index().unwrap();
            index.write_tree().unwrap()
        };
        let tree = repo.find_tree(tree_id).unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "Initial commit", &tree, &[])
            .unwrap();

        let cleanup = BranchCleanup::new(temp_dir.path()).unwrap();
        (temp_dir, cleanup)
    }

    #[test]
    fn test_cleanup_report_formatting() {
        let report = CleanupReport {
            session_id: "test-session".to_string(),
            branches_deleted: 5,
            worktrees_removed: 3,
            disk_space_reclaimed: 1024 * 1024 * 10, // 10 MB
        };

        assert_eq!(report.format_disk_space(), "10.00 MB");
    }

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(0), "0.00 B");
        assert_eq!(format_bytes(512), "512.00 B");
        assert_eq!(format_bytes(1024), "1.00 KB");
        assert_eq!(format_bytes(1024 * 1024), "1.00 MB");
        assert_eq!(format_bytes(1024 * 1024 * 1024), "1.00 GB");
    }
}
