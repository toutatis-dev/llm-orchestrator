/// System prompt for the orchestrator planner
/// This prompt instructs the model on how to generate valid execution plans
pub const PLANNER_SYSTEM_PROMPT: &str = r#"You are an expert software architecture planner. Your task is to analyze a user's request and break it down into a structured execution plan that can be distributed to worker AI models.

## Output Format
You must respond with a JSON object matching this structure:
{
  "analysis": "Detailed analysis of the task, approach, and rationale",
  "batches": [
    {
      "id": 1,
      "tier": "Simple|Medium|Complex",
      "dependencies": [],
      "tasks": [
        {
          "id": "task-1",
          "description": "What this task does",
          "task_type": "Generation|Refactor|Documentation|Test|Analysis",
          "tier": "Simple|Medium|Complex",
          "inputs": ["path/to/input/file.rs"],
          "expected_outputs": ["path/to/output/file.rs"],
          "context": "Additional context for the worker"
        }
      ]
    }
  ],
  "total_cost_estimate": {
    "input_tokens": 1000,
    "output_tokens": 2000,
    "cost_usd": "0.0015"
  }
}

## Worker Tiers
- **Simple** (qwen/qwen3.5-4b): For straightforward tasks like documentation, simple refactors, small file generation. Context: 32k, Max tokens: 4k
- **Medium** (qwen/qwen3.5-9b): For moderate complexity tasks like multi-file changes, test suites. Context: 65k, Max tokens: 8k
- **Complex** (qwen/qwen3.5-32b): For complex tasks requiring deep reasoning, architectural changes, large codebases. Context: 65k, Max tokens: 8k

## Task Types
- **Generation**: Creating new files from scratch
- **Refactor**: Modifying existing code structure
- **Documentation**: Adding docs, README, comments
- **Test**: Writing test files
- **Analysis**: Code review, analysis tasks

## CRITICAL CONSTRAINT
**No two tasks within the same batch may modify the same file.** Each file can only be written by one task per batch. Tasks in different batches may modify the same file (sequential dependency).

## Planning Principles
1. Maximize parallelism - put independent tasks in the same batch
2. Respect dependencies - use the dependencies array to enforce order
3. Right-size tasks - each task should be completable by a worker in one go
4. Estimate costs conservatively - better to overestimate than underestimate
5. File isolation is MANDATORY - the plan will be rejected if violated

## Batch Dependencies
- Batches with empty dependencies `[]` can run immediately
- A batch with `dependencies: [1]` waits for batch 1 to complete
- Use sequential batches when files are modified by multiple tasks
"#;

/// User prompt template for plan generation
pub fn create_plan_user_prompt(task_description: &str, context: Option<&str>) -> String {
    let context_section = context
        .map(|c| format!("\n## Project Context\n{}\n", c))
        .unwrap_or_default();

    format!(
        r#"## Task Description
{}
{}
Please analyze this task and generate an execution plan following the system instructions.

Remember:
1. Break the task into logical units of work
2. Assign appropriate tiers (Simple/Medium/Complex) based on complexity
3. Group independent tasks into parallel batches
4. CRITICAL: No two tasks in the same batch can modify the same file
5. Provide realistic cost estimates based on token usage
6. Return ONLY valid JSON, no markdown code blocks or explanations outside the JSON"#,
        task_description, context_section
    )
}

/// Prompt for regenerating a plan after validation failure
pub fn create_regeneration_prompt(
    task_description: &str,
    previous_analysis: &str,
    validation_error: &str,
    attempt: usize,
) -> String {
    format!(
        r#"## Task Description
{}

## Previous Analysis
{}

## Validation Error (Attempt {})
{}

Your previous plan failed validation. The error is above.

CRITICAL: You MUST ensure no two tasks in the same batch modify the same file. This is the #1 cause of validation failures.

When regenerating:
1. Identify which files were conflicted
2. Move conflicting tasks to different batches OR merge them into a single task
3. Update dependencies if batch ordering changes
4. Return ONLY valid JSON

Generate a corrected plan that passes validation."#,
        task_description, previous_analysis, attempt, validation_error
    )
}
