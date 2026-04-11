use crate::core::{BatchId, BatchStatus, ExecutionPlan, PlanStatus};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Hybrid cancellation token
/// 
/// This provides cooperative cancellation that allows tasks to complete
/// their current work before stopping. It's used to gracefully handle
/// user cancellation requests during execution.
#[derive(Debug, Clone)]
pub struct CancellationToken {
    cancelled: Arc<AtomicBool>,
}

impl CancellationToken {
    /// Create a new cancellation token
    pub fn new() -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Cancel the operation
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
        tracing::info!("Cancellation requested");
    }

    /// Check if cancellation has been requested
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }

    /// Check cancellation and return error if cancelled
    pub fn check(&self) -> Result<(), CancellationError> {
        if self.is_cancelled() {
            Err(CancellationError::Cancelled)
        } else {
            Ok(())
        }
    }

    /// Run a closure if not cancelled
    pub fn if_not_cancelled<F, T>(&self, f: F) -> Option<T>
    where
        F: FnOnce() -> T,
    {
        if self.is_cancelled() {
            None
        } else {
            Some(f())
        }
    }
}

impl Default for CancellationToken {
    fn default() -> Self {
        Self::new()
    }
}

/// Cancellation error types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CancellationError {
    Cancelled,
}

impl std::fmt::Display for CancellationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Operation was cancelled")
    }
}

impl std::error::Error for CancellationError {}

/// Checkpoint for resuming execution
/// 
/// A checkpoint captures the state of execution at a specific point,
/// allowing the orchestrator to resume from that point after cancellation
/// or failure.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Checkpoint {
    /// Session ID
    pub session_id: String,
    /// Plan ID
    pub plan_id: String,
    /// Last completed batch ID
    pub last_completed_batch: Option<BatchId>,
    /// Current batch being processed
    pub current_batch: Option<BatchId>,
    /// Completed batches and their merge commits
    pub completed_batches: HashMap<BatchId, String>,
    /// Timestamp when checkpoint was created
    pub timestamp: chrono::DateTime<chrono::Local>,
    /// Status of the plan when checkpoint was created
    pub status: PlanStatus,
}

impl Checkpoint {
    /// Create a new checkpoint
    pub fn new(session_id: String, plan_id: String) -> Self {
        Self {
            session_id,
            plan_id,
            last_completed_batch: None,
            current_batch: None,
            completed_batches: HashMap::new(),
            timestamp: chrono::Local::now(),
            status: PlanStatus::InProgress,
        }
    }

    /// Mark a batch as completed
    pub fn mark_batch_completed(&mut self, batch_id: BatchId, merge_commit: String) {
        self.completed_batches.insert(batch_id, merge_commit.clone());
        self.last_completed_batch = Some(batch_id);
        self.current_batch = None;
    }

    /// Set the current batch being processed
    pub fn set_current_batch(&mut self, batch_id: BatchId) {
        self.current_batch = Some(batch_id);
    }

    /// Save checkpoint to disk
    pub async fn save(&self, checkpoint_dir: &Path) -> anyhow::Result<PathBuf> {
        tokio::fs::create_dir_all(checkpoint_dir).await?;

        let filename = format!("checkpoint-{}-{}.json", self.session_id, self.timestamp.format("%Y%m%d-%H%M%S"));
        let path = checkpoint_dir.join(&filename);

        let json = serde_json::to_string_pretty(self)?;
        tokio::fs::write(&path, json).await?;

        tracing::info!("Checkpoint saved to {:?}", path);
        Ok(path)
    }

    /// Load checkpoint from disk
    pub async fn load(path: &Path) -> anyhow::Result<Self> {
        let json = tokio::fs::read_to_string(path).await?;
        let checkpoint: Checkpoint = serde_json::from_str(&json)?;
        Ok(checkpoint)
    }

    /// Find the latest checkpoint for a session
    pub async fn find_latest(checkpoint_dir: &Path, session_id: &str) -> anyhow::Result<Option<Self>> {
        let mut latest: Option<Checkpoint> = None;

        if !checkpoint_dir.exists() {
            return Ok(None);
        }

        let mut entries = tokio::fs::read_dir(checkpoint_dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.extension().map_or(false, |e| e == "json") {
                if let Ok(checkpoint) = Self::load(&path).await {
                    if checkpoint.session_id == session_id {
                        if latest.as_ref().map_or(true, |l| checkpoint.timestamp > l.timestamp) {
                            latest = Some(checkpoint);
                        }
                    }
                }
            }
        }

        Ok(latest)
    }
}

/// Manages checkpoints for the orchestrator
pub struct CheckpointManager {
    checkpoint_dir: PathBuf,
}

impl CheckpointManager {
    /// Create a new checkpoint manager
    pub fn new(checkpoint_dir: PathBuf) -> Self {
        Self { checkpoint_dir }
    }

    /// Create a checkpoint for the current execution state
    pub async fn create_checkpoint(&self, plan: &ExecutionPlan, session_id: &str) -> anyhow::Result<Checkpoint> {
        let mut checkpoint = Checkpoint::new(session_id.to_string(), plan.id.clone());

        // Find the last completed batch
        for batch in &plan.batches {
            if batch.status == Some(BatchStatus::Completed) {
                checkpoint.mark_batch_completed(
                    batch.id,
                    format!("orchestrator/{}/batch-{}-merged", session_id, batch.id),
                );
            } else if batch.status == Some(BatchStatus::InProgress) {
                checkpoint.set_current_batch(batch.id);
                break;
            }
        }

        checkpoint.status = plan.status;

        // Save checkpoint
        checkpoint.save(&self.checkpoint_dir).await?;

        Ok(checkpoint)
    }

    /// Resume execution from a checkpoint
    pub async fn resume_from_checkpoint(
        &self,
        session_id: &str,
    ) -> anyhow::Result<Option<(Checkpoint, ExecutionPlan)>> {
        let checkpoint = match Checkpoint::find_latest(&self.checkpoint_dir, session_id).await? {
            Some(cp) => cp,
            None => return Ok(None),
        };

        // Load the plan from the checkpoint directory
        let plan_path = self.checkpoint_dir.join(format!("plan-{}.json", checkpoint.plan_id));
        let plan = if plan_path.exists() {
            let json = tokio::fs::read_to_string(&plan_path).await?;
            serde_json::from_str(&json)?
        } else {
            return Err(anyhow::anyhow!("Plan file not found: {:?}", plan_path));
        };

        Ok(Some((checkpoint, plan)))
    }

    /// Save a plan for future resumption
    pub async fn save_plan(&self, plan: &ExecutionPlan) -> anyhow::Result<PathBuf> {
        tokio::fs::create_dir_all(&self.checkpoint_dir).await?;

        let path = self.checkpoint_dir.join(format!("plan-{}.json", plan.id));
        let json = serde_json::to_string_pretty(plan)?;
        tokio::fs::write(&path, json).await?;

        Ok(path)
    }

    /// List all checkpoints
    pub async fn list_checkpoints(&self) -> anyhow::Result<Vec<Checkpoint>> {
        let mut checkpoints = Vec::new();

        if !self.checkpoint_dir.exists() {
            return Ok(checkpoints);
        }

        let mut entries = tokio::fs::read_dir(&self.checkpoint_dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.extension().map_or(false, |e| e == "json") {
                let filename = path.file_name().unwrap_or_default().to_string_lossy();
                // Skip plan files, only load checkpoint files
                if filename.starts_with("plan-") {
                    continue;
                }
                if let Ok(checkpoint) = Checkpoint::load(&path).await {
                    checkpoints.push(checkpoint);
                }
            }
        }

        // Sort by timestamp descending
        checkpoints.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

        Ok(checkpoints)
    }

    /// Clean up old checkpoints (keep only the most recent N)
    pub async fn cleanup_old_checkpoints(&self, keep_count: usize) -> anyhow::Result<usize> {
        let mut checkpoints = self.list_checkpoints().await?;

        if checkpoints.len() <= keep_count {
            return Ok(0);
        }

        // Remove older checkpoints
        let to_remove = checkpoints.split_off(keep_count);
        let mut removed = 0;

        for checkpoint in to_remove {
            let pattern = format!("checkpoint-{}-*.json", checkpoint.session_id);
            let mut entries = tokio::fs::read_dir(&self.checkpoint_dir).await?;

            while let Some(entry) = entries.next_entry().await? {
                let path = entry.path();
                let filename = path.file_name().unwrap_or_default().to_string_lossy();
                if filename.starts_with(&format!("checkpoint-{}-", checkpoint.session_id)) {
                    tokio::fs::remove_file(&path).await?;
                    removed += 1;
                }
            }
        }

        Ok(removed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_cancellation_token() {
        let token = CancellationToken::new();
        assert!(!token.is_cancelled());

        token.cancel();
        assert!(token.is_cancelled());
        assert!(token.check().is_err());
    }

    #[test]
    fn test_checkpoint_creation() {
        let mut checkpoint = Checkpoint::new(
            "test-session".to_string(),
            "test-plan".to_string(),
        );

        assert!(checkpoint.last_completed_batch.is_none());

        checkpoint.mark_batch_completed(1, "abc123".to_string());
        assert_eq!(checkpoint.last_completed_batch, Some(1));
        assert_eq!(checkpoint.completed_batches.get(&1), Some(&"abc123".to_string()));
    }

    #[tokio::test]
    async fn test_checkpoint_save_load() {
        let temp_dir = TempDir::new().unwrap();
        let checkpoint_dir = temp_dir.path().join("checkpoints");

        let checkpoint = Checkpoint::new(
            "test-session".to_string(),
            "test-plan".to_string(),
        );

        let path = checkpoint.save(&checkpoint_dir).await.unwrap();
        assert!(path.exists());

        let loaded = Checkpoint::load(&path).await.unwrap();
        assert_eq!(loaded.session_id, checkpoint.session_id);
        assert_eq!(loaded.plan_id, checkpoint.plan_id);
    }

    #[tokio::test]
    async fn test_checkpoint_find_latest() {
        let temp_dir = TempDir::new().unwrap();
        let checkpoint_dir = temp_dir.path().join("checkpoints");

        // Create old checkpoint
        let old = Checkpoint::new("session-1".to_string(), "plan-1".to_string());
        old.save(&checkpoint_dir).await.unwrap();

        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        // Create new checkpoint
        let new = Checkpoint::new("session-1".to_string(), "plan-2".to_string());
        new.save(&checkpoint_dir).await.unwrap();

        let latest = Checkpoint::find_latest(&checkpoint_dir, "session-1").await.unwrap();
        assert!(latest.is_some());
        assert_eq!(latest.unwrap().plan_id, "plan-2");
    }
}
