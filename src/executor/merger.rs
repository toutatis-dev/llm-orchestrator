use crate::core::{BatchId, TaskBatch};
use crate::executor::executor::TaskResult;
use anyhow::{Context, Result};
use git2::{BranchType, Repository};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Result of a merge operation
#[derive(Debug, Clone)]
pub enum MergeResult {
    /// Merge succeeded
    Success { commit_sha: String },
    /// Merge conflict detected
    Conflict {
        files: Vec<PathBuf>,
        message: String,
    },
    /// Merge failed for other reasons
    Error { message: String },
}

/// Manages merging of batch results into the main branch
///
/// The BatchMerger handles sequential merging of task branches. Due to the
/// file isolation constraint (no two tasks in a batch touch the same file),
/// conflicts should be rare. When they do occur, we hand off to the user
/// with detailed diagnostics.
pub struct BatchMerger {
    repo: Repository,
    conflict_resolution_dir: PathBuf,
}

impl BatchMerger {
    /// Create a new BatchMerger for the given repository
    pub fn new(repo_path: &Path) -> Result<Self> {
        let repo = Repository::open(repo_path)
            .with_context(|| format!("Failed to open repository at {:?}", repo_path))?;

        let conflict_resolution_dir = repo_path.join(".orchestrator").join("conflicts");

        Ok(Self {
            repo,
            conflict_resolution_dir,
        })
    }

    /// Merge all task results from a batch into a single merge branch
    ///
    /// This performs sequential merges of each task branch into the merge branch.
    /// If all tasks properly isolated their files (as enforced by plan validation),
    /// these merges should be trivial.
    pub fn merge_batch(
        &self,
        batch: &TaskBatch,
        results: &[TaskResult],
        base_branch: &str,
        session_id: &str,
    ) -> Result<MergeResult> {
        let merge_branch = format!("orchestrator/{}/batch-{}-merged", session_id, batch.id);

        // Create the merge branch from base
        self.create_merge_branch(&merge_branch, base_branch)?;

        // Track successfully merged commits
        let mut merged_commits = Vec::new();

        // Merge each task branch sequentially
        for result in results {
            if !result.success {
                tracing::warn!("Skipping failed task {} in merge", result.task_id);
                continue;
            }

            let task_branch = format!(
                "orchestrator/{}/batch-{}-task-{}",
                session_id, batch.id, result.task_id
            );

            match self.merge_task_branch(&task_branch, &merge_branch, result) {
                Ok(commit_sha) => {
                    merged_commits.push(commit_sha);
                }
                Err(e) => {
                    let error_msg = format!(
                        "Merge conflict in batch {} for task {}: {}",
                        batch.id, result.task_id, e
                    );
                    tracing::error!("{}", error_msg);

                    // Detect conflict files
                    let conflict_files = self.detect_conflict_files(&task_branch, &merge_branch);

                    return Ok(MergeResult::Conflict {
                        files: conflict_files,
                        message: error_msg,
                    });
                }
            }
        }

        // Get final commit SHA
        let merge_commit = self.get_branch_commit(&merge_branch)?;

        tracing::info!(
            "Successfully merged batch {} into {} ({} tasks)",
            batch.id,
            merge_branch,
            merged_commits.len()
        );

        Ok(MergeResult::Success {
            commit_sha: merge_commit,
        })
    }

    /// Create a merge branch from a base branch
    fn create_merge_branch(&self, merge_branch: &str, base_branch: &str) -> Result<()> {
        // Find base branch
        let base_ref = self
            .repo
            .find_branch(base_branch, BranchType::Local)?
            .into_reference();
        let base_commit = base_ref.peel_to_commit()?;

        // Create merge branch
        self.repo
            .branch(merge_branch, &base_commit, false)
            .with_context(|| format!("Failed to create merge branch '{}'", merge_branch))?;

        tracing::debug!(
            "Created merge branch '{}' from '{}'",
            merge_branch,
            base_branch
        );
        Ok(())
    }

    /// Merge a single task branch into the merge branch
    fn merge_task_branch(
        &self,
        task_branch: &str,
        merge_branch: &str,
        result: &TaskResult,
    ) -> Result<String> {
        // Get the task branch commit
        let task_ref = self
            .repo
            .find_branch(task_branch, BranchType::Local)?
            .into_reference();
        let task_commit = task_ref.peel_to_commit()?;

        // Get the merge branch commit
        let merge_ref = self
            .repo
            .find_branch(merge_branch, BranchType::Local)?
            .into_reference();
        let merge_commit = merge_ref.peel_to_commit()?;

        // Check if we can fast-forward
        if self.is_fast_forward(&merge_commit, &task_commit)? {
            // Fast-forward merge
            let mut branch_ref = self
                .repo
                .find_branch(merge_branch, BranchType::Local)?
                .into_reference();
            branch_ref
                .set_target(
                    task_commit.id(),
                    &format!("Merge task {} (fast-forward)", result.task_id),
                )
                .with_context(|| "Failed to fast-forward merge branch")?;

            tracing::debug!(
                "Fast-forward merged task {} into {}",
                result.task_id,
                merge_branch
            );
            Ok(task_commit.id().to_string())
        } else {
            // Create a merge commit
            self.create_merge_commit(&merge_commit, &task_commit, merge_branch, result)
        }
    }

    /// Check if fast-forward is possible
    fn is_fast_forward(&self, base: &git2::Commit, target: &git2::Commit) -> Result<bool> {
        Ok(self.repo.graph_descendant_of(target.id(), base.id())?)
    }

    /// Create a merge commit
    fn create_merge_commit(
        &self,
        base_commit: &git2::Commit,
        task_commit: &git2::Commit,
        merge_branch: &str,
        result: &TaskResult,
    ) -> Result<String> {
        // Find merge base
        let merge_base_oid = self.repo.merge_base(base_commit.id(), task_commit.id())?;
        let merge_base = self.repo.find_commit(merge_base_oid)?;

        // Get trees
        let ancestor_tree = merge_base.tree()?;
        let base_tree = base_commit.tree()?;
        let task_tree = task_commit.tree()?;

        // Perform merge
        let mut index = self
            .repo
            .merge_trees(&ancestor_tree, &base_tree, &task_tree, None)?;

        // Check for conflicts
        if index.has_conflicts() {
            return Err(anyhow::anyhow!(
                "Merge conflict detected between {} and task {}",
                merge_branch,
                result.task_id
            ));
        }

        // Write tree
        let tree_id = index.write_tree_to(&self.repo)?;
        let tree = self.repo.find_tree(tree_id)?;

        // Create commit
        let sig = self.repo.signature()?;
        let message = format!(
            "[orchestrator] Merge task {}\n\nTokens: {} in / {} out",
            result.task_id, result.tokens_used.input, result.tokens_used.output
        );

        let parents = [base_commit, task_commit];
        let commit_id = self.repo.commit(
            Some(&format!("refs/heads/{}", merge_branch)),
            &sig,
            &sig,
            &message,
            &tree,
            &parents,
        )?;

        tracing::debug!(
            "Created merge commit {} for task {} into {}",
            commit_id,
            result.task_id,
            merge_branch
        );

        Ok(commit_id.to_string())
    }

    /// Detect which files have conflicts between two branches
    fn detect_conflict_files(&self, branch1: &str, branch2: &str) -> Vec<PathBuf> {
        let mut conflict_files = Vec::new();

        // Try to get the merge trees and find conflicts
        if let (Ok(commit1), Ok(commit2)) = (
            self.get_branch_commit_sha(branch1),
            self.get_branch_commit_sha(branch2),
        ) {
            if let (Ok(commit1), Ok(commit2)) = (
                self.repo.find_commit(commit1),
                self.repo.find_commit(commit2),
            ) {
                if let Ok(merge_base_oid) = self.repo.merge_base(commit1.id(), commit2.id()) {
                    if let Ok(merge_base) = self.repo.find_commit(merge_base_oid) {
                        if let (Ok(ancestor_tree), Ok(tree1), Ok(tree2)) =
                            (merge_base.tree(), commit1.tree(), commit2.tree())
                        {
                            if let Ok(mut index) =
                                self.repo.merge_trees(&ancestor_tree, &tree1, &tree2, None)
                            {
                                // Iterate over conflicts
                                if let Ok(conflicts) = index.conflicts() {
                                    for conflict in conflicts {
                                        if let Ok(conflict) = conflict {
                                            if let Some(ours) = conflict.our {
                                                let path_str = String::from_utf8_lossy(&ours.path);
                                                conflict_files
                                                    .push(PathBuf::from(path_str.as_ref()));
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        conflict_files
    }

    /// Get the commit SHA for a branch
    fn get_branch_commit_sha(&self, branch: &str) -> Result<git2::Oid> {
        let branch_ref = self
            .repo
            .find_branch(branch, BranchType::Local)?
            .into_reference();
        Ok(branch_ref.target().unwrap())
    }

    /// Get the commit SHA for a branch as string
    fn get_branch_commit(&self, branch: &str) -> Result<String> {
        let oid = self.get_branch_commit_sha(branch)?;
        Ok(oid.to_string())
    }

    /// Handle a merge conflict by generating conflict resolution instructions
    ///
    /// This creates a detailed report for the user to resolve manually.
    pub fn handle_conflict(
        &self,
        batch_id: BatchId,
        conflict_files: &[PathBuf],
        task_results: &[TaskResult],
        session_id: &str,
    ) -> Result<ConflictResolutionGuide> {
        // Ensure conflict directory exists
        std::fs::create_dir_all(&self.conflict_resolution_dir)?;

        // Generate conflict report
        let timestamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
        let report_path = self
            .conflict_resolution_dir
            .join(format!("{}-batch-{}-conflict.md", timestamp, batch_id));

        let report =
            self.generate_conflict_report(batch_id, conflict_files, task_results, session_id);

        std::fs::write(&report_path, report)?;

        tracing::info!("Conflict resolution guide written to {:?}", report_path);

        Ok(ConflictResolutionGuide {
            batch_id,
            conflict_files: conflict_files.to_vec(),
            report_path,
            session_id: session_id.to_string(),
        })
    }

    fn generate_conflict_report(
        &self,
        batch_id: BatchId,
        conflict_files: &[PathBuf],
        task_results: &[TaskResult],
        session_id: &str,
    ) -> String {
        let mut report = String::new();

        report.push_str("# Merge Conflict Resolution Guide\n\n");
        report.push_str(&format!("**Batch:** {}\n", batch_id));
        report.push_str(&format!("**Session:** {}\n", session_id));
        report.push_str(&format!(
            "**Generated:** {}\n\n",
            chrono::Local::now().format("%Y-%m-%d %H:%M:%S")
        ));

        report.push_str("## Conflicting Files\n\n");
        for file in conflict_files {
            report.push_str(&format!("- `{}`\n", file.display()));
        }

        report.push_str("\n## Affected Tasks\n\n");
        for result in task_results {
            report.push_str(&format!(
                "- **{}**: {} ({} files written)\n",
                result.task_id,
                if result.success {
                    "✓ success"
                } else {
                    "✗ failed"
                },
                result.files_written.len()
            ));
        }

        report.push_str("\n## Resolution Steps\n\n");
        report.push_str("1. Review the conflicting files listed above\n");
        report.push_str("2. Manually merge the changes from each task branch\n");
        report.push_str("3. Commit the resolved files\n");
        report.push_str(&format!(
            "4. Resume the orchestrator with: `orchestrator resume --session-id {}`\n\n",
            session_id
        ));

        report.push_str("## Task Branches\n\n");
        for result in task_results {
            if result.success {
                let branch = format!(
                    "orchestrator/{}/batch-{}-task-{}",
                    session_id, batch_id, result.task_id
                );
                report.push_str(&format!("- `{}`\n", branch));
            }
        }

        report.push_str("\n---\n\n");
        report.push_str("**Note:** This conflict should be rare. The orchestrator validates ");
        report.push_str("that no two tasks in a batch modify the same file. If you see this, ");
        report.push_str("the validation may have missed an edge case.\n");

        report
    }
}

/// Guide for resolving a merge conflict
#[derive(Debug, Clone)]
pub struct ConflictResolutionGuide {
    pub batch_id: BatchId,
    pub conflict_files: Vec<PathBuf>,
    pub report_path: PathBuf,
    pub session_id: String,
}

impl ConflictResolutionGuide {
    /// Print conflict resolution instructions to stdout
    pub fn print_instructions(&self) {
        println!("╔════════════════════════════════════════════════════════════╗");
        println!("║                   MERGE CONFLICT                           ║");
        println!("╠════════════════════════════════════════════════════════════╣");
        println!("║  Batch:            {:<40} ║", self.batch_id);
        println!("║  Session:          {:<40} ║", self.session_id);
        println!("╚════════════════════════════════════════════════════════════╝");
        println!();
        println!("The following files have conflicting changes:");
        for file in &self.conflict_files {
            println!("  • {}", file.display());
        }
        println!();
        println!("───────────────────────────────────────────────────────────");
        println!("A detailed resolution guide has been saved to:");
        println!("  {}", self.report_path.display());
        println!();
        println!("To resolve manually:");
        println!("  1. Review the conflicting files");
        println!("  2. Merge the changes from each task branch");
        println!("  3. Commit the resolved files");
        println!(
            "  4. Resume with: orchestrator resume --session-id {}",
            self.session_id
        );
        println!("───────────────────────────────────────────────────────────");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup_test_repo() -> TempDir {
        let temp_dir = TempDir::new().unwrap();
        let repo = git2::Repository::init(&temp_dir).unwrap();

        // Configure git user
        let mut config = repo.config().unwrap();
        config.set_str("user.name", "Test").unwrap();
        config.set_str("user.email", "test@example.com").unwrap();

        // Create initial commit
        let sig = git2::Signature::now("Test", "test@example.com").unwrap();
        let tree_id = repo.index().unwrap().write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "Initial commit", &tree, &[])
            .unwrap();

        // Rename to main
        let head = repo.head().unwrap();
        let commit = head.peel_to_commit().unwrap();
        repo.branch("main", &commit, false).unwrap();
        repo.set_head("refs/heads/main").unwrap();

        temp_dir
    }

    #[test]
    fn test_batch_merger_creation() {
        let temp_dir = setup_test_repo();
        let merger = BatchMerger::new(temp_dir.path());
        assert!(merger.is_ok());
    }
}
