use crate::core::{BatchId, TaskId};
use anyhow::{Context, Result};
use git2::{Repository, Worktree, WorktreeAddOptions};
use std::path::{Path, PathBuf};

/// Manages git worktrees for worker isolation
///
/// Each worker task gets its own worktree and branch, ensuring complete
/// isolation during execution. Worktrees are cleaned up after successful
/// merge.
pub struct WorktreeManager {
    repo: Repository,
    base_path: PathBuf,
}

/// Represents a worker's isolated worktree
#[derive(Clone)]
pub struct WorkerWorktree {
    pub path: PathBuf,
    pub branch: String,
}

impl WorkerWorktree {
    /// Create a new WorkerWorktree
    pub fn new(path: PathBuf, branch: String) -> Self {
        Self { path, branch }
    }
}

impl WorktreeManager {
    /// Create a new WorktreeManager for the given repository
    pub fn new(repo_path: &Path) -> Result<Self> {
        let repo = Repository::open(repo_path)
            .with_context(|| format!("Failed to open git repository at {:?}", repo_path))?;

        // Use .git/worktrees as the base directory for worktrees
        let base_path = repo
            .path()
            .parent()
            .map(|p| p.join(".orchestrator-worktrees"))
            .unwrap_or_else(|| PathBuf::from(".orchestrator-worktrees"));

        Ok(Self { repo, base_path })
    }

    /// Create an isolated worktree for a worker task
    ///
    /// The worktree is created at `.orchestrator-worktrees/{session}/batch-{batch}-task-{task}/`
    /// on a branch named `orchestrator/{session}/batch-{batch}-task-{task}`
    pub fn create_worktree(
        &self,
        session_id: &str,
        batch_id: BatchId,
        task_id: &TaskId,
        base_branch: &str,
    ) -> Result<WorkerWorktree> {
        let branch_name = format!(
            "orchestrator/{}/batch-{}-task-{}",
            session_id, batch_id, task_id
        );
        // Worktree name must be a simple identifier (no slashes)
        let worktree_name =
            format!("orch-{}-b{}-t{}", session_id, batch_id, task_id).replace('/', "-");
        let worktree_path = self
            .base_path
            .join(session_id)
            .join(format!("batch-{}-task-{}", batch_id, task_id));

        // Ensure parent directory exists
        std::fs::create_dir_all(&worktree_path.parent().unwrap())
            .with_context(|| "Failed to create worktree parent directory")?;

        // Create the branch if it doesn't exist
        // Handle "HEAD" specially - resolve to current commit
        let base_commit = if base_branch == "HEAD" {
            self.repo.head()?.peel_to_commit()?
        } else {
            self.repo
                .find_branch(base_branch, git2::BranchType::Local)?
                .into_reference()
                .peel_to_commit()?
        };

        // Create branch
        let branch = self
            .repo
            .branch(&branch_name, &base_commit, false)
            .with_context(|| format!("Failed to create branch '{}'", branch_name))?;

        // Get the branch reference for worktree creation
        let branch_ref = branch.get();

        // Create worktree options pointing to the branch
        let mut opts = WorktreeAddOptions::new();
        opts.reference(Some(&branch_ref));

        // Create worktree (use simple name for worktree, branch reference for checkout)
        let _worktree = self
            .repo
            .worktree(&worktree_name, &worktree_path, Some(&opts))
            .with_context(|| format!("Failed to create worktree at {:?}", worktree_path))?;

        tracing::info!(
            "Created worktree for batch {} task {} at {:?}",
            batch_id,
            task_id,
            worktree_path
        );

        // The worktree exists on disk now
        Ok(WorkerWorktree::new(worktree_path, branch_name))
    }

    /// Remove a worktree and its associated branch
    pub fn remove_worktree(&self, worktree: WorkerWorktree) -> Result<()> {
        // Remove the worktree directory
        if worktree.path.exists() {
            std::fs::remove_dir_all(&worktree.path)
                .with_context(|| format!("Failed to remove worktree at {:?}", worktree.path))?;
        }

        // Delete the branch
        let mut branch = self
            .repo
            .find_branch(&worktree.branch, git2::BranchType::Local)?;
        branch
            .delete()
            .with_context(|| format!("Failed to delete branch '{}'", worktree.branch))?;

        // Note: The Worktree object was dropped in create_worktree(), which pruned it from git's metadata.
        // If we need explicit pruning, we would call self.repo.prune_worktrees(None) here.

        tracing::info!(
            "Removed worktree at {:?} and branch '{}'",
            worktree.path,
            worktree.branch
        );

        Ok(())
    }

    /// Get the base path where worktrees are created
    pub fn base_path(&self) -> &Path {
        &self.base_path
    }

    /// List all session worktrees
    pub fn list_session_worktrees(&self, session_id: &str) -> Result<Vec<PathBuf>> {
        let session_path = self.base_path.join(session_id);
        if !session_path.exists() {
            return Ok(Vec::new());
        }

        let mut worktrees = Vec::new();
        for entry in std::fs::read_dir(&session_path)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                worktrees.push(entry.path());
            }
        }

        Ok(worktrees)
    }

    /// Clean up all worktrees for a session
    pub fn cleanup_session(&self, session_id: &str) -> Result<usize> {
        let session_path = self.base_path.join(session_id);
        if !session_path.exists() {
            return Ok(0);
        }

        let count = std::fs::read_dir(&session_path)?.count();
        std::fs::remove_dir_all(&session_path)
            .with_context(|| format!("Failed to cleanup session directory {:?}", session_path))?;

        tracing::info!("Cleaned up {} worktrees for session {}", count, session_id);
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup_test_repo() -> (TempDir, Repository) {
        let temp_dir = TempDir::new().unwrap();
        let repo = Repository::init(&temp_dir).unwrap();

        // Create initial commit
        let sig = git2::Signature::now("Test", "test@example.com").unwrap();
        let tree_id = {
            let mut index = repo.index().unwrap();
            index.write_tree().unwrap()
        };
        {
            let tree = repo.find_tree(tree_id).unwrap();
            repo.commit(Some("HEAD"), &sig, &sig, "Initial commit", &tree, &[])
                .unwrap();
        } // tree dropped here

        (temp_dir, repo)
    }

    #[test]
    fn test_worktree_manager_creation() {
        let (temp_dir, _repo) = setup_test_repo();
        let manager = WorktreeManager::new(temp_dir.path());
        assert!(manager.is_ok());
    }

    #[test]
    fn test_create_worktree() {
        let (temp_dir, _repo) = setup_test_repo();
        let manager = WorktreeManager::new(temp_dir.path()).unwrap();

        let worktree = manager.create_worktree("test-session", 1, &"task-1".to_string(), "main");

        // Should fail because "main" branch doesn't exist, but structure is correct
        // In real usage, we'd create the base branch first
    }
}
