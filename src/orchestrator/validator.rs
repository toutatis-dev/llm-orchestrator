use crate::core::{plan::ValidationError, ExecutionPlan};

pub struct PlanValidator {
    max_attempts: usize,
}

impl PlanValidator {
    pub fn new(max_attempts: usize) -> Self {
        Self { max_attempts }
    }

    pub fn validate(&self, plan: &ExecutionPlan) -> Result<(), ValidationError> {
        plan.validate()
    }
}
