use anyhow::{Context, Result};
use git2::{BranchType, Repository, Signature};
use std::path::Path;

/// Manages git branches for orchestrator sessions
pub struct BranchManager {
    repo: Repository,
}

impl BranchManager {
    /// Create a new BranchManager for the given repository path
    pub fn new(repo_path: &Path) -> Result<Self> {
        let repo = Repository::open(repo_path)
            .with_context(|| format!("Failed to open git repository at {:?}", repo_path))?;
        Ok(Self { repo })
    }

    /// Create a new branch from a base branch
    pub fn create_branch(&self, name: &str, base: &str) -> Result<()> {
        // Find the base branch
        let base_ref = self
            .repo
            .find_branch(base, BranchType::Local)?
            .into_reference();
        let base_commit = base_ref.peel_to_commit()?;

        // Create the new branch
        self.repo
            .branch(name, &base_commit, false)
            .with_context(|| format!("Failed to create branch '{}' from '{}'", name, base))?;

        tracing::info!("Created branch '{}' from '{}'", name, base);
        Ok(())
    }

    /// Create a new branch from a commit SHA
    pub fn create_branch_from_commit(&self, name: &str, commit_sha: &str) -> Result<()> {
        let oid = git2::Oid::from_str(commit_sha)?;
        let commit = self
            .repo
            .find_commit(oid)
            .with_context(|| format!("Failed to find commit {}", commit_sha))?;

        self.repo.branch(name, &commit, false).with_context(|| {
            format!(
                "Failed to create branch '{}' from commit {}",
                name, commit_sha
            )
        })?;

        tracing::info!("Created branch '{}' from commit {}", name, commit_sha);
        Ok(())
    }

    /// Delete a branch
    pub fn delete_branch(&self, name: &str) -> Result<()> {
        let mut branch = self
            .repo
            .find_branch(name, BranchType::Local)
            .with_context(|| format!("Branch '{}' not found", name))?;

        branch
            .delete()
            .with_context(|| format!("Failed to delete branch '{}'", name))?;

        tracing::info!("Deleted branch '{}'", name);
        Ok(())
    }

    /// Check if a branch exists
    pub fn branch_exists(&self, name: &str) -> bool {
        self.repo.find_branch(name, BranchType::Local).is_ok()
    }

    /// List all branches with the given prefix
    pub fn list_branches_with_prefix(&self, prefix: &str) -> Result<Vec<String>> {
        let mut branches = Vec::new();

        for branch in self.repo.branches(Some(BranchType::Local))? {
            let (branch, _) = branch?;
            if let Some(name) = branch.name()? {
                if name.starts_with(prefix) {
                    branches.push(name.to_string());
                }
            }
        }

        Ok(branches)
    }

    /// Get the current HEAD commit SHA
    pub fn head_sha(&self) -> Result<String> {
        let head = self.repo.head()?;
        let commit = head.peel_to_commit()?;
        Ok(commit.id().to_string())
    }

    /// Merge a branch into the current HEAD
    ///
    /// This performs a fast-forward merge if possible, or creates a merge commit.
    pub fn merge_branch(&self, branch_name: &str, message: &str) -> Result<()> {
        // Get the branch reference
        let branch = self.repo.find_branch(branch_name, BranchType::Local)?;
        let branch_ref = branch.get();
        let branch_commit = branch_ref.peel_to_commit()?;

        // Get HEAD
        let head = self.repo.head()?;
        let head_commit = head.peel_to_commit()?;

        // Check if we can fast-forward
        if self
            .repo
            .graph_descendant_of(head_commit.id(), branch_commit.id())?
        {
            // Already merged
            tracing::info!("Branch '{}' is already merged into HEAD", branch_name);
            return Ok(());
        }

        if self
            .repo
            .graph_descendant_of(branch_commit.id(), head_commit.id())?
        {
            // Fast-forward possible
            let mut head_ref = self.repo.head()?;
            head_ref
                .set_target(branch_commit.id(), "Fast-forward merge")
                .with_context(|| "Failed to fast-forward HEAD")?;
            tracing::info!("Fast-forwarded HEAD to branch '{}'", branch_name);
        } else {
            // Need to create a merge commit
            self.create_merge_commit(&head_commit, &branch_commit, message)?;
            tracing::info!("Created merge commit for branch '{}'", branch_name);
        }

        Ok(())
    }

    fn create_merge_commit(
        &self,
        head_commit: &git2::Commit,
        branch_commit: &git2::Commit,
        message: &str,
    ) -> Result<()> {
        // Create signature
        let sig = self.repo.signature()?;

        // Create merge commit
        let mut parents = [head_commit, branch_commit];
        let tree = self.find_merge_tree(head_commit, branch_commit)?;

        let merge_commit = self
            .repo
            .commit(Some("HEAD"), &sig, &sig, message, &tree, &parents)
            .with_context(|| "Failed to create merge commit")?;

        tracing::info!("Created merge commit {}", merge_commit);
        Ok(())
    }

    fn find_merge_tree(
        &self,
        commit1: &git2::Commit,
        commit2: &git2::Commit,
    ) -> Result<git2::Tree> {
        // Get trees
        let tree1 = commit1.tree()?;
        let tree2 = commit2.tree()?;

        // Try to merge trees
        // Find merge base for the two commits
        let merge_base_oid = self.repo.merge_base(commit1.id(), commit2.id())?;
        let merge_base_commit = self.repo.find_commit(merge_base_oid)?;
        let ancestor_tree = merge_base_commit.tree()?;

        let mut index = self
            .repo
            .merge_trees(&ancestor_tree, &tree1, &tree2, None)?;

        if index.has_conflicts() {
            return Err(anyhow::anyhow!("Merge conflict detected"));
        }

        let tree_id = index.write_tree_to(&self.repo)?;
        let tree = self.repo.find_tree(tree_id)?;

        Ok(tree)
    }

    /// Checkout a branch
    pub fn checkout_branch(&self, branch_name: &str) -> Result<()> {
        let branch = self.repo.find_branch(branch_name, BranchType::Local)?;
        let commit = branch.get().peel_to_commit()?;

        // Checkout the tree
        let tree = commit.tree()?;
        self.repo
            .checkout_tree(tree.as_object(), None)
            .with_context(|| format!("Failed to checkout branch '{}'", branch_name))?;

        // Set HEAD to the branch
        self.repo
            .set_head(branch.get().name().unwrap_or(branch_name))
            .with_context(|| format!("Failed to set HEAD to branch '{}'", branch_name))?;

        tracing::info!("Checked out branch '{}'", branch_name);
        Ok(())
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
        let sig = Signature::now("Test", "test@example.com").unwrap();
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
    fn test_branch_manager_creation() {
        let (temp_dir, _repo) = setup_test_repo();
        let manager = BranchManager::new(temp_dir.path());
        assert!(manager.is_ok());
    }
}
