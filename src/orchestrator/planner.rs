use crate::core::{BatchStatus, CostEstimate, ExecutionPlan, Task, TaskBatch, TaskType, WorkerTier};
use crate::orchestrator::client::OrchestratorClient;
use crate::orchestrator::prompts::{create_plan_user_prompt, PLANNER_SYSTEM_PROMPT};
use anyhow::{Context, Result};
use rust_decimal::Decimal;
use serde::Deserialize;

pub struct Planner {
    client: OrchestratorClient,
}

/// Plan response structure from the orchestrator
#[derive(Debug, Deserialize)]
struct PlanResponse {
    analysis: String,
    batches: Vec<BatchSpec>,
    #[serde(default)]
    total_cost_estimate: CostEstimateSpec,
}

#[derive(Debug, Deserialize)]
struct BatchSpec {
    id: usize,
    tier: TierSpec,
    #[serde(default)]
    dependencies: Vec<usize>,
    tasks: Vec<TaskSpec>,
}

#[derive(Debug, Deserialize)]
struct TaskSpec {
    id: String,
    description: String,
    #[serde(rename = "task_type")]
    task_type: TaskTypeSpec,
    tier: TierSpec,
    #[serde(default)]
    inputs: Vec<String>,
    expected_outputs: Vec<String>,
    #[serde(default)]
    context: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
enum TierSpec {
    Simple,
    Medium,
    Complex,
}

impl From<TierSpec> for WorkerTier {
    fn from(tier: TierSpec) -> Self {
        match tier {
            TierSpec::Simple => WorkerTier::Simple,
            TierSpec::Medium => WorkerTier::Medium,
            TierSpec::Complex => WorkerTier::Complex,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
enum TaskTypeSpec {
    Generation,
    Refactor,
    Documentation,
    Test,
    Analysis,
}

impl From<TaskTypeSpec> for TaskType {
    fn from(task_type: TaskTypeSpec) -> Self {
        match task_type {
            TaskTypeSpec::Generation => TaskType::Generation,
            TaskTypeSpec::Refactor => TaskType::Refactor,
            TaskTypeSpec::Documentation => TaskType::Documentation,
            TaskTypeSpec::Test => TaskType::Test,
            TaskTypeSpec::Analysis => TaskType::Analysis,
        }
    }
}

#[derive(Debug, Deserialize, Default)]
struct CostEstimateSpec {
    input_tokens: usize,
    output_tokens: usize,
    #[serde(deserialize_with = "deserialize_decimal")]
    cost_usd: Decimal,
}

fn deserialize_decimal<'de, D>(deserializer: D) -> Result<Decimal, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::Error;
    let s = String::deserialize(deserializer)?;
    Decimal::from_str_exact(&s).map_err(D::Error::custom)
}

impl Planner {
    pub fn new(client: OrchestratorClient) -> Self {
        Self { client }
    }

    pub async fn generate_plan(
        &self,
        task_description: &str,
        context: Option<&str>,
    ) -> Result<ExecutionPlan> {
        let user_prompt = create_plan_user_prompt(task_description, context);

        // Create messages for the orchestrator
        let messages = vec![
            crate::core::ChatMessage::system(PLANNER_SYSTEM_PROMPT),
            crate::core::ChatMessage::user(user_prompt),
        ];

        // Get non-streaming response for plan generation
        let (response_text, _usage) = self
            .client
            .chat(&messages)
            .await
            .context("Failed to generate plan from orchestrator")?;

        // Parse the JSON response
        let plan_spec: PlanResponse = serde_json::from_str(&response_text)
            .with_context(|| format!("Failed to parse plan response as JSON: {}", response_text))?;

        // Convert to ExecutionPlan
        let mut plan = ExecutionPlan::new(task_description.to_string());
        plan.analysis = plan_spec.analysis;
        plan.total_cost_estimate = CostEstimate {
            input_tokens: plan_spec.total_cost_estimate.input_tokens,
            output_tokens: plan_spec.total_cost_estimate.output_tokens,
            cost_usd: plan_spec.total_cost_estimate.cost_usd,
        };

        // Convert batches
        plan.batches = plan_spec
            .batches
            .into_iter()
            .map(|batch| TaskBatch {
                id: batch.id,
                tier: batch.tier.into(),
                dependencies: batch.dependencies,
                tasks: batch
                    .tasks
                    .into_iter()
                    .map(|task| Task {
                        id: task.id,
                        description: task.description,
                        task_type: task.task_type.into(),
                        tier: task.tier.into(),
                        inputs: task.inputs.into_iter().map(std::path::PathBuf::from).collect(),
                        expected_outputs: task
                            .expected_outputs
                            .into_iter()
                            .map(std::path::PathBuf::from)
                            .collect(),
                        context: task.context,
                    })
                    .collect(),
                status: Some(BatchStatus::Pending),
            })
            .collect();

        Ok(plan)
    }

    /// Generate plan with progress callback for streaming updates
    pub async fn generate_plan_with_progress<F>(
        &self,
        task_description: &str,
        context: Option<&str>,
        mut _on_progress: F,
    ) -> Result<ExecutionPlan>
    where
        F: FnMut(&str),
    {
        // For now, use non-streaming version
        // TODO: Implement streaming JSON parsing for real-time progress
        self.generate_plan(task_description, context).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_plan_response() {
        let json = r#"{
            "analysis": "This is a test plan",
            "batches": [
                {
                    "id": 1,
                    "tier": "simple",
                    "dependencies": [],
                    "tasks": [
                        {
                            "id": "task-1",
                            "description": "Create main.rs",
                            "task_type": "generation",
                            "tier": "simple",
                            "inputs": [],
                            "expected_outputs": ["src/main.rs"],
                            "context": "Entry point for the application"
                        }
                    ]
                }
            ],
            "total_cost_estimate": {
                "input_tokens": 1000,
                "output_tokens": 500,
                "cost_usd": "0.0003"
            }
        }"#;

        let plan: PlanResponse = serde_json::from_str(json).unwrap();
        assert_eq!(plan.analysis, "This is a test plan");
        assert_eq!(plan.batches.len(), 1);
        assert_eq!(plan.batches[0].tasks.len(), 1);
    }
}
