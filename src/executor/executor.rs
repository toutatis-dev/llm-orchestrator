use crate::config::Config;
use crate::core::{BatchStatus, ExecutionPlan, Task, TaskBatch, WorkerTier};
use crate::executor::worktree::{WorktreeManager, WorkerWorktree};
use crate::git::branch::BranchManager;
use crate::models::openrouter::OpenRouterClient;
use crate::models::types::{CompletionRequest, Message};
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::Path;
use tokio::task::JoinHandle;

/// Result of executing a single task
#[derive(Debug, Clone)]
pub struct TaskResult {
    pub task_id: String,
    pub success: bool,
    pub files_written: Vec<std::path::PathBuf>,
    pub commit_sha: Option<String>,
    pub error: Option<String>,
    pub tokens_used: TokenUsage,
}

#[derive(Debug, Clone, Default)]
pub struct TokenUsage {
    pub input: usize,
    pub output: usize,
}

/// Result of executing a batch
#[derive(Debug, Clone)]
pub struct BatchResult {
    pub batch_id: usize,
    pub task_results: Vec<TaskResult>,
    pub merged_branch: Option<String>,
    pub success: bool,
}

/// Executor that manages worker task execution using git worktrees
pub struct Executor {
    worktree_manager: WorktreeManager,
    branch_manager: BranchManager,
    config: Config,
    session_id: String,
}

impl Executor {
    /// Create a new Executor for the given repository
    pub fn new(repo_path: &Path, session_id: String, config: Config) -> Result<Self> {
        let worktree_manager = WorktreeManager::new(repo_path)?;
        let branch_manager = BranchManager::new(repo_path)?;

        Ok(Self {
            worktree_manager,
            branch_manager,
            config,
            session_id,
        })
    }

    /// Execute a complete plan by running batches in dependency order
    pub async fn execute_plan(&mut self, plan: &mut ExecutionPlan) -> Result<Vec<BatchResult>> {
        let mut batch_results = Vec::new();
        let mut completed_batches: HashMap<usize, String> = HashMap::new(); // batch_id -> commit_sha

        plan.status = crate::core::PlanStatus::InProgress;

        // Sort batches topologically (dependencies first)
        let sorted_ids = plan.topological_sort_ids()
            .map_err(|batch_id| anyhow::anyhow!("Circular dependency detected in batch {}", batch_id))?;

        // Process batches in topological order
        for batch_id in sorted_ids {
            let batch = plan.batches.iter_mut().find(|b| b.id == batch_id)
                .expect("Batch from topological sort must exist");
            // Check if all dependencies are satisfied
            let deps_satisfied = batch.dependencies.iter().all(|dep_id| {
                completed_batches.contains_key(dep_id)
            });

            if !deps_satisfied {
                return Err(anyhow::anyhow!(
                    "Batch {} has unsatisfied dependencies: {:?}",
                    batch.id,
                    batch.dependencies
                ));
            }

            // Get the base branch (last merged batch or current HEAD)
            let base_branch = batch.dependencies
                .iter()
                .filter_map(|dep_id| completed_batches.get(dep_id))
                .last()
                .cloned()
                .unwrap_or_else(|| {
                    // Resolve HEAD to actual branch name for better tracking
                    match self.branch_manager.head_sha() {
                        Ok(sha) => sha,
                        Err(_) => "HEAD".to_string(),
                    }
                });

            // Execute the batch
            let result = self.execute_batch(batch, &base_branch).await?;
            batch_results.push(result.clone());

            if result.success {
                batch.status = Some(BatchStatus::Completed);
                if let Some(branch) = result.merged_branch {
                    completed_batches.insert(batch.id, branch);
                }
            } else {
                batch.status = Some(BatchStatus::Failed);
                tracing::error!("Batch {} failed", batch.id);
                // Continue with remaining batches or stop based on config
                if !self.config.general.auto_retry {
                    break;
                }
            }
        }

        // Update plan status
        let all_success = batch_results.iter().all(|r| r.success);
        plan.status = if all_success {
            crate::core::PlanStatus::Completed
        } else {
            crate::core::PlanStatus::Failed
        };

        Ok(batch_results)
    }

    /// Execute a single batch of tasks in parallel
    pub async fn execute_batch(
        &mut self,
        batch: &TaskBatch,
        base_branch: &str,
    ) -> Result<BatchResult> {
        tracing::info!(
            "Executing batch {} with {} tasks (tier: {:?})",
            batch.id,
            batch.tasks.len(),
            batch.tier
        );

        // Create worktrees for all tasks in parallel
        let worktrees: Vec<WorkerWorktree> = batch
            .tasks
            .iter()
            .map(|task| {
                self.worktree_manager.create_worktree(
                    &self.session_id,
                    batch.id,
                    &task.id,
                    base_branch,
                )
            })
            .collect::<Result<Vec<_>>>()?;

        // Execute tasks in parallel
        let mut handles: Vec<JoinHandle<Result<TaskResult>>> = Vec::new();

        for (task, worktree) in batch.tasks.iter().zip(worktrees.clone()) {
            let task = task.clone();
            let config = self.config.clone();
            let handle = tokio::spawn(async move {
                execute_task(&task, &worktree, &config).await
            });
            handles.push(handle);
        }

        // Collect results
        let mut task_results = Vec::new();
        for handle in handles {
            match handle.await {
                Ok(Ok(result)) => task_results.push(result),
                Ok(Err(e)) => {
                    tracing::error!("Task execution failed: {}", e);
                    task_results.push(TaskResult {
                        task_id: "unknown".to_string(),
                        success: false,
                        files_written: Vec::new(),
                        commit_sha: None,
                        error: Some(e.to_string()),
                        tokens_used: TokenUsage::default(),
                    });
                }
                Err(e) => {
                    tracing::error!("Task panicked: {}", e);
                    task_results.push(TaskResult {
                        task_id: "unknown".to_string(),
                        success: false,
                        files_written: Vec::new(),
                        commit_sha: None,
                        error: Some(format!("Task panicked: {}", e)),
                        tokens_used: TokenUsage::default(),
                    });
                }
            }
        }

        // Check if all tasks succeeded
        let all_success = task_results.iter().all(|r| r.success);

        if !all_success {
            // Some tasks failed - clean up worktrees but keep branches for forensics
            tracing::warn!("Batch {} had failures, cleaning up worktrees", batch.id);
            for result in &task_results {
                if let Some(worktree) = worktrees.iter().find(|w| w.branch.contains(&result.task_id)) {
                    // Just remove the worktree directory, keep the branch
                    let _ = std::fs::remove_dir_all(&worktree.path);
                }
            }

            return Ok(BatchResult {
                batch_id: batch.id,
                task_results,
                merged_branch: None,
                success: false,
            });
        }

        // All tasks succeeded - merge branches
        let merged_branch = self.merge_batch_results(batch, &task_results, base_branch).await?;

        // Clean up worktrees and branches
        for worktree in worktrees {
            if let Err(e) = self.worktree_manager.remove_worktree(worktree) {
                tracing::warn!("Failed to remove worktree: {}", e);
            }
        }

        Ok(BatchResult {
            batch_id: batch.id,
            task_results,
            merged_branch: Some(merged_branch),
            success: true,
        })
    }

    /// Merge all task results from a batch into a single branch
    async fn merge_batch_results(
        &mut self,
        batch: &TaskBatch,
        results: &[TaskResult],
        base_branch: &str,
    ) -> Result<String> {
        let merged_branch = format!("orchestrator/{}/batch-{}-merged", self.session_id, batch.id);

        // Create merged branch from base
        self.branch_manager.create_branch(&merged_branch, base_branch)?;

        // Merge each task's branch into the merged branch
        for result in results {
            if let Some(commit_sha) = &result.commit_sha {
                let task_branch = format!(
                    "orchestrator/{}/batch-{}-task-{}",
                    self.session_id,
                    batch.id,
                    result.task_id
                );

                let message = format!(
                    "[orchestrator] Merge task {}: {}\n\nCommit: {}",
                    result.task_id,
                    if result.success { "success" } else { "failed" },
                    commit_sha
                );

                self.branch_manager
                    .merge_branch(&task_branch, &message)
                    .with_context(|| format!("Failed to merge task {} into batch merge", result.task_id))?;
            }
        }

        tracing::info!(
            "Created merged branch '{}' for batch {}",
            merged_branch,
            batch.id
        );

        Ok(merged_branch)
    }
}

/// Execute a single task in its worktree
async fn execute_task(
    task: &Task,
    worktree: &WorkerWorktree,
    config: &Config,
) -> Result<TaskResult> {
    tracing::info!("Executing task {}: {}", task.id, task.description);

    // Get the appropriate model for this task tier
    let tier_config = config.tiers.get(&format!("{:?}", task.tier).to_lowercase())
        .or_else(|| config.tiers.get("simple"))
        .context("No tier configuration found")?;

    // Create API client
    let api_key = crate::api_key::resolve_api_key()?;
    let client = OpenRouterClient::new(api_key);

    // Build completion request
    let request = CompletionRequest {
        model: tier_config.model.clone(),
        messages: vec![
            Message {
                role: "system".to_string(),
                content: build_system_prompt(task),
            },
            Message {
                role: "user".to_string(),
                content: build_user_prompt(task),
            },
        ],
        temperature: 0.1,
        max_tokens: tier_config.max_tokens,
        stream: false,
    };

    // Call the model
    let response = client.complete(request).await
        .with_context(|| format!("Failed to execute task {} with model", task.id))?;

    // Parse response and write files
    let content = response.choices
        .first()
        .map(|c| c.message.content.clone())
        .unwrap_or_default();

    let files_written = write_task_output(task, worktree, &content).await
        .with_context(|| format!("Failed to write output for task {}", task.id))?;

    // Commit changes
    let commit_sha = commit_worktree_changes(worktree, task, &response.usage)
        .with_context(|| format!("Failed to commit changes for task {}", task.id))?;

    tracing::info!(
        "Task {} completed, wrote {} files, commit {}",
        task.id,
        files_written.len(),
        commit_sha.as_deref().unwrap_or("unknown")
    );

    Ok(TaskResult {
        task_id: task.id.clone(),
        success: true,
        files_written,
        commit_sha,
        error: None,
        tokens_used: TokenUsage {
            input: response.usage.prompt_tokens,
            output: response.usage.completion_tokens,
        },
    })
}

fn build_system_prompt(task: &Task) -> String {
    format!(
        "You are a software development assistant. Your task is: {}\n\n\
         Task type: {:?}\n\n\
         Output each file as a fenced code block where the opening fence is followed \
         immediately by the file path. For example:\n\n\
         ```src/main.rs\n\
         fn main() {{\n\
             println!(\"Hello, world!\");\n\
         }}\n\
         ```\n\n\
         You can output multiple files by including multiple code blocks.",
        task.description,
        task.task_type
    )
}

fn build_user_prompt(task: &Task) -> String {
    let mut prompt = task.context.clone();
    
    if !task.inputs.is_empty() {
        prompt.push_str("\n\nInput files:\n");
        for input in &task.inputs {
            prompt.push_str(&format!("- {}\n", input.display()));
        }
    }

    if !task.expected_outputs.is_empty() {
        prompt.push_str("\n\nExpected output files:\n");
        for output in &task.expected_outputs {
            prompt.push_str(&format!("- {}\n", output.display()));
        }
    }

    prompt
}

async fn write_task_output(
    task: &Task,
    worktree: &WorkerWorktree,
    content: &str,
) -> Result<Vec<std::path::PathBuf>> {
    let mut files_written = Vec::new();
    let mut lines = content.lines().peekable();

    while let Some(line) = lines.next() {
        // Look for opening fence: ```filename
        if line.starts_with("```") && line.len() > 3 {
            let remainder = &line[3..];
            // Skip language tags that don't look like paths
            if remainder.is_empty() || remainder.starts_with("language") {
                continue;
            }
            
            // Extract filename
            let filename = remainder.trim();
            if filename.is_empty() {
                continue;
            }

            // Collect content until closing fence
            let mut file_content = String::new();
            let mut found_closing = false;
            
            while let Some(content_line) = lines.next() {
                if content_line == "```" {
                    found_closing = true;
                    break;
                }
                if !file_content.is_empty() {
                    file_content.push('\n');
                }
                file_content.push_str(content_line);
            }

            if !found_closing {
                tracing::warn!("Unclosed code block for file: {}", filename);
            }

            // Write the file
            let file_path = worktree.path.join(filename);
            
            // Ensure parent directory exists
            if let Some(parent) = file_path.parent() {
                tokio::fs::create_dir_all(parent).await?;
            }
            
            tokio::fs::write(&file_path, file_content.as_bytes()).await?;
            tracing::info!("Wrote file: {:?} ({} bytes)", file_path, file_content.len());
            files_written.push(file_path);
        }
    }

    // If no files were extracted but we have expected outputs, fall back to writing
    // the entire response to the first expected output
    if files_written.is_empty() && !task.expected_outputs.is_empty() {
        let output = &task.expected_outputs[0];
        let file_path = worktree.path.join(output);
        if let Some(parent) = file_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(&file_path, content.as_bytes()).await?;
        tracing::info!("Wrote fallback file: {:?}", file_path);
        files_written.push(file_path);
    }

    Ok(files_written)
}

fn commit_worktree_changes(
    worktree: &WorkerWorktree,
    task: &Task,
    usage: &crate::models::types::Usage,
) -> Result<Option<String>> {
    // Open the worktree as a repository
    let repo = git2::Repository::open(&worktree.path)?;

    // Add all changes
    let mut index = repo.index()?;
    index.add_all(&["*"], git2::IndexAddOption::DEFAULT, None)?;
    index.write()?;

    // Create commit
    let tree_id = index.write_tree()?;
    let tree = repo.find_tree(tree_id)?;
    let sig = repo.signature()?;

    // Get parent commit
    let head = repo.head()?;
    let parent_commit = head.peel_to_commit()?;

    let message = format!(
        "[orchestrator] Task {}: {}\n\nTokens: {} in / {} out",
        task.id,
        task.description,
        usage.prompt_tokens,
        usage.completion_tokens
    );

    let commit_id = repo.commit(
        Some("HEAD"),
        &sig,
        &sig,
        &message,
        &tree,
        &[&parent_commit],
    )?;

    Ok(Some(commit_id.to_string()))
}
