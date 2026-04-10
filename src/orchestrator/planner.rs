use crate::core::ExecutionPlan;
use anyhow::Result;

pub struct Planner;

impl Planner {
    pub fn new() -> Self {
        Self
    }
    
    pub async fn generate_plan(&self, task_description: &str) -> Result<ExecutionPlan> {
        // TODO: Implement plan generation
        let mut plan = ExecutionPlan::new(task_description.to_string());
        plan.analysis = "Plan generation not yet implemented".to_string();
        Ok(plan)
    }
}