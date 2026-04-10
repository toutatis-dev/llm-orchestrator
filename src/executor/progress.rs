#[derive(Debug, Clone)]
pub struct ExecutionProgress {
    pub current_batch: usize,
    pub total_batches: usize,
    pub tasks_completed: usize,
    pub total_tasks: usize,
}

impl Default for ExecutionProgress {
    fn default() -> Self {
        Self {
            current_batch: 0,
            total_batches: 0,
            tasks_completed: 0,
            total_tasks: 0,
        }
    }
}
