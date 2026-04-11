use crate::core::{plan::ValidationError, ExecutionPlan, PlanStatus};
use crate::orchestrator::client::OrchestratorClient;
use crate::orchestrator::prompts::create_regeneration_prompt;
use anyhow::{Context, Result};
use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum PlanValidationError {
    #[error("Plan validation failed after {attempts} attempts: {last_error}")]
    ValidationFailed {
        attempts: usize,
        last_error: ValidationError,
    },

    #[error("Failed to save rejected plan: {0}")]
    SaveError(String),

    #[error("Orchestrator error: {0}")]
    OrchestratorError(#[from] anyhow::Error),
}

pub struct PlanValidator {
    max_attempts: usize,
    rejected_plans_dir: PathBuf,
    client: OrchestratorClient,
}

impl PlanValidator {
    pub fn new(max_attempts: usize, client: OrchestratorClient) -> Self {
        Self {
            max_attempts,
            rejected_plans_dir: PathBuf::from(".orchestrator/rejected-plans"),
            client,
        }
    }

    pub fn with_rejected_plans_dir(mut self, dir: PathBuf) -> Self {
        self.rejected_plans_dir = dir;
        self
    }

    /// Validate a plan with retry logic
    /// 
    /// If validation fails, the plan is regenerated up to max_attempts times
    /// with progressively stronger prompts explaining the constraint violation.
    pub async fn validate_with_retry(
        &self,
        mut plan: ExecutionPlan,
        task_description: &str,
    ) -> std::result::Result<ExecutionPlan, PlanValidationError> {
        // Ensure the rejected plans directory exists
        if let Err(e) = tokio::fs::create_dir_all(&self.rejected_plans_dir).await {
            tracing::warn!("Failed to create rejected plans directory: {}", e);
        }

        for attempt in 1..=self.max_attempts {
            plan.validation_attempts = attempt;

            match plan.validate() {
                Ok(()) => {
                    tracing::info!("Plan validation succeeded on attempt {}", attempt);
                    plan.status = PlanStatus::Draft;
                    return Ok(plan);
                }
                Err(e) => {
                    tracing::warn!(
                        "Plan validation failed (attempt {}/{}): {:?}",
                        attempt,
                        self.max_attempts,
                        e
                    );

                    // Save the rejected plan
                    if let Err(save_err) = self.save_rejected_plan(&plan, &e, attempt).await {
                        tracing::error!("Failed to save rejected plan: {}", save_err);
                    }

                    if attempt < self.max_attempts {
                        // Regenerate with stronger constraint
                        tracing::info!("Regenerating plan with stronger constraints...");
                        plan = self
                            .regenerate_plan(&plan, task_description, &e.to_string(), attempt)
                            .await?;
                    } else {
                        // Max attempts reached
                        return Err(PlanValidationError::ValidationFailed {
                            attempts: self.max_attempts,
                            last_error: e,
                        });
                    }
                }
            }
        }

        unreachable!()
    }

    /// Save a rejected plan to the rejected-plans directory for forensics
    async fn save_rejected_plan(
        &self,
        plan: &ExecutionPlan,
        error: &ValidationError,
        attempt: usize,
    ) -> Result<()> {
        let timestamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
        let filename = format!("{}-{}-attempt-{}.md", timestamp, plan.id, attempt);
        let path = self.rejected_plans_dir.join(&filename);

        let markdown = format!(
            "# Rejected Plan (Attempt {} of {})\n\n\
             **Plan ID:** {}  \n\
             **Generated:** {}  \n\
             **Validation Error:** {:?}\n\n\
             ## Original Task\n{}\n\n\
             ## Analysis\n{}\n\n\
             ## JSON Representation\n```json\n{}\n```\n",
            attempt,
            self.max_attempts,
            plan.id,
            plan.created_at.format("%Y-%m-%d %H:%M:%S"),
            error,
            plan.task_description,
            plan.analysis,
            serde_json::to_string_pretty(plan).unwrap_or_else(|e| format!("Error serializing: {}", e))
        );

        tokio::fs::write(&path, markdown)
            .await
            .with_context(|| format!("Failed to write rejected plan to {}", path.display()))?;

        tracing::info!("Saved rejected plan to {}", path.display());
        Ok(())
    }

    /// Regenerate a plan with stronger constraints based on the validation error
    async fn regenerate_plan(
        &self,
        failed_plan: &ExecutionPlan,
        task_description: &str,
        validation_error: &str,
        attempt: usize,
    ) -> Result<ExecutionPlan> {
        let prompt = create_regeneration_prompt(
            task_description,
            &failed_plan.analysis,
            validation_error,
            attempt,
        );

        // Create messages for the orchestrator
        let messages = vec![
            crate::core::ChatMessage::system(crate::orchestrator::prompts::PLANNER_SYSTEM_PROMPT),
            crate::core::ChatMessage::user(prompt),
        ];

        // Get response from orchestrator
        let (response_text, _usage) = self
            .client
            .chat(&messages)
            .await
            .context("Failed to regenerate plan from orchestrator")?;

        // Parse the JSON response
        let new_plan: ExecutionPlan = serde_json::from_str(&response_text)
            .with_context(|| format!("Failed to parse regenerated plan: {}", response_text))?;

        Ok(new_plan)
    }

    /// Simple validation without retry (for external use)
    pub fn validate(&self, plan: &ExecutionPlan) -> std::result::Result<(), ValidationError> {
        plan.validate()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{BatchStatus, CostEstimate, Task, TaskBatch, WorkerTier};
    use rust_decimal::Decimal;

    #[test]
    fn test_simple_validation() {
        // This test would need a mock orchestrator client
        // For now, just test the validate method directly
    }
}
