# LLM Orchestrator - Implementation Plan
**Version:** 1.2 (Final)  
**Last Updated:** 2024-04-10  
**Estimated Duration:** 26 days (14 days Phase 1, 12 days Phase 2)
---
## Architecture Overview
┌─────────────────────────────────────────────────────────────┐
│                      TUI Application                        │
│  ┌─────────────────────────────────────────────────────────┐│
│  │                    Event Loop                            ││
│  │         ┌──────────┐  ┌──────────┐  ┌──────────┐       ││
│  │         │  Input   │  │  Update  │  │  Render  │       ││
│  │         │ Handler  │  │ Handler  │  │ Handler  │       ││
│  │         └────┬─────┘  └────┬─────┘  └────┬─────┘       ││
│  │              │             │             │              ││
│  │              ▼             ▼             ▼              ││
│  │  ┌─────────────────────────────────────────────────┐   ││
│  │  │              App State                          │   ││
│  │  │  • SessionState (Discovery/Planning/Executing)  │   ││
│  │  │  • ChatHistory                                  │   ││
│  │  │  • ExecutionPlan                                │   ││
│  │  │  • CancellationToken                            │   ││
│  │  │  • CostTracker                                  │   ││
│  │  └─────────────────────────────────────────────────┘   ││
│  └─────────────────────────────────────────────────────────┘│
└───────────────────────┬─────────────────────────────────────┘
                        │
        ┌───────────────┴───────────────┐
        ▼                               ▼
┌───────────────┐               ┌───────────────────────────┐
│ Orchestrator  │               │       Executor            │
│ (Kimi K2.5)   │               │  ┌─────────────────────┐  │
│ Streaming SSE │               │  │ Worktree per Worker │  │
└───────┬───────┘               │  │ Executor owns git   │  │
        │                       │  │ Workers stateless   │  │
        │ Plan (validated)      │  └─────────────────────┘  │
        ▼                       └───────────────────────────┘
┌─────────────────────────────────────────────────────────────┐
│                    Git Repository                            │
│                                                              │
│  master                                                      │
│    │                                                         │
│    ├── session-{id}/planning                                 │
│    │                                                         │
│    ├── session-{id}/batch-1-task-1  (Worker 1 - deleted)    │
│    ├── session-{id}/batch-1-task-2  (Worker 2 - deleted)    │
│    ├── session-{id}/batch-1-merged  (deleted on success)    │
│    │                                                         │
│    ├── session-{id}/batch-2-task-1  (Worker 3 - deleted)    │
│    ...                                                       │
│    │                                                         │
│    └── session-{id}/final  (merged to master, deleted)      │
│                                                              │
│  (.orchestrator/rejected-plans/ for failed validations)      │
└─────────────────────────────────────────────────────────────┘
**Key Constraints:**
- Orchestrator ensures: no two tasks in a batch touch the same file
- Executor owns all git operations (workers are stateless)
- Workers return JSON content, executor writes files and commits
- Session branches auto-deleted on success, kept on failure for forensics
---
## Model Configuration
### Orchestrator (Planning)
- **Model**: `moonshotai/kimi-k2.5`
- **Provider**: OpenRouter
- **Context**: 200,000 tokens
- **Temperature**: 0.1
- **Streaming**: Yes (SSE, buffered lines/paragraphs)
### Workers (Execution) - Blocking API
| Tier | Model | Context | Max Tokens |
|------|-------|---------|------------|
| Simple | `qwen/qwen3.5-4b` | 32,768 | 4,096 |
| Medium | `qwen/qwen3.5-9b` | 65,536 | 8,192 |
| Complex | `qwen/qwen3.5-32b` | 65,536 | 8,192 |
---
## Project Structure
llm-orchestrator/
├── Cargo.toml
├── .gitignore
├── .orchestrator/                  # Project-local (gitignored)
│   ├── config.toml
│   ├── plans/
│   │   └── plan-{timestamp}.md
│   ├── current-plan.md
│   └── rejected-plans/             # Failed plan validations
│       └── {timestamp}-attempt-{N}.md
├── src/
│   ├── main.rs
│   ├── lib.rs
│   ├── cli/
│   │   └── mod.rs
│   ├── tui/
│   │   ├── mod.rs
│   │   ├── app.rs                  # App state machine
│   │   ├── events.rs
│   │   ├── layout.rs
│   │   └── components/
│   │       ├── mod.rs
│   │       ├── chat.rs             # Streaming chat
│   │       ├── plan.rs
│   │       ├── progress.rs
│   │       ├── log.rs
│   │       └── wizard.rs
│   ├── core/
│   │   ├── mod.rs
│   │   ├── task.rs
│   │   ├── plan.rs
│   │   ├── message.rs
│   │   ├── cost.rs
│   │   └── error.rs
│   ├── orchestrator/
│   │   ├── mod.rs
│   │   ├── client.rs               # Streaming SSE client
│   │   ├── planner.rs
│   │   └── validator.rs            # Plan validation + retry
│   ├── executor/
│   │   ├── mod.rs
│   │   ├── executor.rs
│   │   ├── worktree.rs             # Git worktree management
│   │   ├── merger.rs               # Batch merging
│   │   └── progress.rs
│   ├── models/
│   │   ├── mod.rs
│   │   ├── openrouter.rs
│   │   └── types.rs
│   ├── context/
│   │   ├── mod.rs
│   │   └── simple.rs
│   ├── watcher/
│   │   ├── mod.rs
│   │   └── handler.rs
│   ├── git/
│   │   ├── mod.rs
│   │   ├── repo.rs
│   │   ├── worktree.rs
│   │   └── cleanup.rs              # Branch cleanup
│   ├── config.rs
│   ├── cancellation.rs
│   ├── rate_limit.rs               # Exponential backoff + jitter
│   └── api_key.rs
└── PLAN.md
---
## Dependencies
```toml
[package]
name = "llm-orchestrator"
version = "0.1.0"
edition = "2024"
[dependencies]
# Async
tokio = { version = "1.35", features = ["full"] }
tokio-util = "0.7"
async-trait = "0.1"
# TUI
ratatui = "0.29"
crossterm = "0.28"
# HTTP + SSE
reqwest = { version = "0.12", features = ["json", "rustls-tls", "stream"] }
reqwest-retry = "0.7"
reqwest-middleware = "0.4"
eventsource-stream = "0.2"
futures = "0.3"
# Serialization
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
toml = "0.8"
# Error handling
thiserror = "1.0"
anyhow = "1.0"
# Config + paths
dirs = "5.0"
# Time
chrono = { version = "0.4", features = ["serde"] }
# Tracing
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
# UUID
uuid = { version = "1.6", features = ["v4", "serde"] }
# Git
git2 = "0.19"
# File watching
notify = "6.1"
# Progress bars
indicatif = "0.17"
# CLI
clap = { version = "4", features = ["derive"] }
# Regex
regex = "1.10"
# Decimal
rust_decimal = "1.35"
# Keyring
keyring = "3.0"
# Rate limiting
backoff = { version = "0.4", features = ["tokio"] }
# Synchronization
parking_lot = "0.12"
---
Core Types
/// Task identifier
pub type TaskId = String;
pub type BatchId = usize;
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
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
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
    
    pub fn next_tier(&self) -> Option<Self> {
        match self {
            WorkerTier::Simple => Some(WorkerTier::Medium),
            WorkerTier::Medium => Some(WorkerTier::Complex),
            WorkerTier::Complex => None,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskBatch {
    pub id: BatchId,
    pub tasks: Vec<Task>,
    pub tier: WorkerTier,
    pub dependencies: Vec<BatchId>,
    pub status: BatchStatus,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum BatchStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
    Skipped,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionPlan {
    pub id: String,
    pub created_at: DateTime<Local>,
    pub task_description: String,
    pub analysis: String,
    pub batches: Vec<TaskBatch>,
    pub total_cost_estimate: CostEstimate,
    pub status: PlanStatus,
    pub validation_attempts: usize,  // Track retries
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum PlanStatus {
    Draft,
    ValidationFailed,  // New: exceeded retry limit
    Approved,
    InProgress,
    Completed,
    Failed,
    Cancelled,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostEstimate {
    pub input_tokens: usize,
    pub output_tokens: usize,
    pub cost_usd: Decimal,
}
impl ExecutionPlan {
    /// Validate: no two tasks in same batch touch same file
    pub fn validate(&self) -> Result<(), ValidationError> {
        for batch in &self.batches {
            let mut files: HashSet<&Path> = HashSet::new();
            for task in &batch.tasks {
                for output in &task.expected_outputs {
                    if !files.insert(output.as_path()) {
                        return Err(ValidationError::FileOverlap {
                            batch_id: batch.id,
                            file: output.clone(),
                        });
                    }
                }
            }
        }
        Ok(())
    }
    
    /// Save rejected plan with error details
    pub fn save_rejected(&self, path: &Path, error: &ValidationError) -> Result<()> {
        let markdown = format!(
            "# Rejected Plan (Attempt {})\n\n\
             **Generated:** {}  \n\
             **Session:** {}  \n\
             **Error:** {:?}\n\n\
             ## Original Task\n{}\n\n\
             ## Analysis\n{}\n\n\
             ## Full Plan\n```json\n{}```\n",
            self.validation_attempts,
            self.created_at,
            self.id,
            error,
            self.task_description,
            self.analysis,
            serde_json::to_string_pretty(self)?
        );
        fs::write(path, markdown)?;
        Ok(())
    }
}
---
Plan Validation & Retry
pub struct PlanValidator {
    max_attempts: usize,
    rejected_plans_dir: PathBuf,
}
impl PlanValidator {
    pub async fn validate_with_retry(
        &self,
        mut plan: ExecutionPlan,
    ) -> Result<ExecutionPlan, PlanError> {
        for attempt in 1..=self.max_attempts {
            plan.validation_attempts = attempt;
            
            match plan.validate() {
                Ok(()) => return Ok(plan),
                Err(e) => {
                    tracing::warn!(
                        "Plan validation failed (attempt {}/{}): {:?}",
                        attempt,
                        self.max_attempts,
                        e
                    );
                    
                    // Save rejected plan for tuning
                    let rejected_path = self.rejected_plans_dir.join(format!(
                        "{}-attempt-{}.md",
                        plan.id,
                        attempt
                    ));
                    plan.save_rejected(&rejected_path, &e)?;
                    
                    if attempt < self.max_attempts {
                        // Regenerate with stronger prompt
                        plan = self.regenerate_with_constraint(plan).await?;
                    } else {
                        return Err(PlanError::ValidationFailed {
                            attempts: self.max_attempts,
                            last_error: e,
                        });
                    }
                }
            }
        }
        
        unreachable!()
    }
    
    async fn regenerate_with_constraint(
        &self,
        failed_plan: ExecutionPlan,
    ) -> Result<ExecutionPlan, PlanError> {
        // Add explicit constraint to prompt and regenerate
        let prompt = format!(
            "Previous plan failed validation: tasks in same batch touched same files.\n\
             CRITICAL: Ensure no two tasks in the same batch modify the same file.\n\n\
             Task: {}\n\n\
             Previous analysis: {}\n\n\
             Regenerate with proper file isolation.",
            failed_plan.task_description,
            failed_plan.analysis
        );
        
        // Call orchestrator with new prompt
        self.orchestrator.generate_plan(&prompt).await
    }
}
Planning prompt constraint:
CRITICAL CONSTRAINT: No two tasks within the same batch may modify the same file.
Each file can only be written by one task per batch. Tasks in different batches
may modify the same file (sequential dependency).
---
Git Worktree Architecture
pub struct WorktreeManager {
    repo: Repository,
    base_path: PathBuf,
}
pub struct WorkerWorktree {
    pub path: PathBuf,
    pub branch: String,
    _guard: WorktreeGuard,  // Cleans up on drop
}
impl WorktreeManager {
    /// Create isolated worktree for worker
    pub fn create_worktree(
        &self,
        session_id: &str,
        batch_id: BatchId,
        task_id: &TaskId,
        base_branch: &str,
    ) -> Result<WorkerWorktree> {
        let branch = format!(
            "orchestrator/{}/batch-{}-task-{}",
            session_id, batch_id, task_id
        );
        
        let path = self.base_path.join(&branch);
        
        // git worktree add --checkout <path> <branch>
        let worktree = self.repo.worktree(
            &branch,
            &path,
            Some(WorktreeAddOptions::new().reference(Some(base_branch))),
        )?;
        
        Ok(WorkerWorktree {
            path,
            branch,
            _guard: WorktreeGuard(worktree),
        })
    }
}
impl Drop for WorktreeGuard {
    fn drop(&mut self) {
        // git worktree remove <path>
        let _ = self.0.remove();
    }
}
---
Executor Flow
pub struct GitExecutor {
    worktree_manager: WorktreeManager,
    merger: BatchMerger,
    cleanup: BranchCleanup,
}
impl GitExecutor {
    pub async fn execute_batch(
        &self,
        batch: &TaskBatch,
        base_branch: &str,
        token: CancellationToken,
    ) -> Result<BatchResult> {
        let mut task_results = Vec::new();
        
        // Create worktrees in parallel (cheap)
        let worktrees: Vec<_> = batch
            .tasks
            .iter()
            .map(|t| self.worktree_manager.create_worktree(...))
            .collect::<Result<_>>()?;
        
        // Execute workers (parallel API calls)
        for (task, worktree) in batch.tasks.iter().zip(&worktrees) {
            token.check()?;
            
            let result = self.execute_worker(task, worktree).await?;
            task_results.push(result);
        }
        
        // Sequential merge (trivial due to file isolation)
        let merged_branch = self.merger.merge_batch(batch, &task_results, base_branch)?;
        
        // Cleanup worker branches
        for result in &task_results {
            self.cleanup.delete_branch(&result.branch)?;
        }
        
        Ok(BatchResult {
            merged_branch,
            tasks: task_results,
        })
    }
    
    async fn execute_worker(
        &self,
        task: &Task,
        worktree: &WorkerWorktree,
    ) -> Result<TaskResult> {
        // Worker is stateless HTTP call
        let response = self.model_client.complete(task.to_request()).await?;
        
        // Executor writes files (not worker)
        for file in &response.files {
            let path = worktree.path.join(&file.path);
            fs::create_dir_all(path.parent().unwrap())?;
            fs::write(&path, &file.content)?;
        }
        
        // Executor commits
        self.commit_worktree(worktree, &format!(
            "[orchestrator] Task {}: {}\n\nTokens: {} in / {} out",
            task.id, task.description,
            response.usage.input_tokens,
            response.usage.output_tokens
        ))?;
        
        Ok(TaskResult {
            task_id: task.id.clone(),
            branch: worktree.branch.clone(),
            commit: self.get_worktree_head(worktree)?,
            files: response.files,
            tokens: response.usage,
        })
    }
}
---
Conflict Handling (Edge Case)
If file isolation fails (shouldn't happen, but edge case):
pub fn handle_merge_conflict(
    conflict: MergeConflict,
    session_id: &str,
) -> Result<Action> {
    println!("═══════════════════════════════════════════════════════════");
    println!("                    MERGE CONFLICT                          ");
    println!("═══════════════════════════════════════════════════════════");
    println!();
    println!("The following files have conflicting changes:");
    for file in &conflict.files {
        println!("  • {}", file.display());
    }
    println!();
    println!("Conflicting tasks:");
    println!("  1. {}", conflict.task_a.description);
    println!("     Branch: {}", conflict.task_a.branch);
    println!("  2. {}", conflict.task_b.description);
    println!("     Branch: {}", conflict.task_b.branch);
    println!();
    println!("───────────────────────────────────────────────────────────");
    println!("Manual resolution required:");
    println!();
    println!("  cd {}", conflict.worktree_path.display());
    println!("  # Resolve conflicts in your editor");
    println!("  git add .");
    println!("  git commit -m 'Resolved conflicts'");
    println!("  orchestrator resume --session-id {}", session_id);
    println!();
    println!("───────────────────────────────────────────────────────────");
    
    // Save state for resume
    self.save_checkpoint(session_id, &conflict)?;
    
    Action::ExitForManualResolution
}
---
Branch Cleanup
pub struct BranchCleanup;
impl BranchCleanup {
    /// Delete all session branches on successful completion
    pub fn cleanup_success(&self, session_id: &str) -> Result<()> {
        let branches = self.list_session_branches(session_id)?;
        for branch in branches {
            self.delete_branch(&branch)?;
        }
        Ok(())
    }
    
    /// Keep branches on failure (forensics), provide cleanup command
    pub fn cleanup_failed_session(&self, session_id: &str) -> Result<CleanupReport> {
        let branches = self.list_session_branches(session_id)?;
        Ok(CleanupReport {
            session_id: session_id.to_string(),
            branches_found: branches.len(),
            disk_usage: self.calculate_size(&branches)?,
        })
    }
}
// CLI command: orchestrator cleanup --older-than 7days --dry-run
---
Rate Limiting with Jitter
pub struct RateLimiter {
    config: RateLimitConfig,
}
pub struct RateLimitConfig {
    initial_backoff_ms: u64,
    max_backoff_ms: u64,
    multiplier: f64,
    jitter: f64,          // ±25%
    max_retries: usize,
}
impl RateLimiter {
    pub async fn execute_with_backoff<F, T>(
        &self,
        operation: F,
    ) -> Result<T>
    where
        F: Fn() -> impl Future<Output = Result<T, RateLimitError>>,
    {
        let mut backoff = self.config.initial_backoff_ms;
        
        for attempt in 0..self.config.max_retries {
            match operation().await {
                Ok(result) => return Ok(result),
                Err(RateLimitError::RateLimited { retry_after }) => {
                    let jitter = 1.0 + (rand::random::<f64>() - 0.5) 
                        * 2.0 * self.config.jitter;
                    
                    let wait_ms = retry_after
                        .map(|d| d.as_millis() as u64)
                        .unwrap_or(backoff);
                    
                    let wait = Duration::from_millis(
                        (wait_ms as f64 * jitter) as u64
                    );
                    
                    sleep(wait).await;
                    
                    backoff = ((backoff as f64 * self.config.multiplier) 
                        as u64)
                        .min(self.config.max_backoff_ms);
                }
                Err(e) => return Err(e.into()),
            }
        }
        
        Err(Error::MaxRetriesExceeded)
    }
}
---
Cancellation (Hybrid)
pub struct CancellationToken {
    cancelled: Arc<AtomicBool>,
}
impl CancellationToken {
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }
    
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }
    
    pub fn check(&self) -> Result<()> {
        if self.is_cancelled() {
            Err(Error::Cancelled)
        } else {
            Ok(())
        }
    }
}
// Usage: finish current batch, save checkpoint, stop
pub async fn execute_batch(...) -> Result<BatchResult> {
    for task in &batch.tasks {
        token.check()?;  // Check before each task
        
        let result = execute_task(task).await?;
        
        // If cancelled mid-batch, finish this batch then stop
        if token.is_cancelled() {
            save_checkpoint(batch.id, &results).await?;
            return Err(Error::Cancelled);
        }
    }
    
    Ok(result)
}
// Resume: checkout last merged batch, continue from there
pub async fn resume_session(session_id: &str) -> Result<Session> {
    let checkpoint = load_checkpoint(session_id).await?;
    let repo = open_repository()?;
    repo.checkout(&checkpoint.last_merged_branch)?;
    
    Session::resume_from(checkpoint)
}
---
Configuration
[general]
execution_mode = "in_process"
max_concurrent_workers = 5
auto_retry = true
max_retries = 1
escalate_on_retry = true
[orchestrator]
provider = "openrouter"
model = "moonshotai/kimi-k2.5"
temperature = 0.1
max_context = 200000
stream = true
stream_buffer_lines = 3
[validation]
max_plan_attempts = 3          # Initial + 2 retries
rejected_plans_dir = ".orchestrator/rejected-plans"
[interactive]
auto_plan = false
cost_warnings = true
warning_threshold = 1.0
show_token_estimates = true
multiline_input = true
[tier.simple]
model = "qwen/qwen3.5-4b"
provider = "openrouter"
context_window = 32768
max_tokens = 4096
cost_per_1k_input = 0.0001
cost_per_1k_output = 0.0002
[tier.medium]
model = "qwen/qwen3.5-9b"
provider = "openrouter"
context_window = 65536
max_tokens = 8192
cost_per_1k_input = 0.0002
cost_per_1k_output = 0.0004
[tier.complex]
model = "qwen/qwen3.5-32b"
provider = "openrouter"
context_window = 65536
max_tokens = 8192
cost_per_1k_input = 0.0006
cost_per_1k_output = 0.0012
[context]
mode = "simple"
max_files = 50
max_tokens = 100000
[file_watcher]
enabled = true
debounce_ms = 500
notify_on_external_change = true
[git]
auto_branch = true
branch_prefix = "orchestrator/"
auto_commit = false
commit_message_template = "[orchestrator] {task_summary}"
cleanup_on_success = true
[tui]
refresh_rate_ms = 100
theme = "default"
[rate_limit]
max_retries = 3
initial_backoff_ms = 1000
max_backoff_ms = 60000
multiplier = 2.0
jitter = 0.25
---
Implementation Phases
Phase 1: Core TUI + Planning (14 days)
Day	Milestone	Deliverable
1	Foundation	Dependencies, crate structure, basic TUI loop, git commit
2-3	Core Types	Task, Plan, Message types; Config system; API key resolution
4-6	Chat + Discovery	Streaming SSE chat, multi-line input, Discovery mode
6-8	Planning	Plan generation, validation with retry (2 attempts), rejected plan logging
8-9	Wizard	Granular approval wizard, step navigation
10-11	Log View	Log panel, tab switching, filtering, save
12-14	Persistence	Plan auto-save to .md, polish, edge cases
Phase 2: Execution + Workers (12 days)
Day	Milestone	Deliverable
15-17	Git Worktrees	Worktree management, worker isolation, executor flow
18	Batch Merging	Sequential merge, conflict detection (edge case handoff)
19	File Watching	notify-rs integration, external change detection
20	Error Handling	Auto-retry with escalation, user intervention modal
21	Cancellation	Hybrid cancellation, checkpoint/resume
22-23	Rate Limiting	Exponential backoff with jitter, 429 handling
24	Branch Cleanup	Auto-delete on success, cleanup command
25-26	Polish	Edge cases, performance, documentation
---
## Testing Strategy
### Unit Tests
- Plan validation (file overlap detection)
- Cost calculation with Decimal
- Rate limit backoff math (with jitter)
- Git branch name generation
- Worktree path sanitization
### Integration Tests
- Mock OpenRouter with SSE streaming
- End-to-end planning with validation retries
- Worktree create/write/commit/destroy cycle
- Cancellation checkpoint save/resume
### Manual Tests
- Real API calls (small tasks)
- File overlap edge case (force conflict)
- Large context handling (200k)
- Concurrent worker stress test
---
Key Principles
1. Orchestrator validates: File isolation enforced at plan time, not merge time
2. Executor owns git: Workers are stateless HTTP clients
3. Fail fast on plan: Up to 2 regenerates, log rejected plans for tuning
4. Clean on success: Session branches deleted after merge to master
5. Forensics on failure: Keep branches, provide cleanup command
6. Hand off conflicts: Edge case rare by construction, drop to shell with good diagnostics
7. Resume capability: Hybrid cancellation saves checkpoint, resume continues from last merged batch
