use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Unique identifier for a task
pub type TaskId = String;
/// Unique identifier for a batch
pub type BatchId = usize;

/// A single unit of work assigned to a worker
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: TaskId,
    pub description: String,
    pub task_type: TaskType,
    pub tier: WorkerTier,
    pub inputs: Vec<PathBuf>,
    pub expected_outputs: Vec<PathBuf>,
    pub context: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum TaskType {
    Generation,
    Refactor,
    Documentation,
    Test,
    Analysis,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum WorkerTier {
    Simple,
    Medium,
    Complex,
}

impl WorkerTier {
    pub fn model_name(&self) -> &'static str {
        match self {
            WorkerTier::Simple => "qwen/qwen3.5-4b",
            WorkerTier::Medium => "qwen/qwen3.5-9b",
            WorkerTier::Complex => "qwen/qwen3.5-32b",
        }
    }

    pub fn context_window(&self) -> usize {
        match self {
            WorkerTier::Simple => 32768,
            WorkerTier::Medium => 65536,
            WorkerTier::Complex => 65536,
        }
    }

    pub fn max_tokens(&self) -> usize {
        match self {
            WorkerTier::Simple => 4096,
            WorkerTier::Medium => 8192,
            WorkerTier::Complex => 8192,
        }
    }

    pub fn next_tier(&self) -> Option<Self> {
        match self {
            WorkerTier::Simple => Some(WorkerTier::Medium),
            WorkerTier::Medium => Some(WorkerTier::Complex),
            WorkerTier::Complex => None,
        }
    }
}
