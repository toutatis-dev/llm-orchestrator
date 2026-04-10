//! LLM Orchestrator
//! 
//! An interactive TUI-based orchestrator that uses a frontier model (Kimi K2.5) 
//! for planning and distributes work to smaller models (Qwen 3.5 series) 
//! to parallelize tasks and improve efficiency.

pub mod cli;
pub mod config;
pub mod core;
pub mod executor;
pub mod git;
pub mod models;
pub mod orchestrator;
pub mod tui;
pub mod watcher;

pub use config::Config;
pub use core::{ExecutionPlan, Task, TaskBatch};

use std::path::Path;

/// Initialize the orchestrator
pub async fn init() -> anyhow::Result<()> {
    // Ensure .orchestrator directory exists
    let orchestrator_dir = Path::new(".orchestrator");
    if !orchestrator_dir.exists() {
        tokio::fs::create_dir_all(orchestrator_dir).await?;
        tokio::fs::create_dir_all(orchestrator_dir.join("plans")).await?;
        tokio::fs::create_dir_all(orchestrator_dir.join("rejected-plans")).await?;
    }
    
    Ok(())
}