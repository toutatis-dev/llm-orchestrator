use crate::config::Config;
use crate::core::{Task, WorkerTier};
use anyhow::Result;
use std::time::Duration;
use tokio::time::sleep;

/// Result of executing a task with retry logic
#[derive(Debug, Clone)]
pub struct RetryResult<T> {
    pub result: Option<T>,
    pub attempts: usize,
    pub escalated: bool,
    pub error: Option<String>,
}

/// Handles task execution with automatic retry and tier escalation
/// 
/// When a task fails, the retry handler will:
/// 1. Retry with the same tier (up to max_retries)
/// 2. If all retries fail and escalate_on_retry is enabled, retry with next tier
/// 3. If all tiers fail, return the error
pub struct RetryHandler {
    config: RetryConfig,
}

#[derive(Debug, Clone)]
pub struct RetryConfig {
    pub max_retries: usize,
    pub escalate_on_retry: bool,
    pub retry_delay_ms: u64,
    pub backoff_multiplier: f64,
    pub max_delay_ms: u64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 1,
            escalate_on_retry: true,
            retry_delay_ms: 1000,
            backoff_multiplier: 2.0,
            max_delay_ms: 30000,
        }
    }
}

impl RetryHandler {
    /// Create a new RetryHandler from config
    pub fn from_config(config: &Config) -> Self {
        let retry_config = RetryConfig {
            max_retries: config.general.max_retries,
            escalate_on_retry: config.general.escalate_on_retry,
            retry_delay_ms: config.rate_limit.initial_backoff_ms,
            backoff_multiplier: config.rate_limit.multiplier,
            max_delay_ms: config.rate_limit.max_backoff_ms,
        };

        Self {
            config: retry_config,
        }
    }

    /// Create a new RetryHandler with custom config
    pub fn with_config(config: RetryConfig) -> Self {
        Self { config }
    }

    /// Execute a task with retry and escalation logic
    pub async fn execute_with_retry<F, Fut, T>(
        &self,
        task: &Task,
        operation: F,
    ) -> RetryResult<T>
    where
        F: Fn(&Task, WorkerTier) -> Fut,
        Fut: std::future::Future<Output = Result<T, anyhow::Error>>,
    {
        let mut current_tier = task.tier;
        let mut escalated = false;

        // Try each tier starting from the assigned one
        loop {
            // Attempt with current tier
            let result = self
                .execute_with_backoff(task, current_tier, &operation)
                .await;

            match result {
                Ok(value) => {
                    return RetryResult {
                        result: Some(value),
                        attempts: 0, // Will be set by execute_with_backoff
                        escalated,
                        error: None,
                    };
                }
                Err((attempts, error)) => {
                    tracing::warn!(
                        "Task {} failed after {} attempts at tier {:?}: {}",
                        task.id,
                        attempts,
                        current_tier,
                        error
                    );

                    // Check if we should escalate
                    if self.config.escalate_on_retry {
                        if let Some(next_tier) = current_tier.next_tier() {
                            tracing::info!(
                                "Escalating task {} from {:?} to {:?}",
                                task.id,
                                current_tier,
                                next_tier
                            );
                            current_tier = next_tier;
                            escalated = true;
                            continue;
                        }
                    }

                    // No more tiers to try
                    return RetryResult {
                        result: None,
                        attempts,
                        escalated,
                        error: Some(error.to_string()),
                    };
                }
            }
        }
    }

    /// Execute a task with exponential backoff retries
    async fn execute_with_backoff<F, Fut, T>(
        &self,
        task: &Task,
        tier: WorkerTier,
        operation: &F,
    ) -> std::result::Result<T, (usize, anyhow::Error)>
    where
        F: Fn(&Task, WorkerTier) -> Fut,
        Fut: std::future::Future<Output = Result<T, anyhow::Error>>,
    {
        let mut delay_ms = self.config.retry_delay_ms;

        for attempt in 0..=self.config.max_retries {
            tracing::debug!(
                "Executing task {} (attempt {}/{}, tier {:?})",
                task.id,
                attempt + 1,
                self.config.max_retries + 1,
                tier
            );

            match operation(task, tier).await {
                Ok(result) => {
                    tracing::info!(
                        "Task {} succeeded on attempt {} (tier {:?})",
                        task.id,
                        attempt + 1,
                        tier
                    );
                    return Ok(result);
                }
                Err(e) => {
                    if attempt < self.config.max_retries {
                        tracing::warn!(
                            "Task {} failed on attempt {} (tier {:?}): {}. Retrying in {}ms...",
                            task.id,
                            attempt + 1,
                            tier,
                            e,
                            delay_ms
                        );

                        // Wait with exponential backoff
                        sleep(Duration::from_millis(delay_ms)).await;

                        // Calculate next delay
                        delay_ms = ((delay_ms as f64 * self.config.backoff_multiplier) as u64)
                            .min(self.config.max_delay_ms);
                    } else {
                        tracing::error!(
                            "Task {} failed after {} attempts (tier {:?}): {}",
                            task.id,
                            attempt + 1,
                            tier,
                            e
                        );
                        return Err((attempt + 1, e));
                    }
                }
            }
        }

        unreachable!()
    }

    /// Get the retry configuration
    pub fn config(&self) -> &RetryConfig {
        &self.config
    }
}

/// Error classification for better handling
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorType {
    /// Transient error that may succeed on retry (e.g., network timeout)
    Transient,
    /// Permanent error that won't succeed on retry (e.g., invalid API key)
    Permanent,
    /// Rate limit error that requires backoff
    RateLimited,
    /// Conflict error (e.g., merge conflict)
    Conflict,
    /// Unknown error type
    Unknown,
}

impl ErrorType {
    /// Classify an error based on its content
    pub fn classify(error: &anyhow::Error) -> Self {
        let error_str = error.to_string().to_lowercase();

        if error_str.contains("rate limit")
            || error_str.contains("429")
            || error_str.contains("too many requests")
        {
            ErrorType::RateLimited
        } else if error_str.contains("timeout")
            || error_str.contains("connection")
            || error_str.contains("network")
        {
            ErrorType::Transient
        } else if error_str.contains("unauthorized")
            || error_str.contains("forbidden")
            || error_str.contains("invalid")
            || error_str.contains("not found")
        {
            ErrorType::Permanent
        } else if error_str.contains("conflict") || error_str.contains("merge") {
            ErrorType::Conflict
        } else {
            ErrorType::Unknown
        }
    }

    /// Check if this error type is retryable
    pub fn is_retryable(&self) -> bool {
        matches!(self, ErrorType::Transient | ErrorType::RateLimited | ErrorType::Unknown)
    }
}

/// User intervention request for errors that require manual handling
#[derive(Debug, Clone)]
pub struct InterventionRequest {
    pub task_id: String,
    pub error: String,
    pub error_type: ErrorType,
    pub suggested_action: String,
    pub context: Option<String>,
}

impl InterventionRequest {
    /// Print intervention instructions to stdout
    pub fn print_instructions(&self) {
        println!("╔════════════════════════════════════════════════════════════╗");
        println!("║              USER INTERVENTION REQUIRED                    ║");
        println!("╠════════════════════════════════════════════════════════════╣");
        println!("║  Task:             {:<40} ║", self.task_id);
        println!("║  Error Type:       {:<40} ║", format!("{:?}", self.error_type));
        println!("╚════════════════════════════════════════════════════════════╝");
        println!();
        println!("Error: {}", self.error);
        println!();
        println!("Suggested Action: {}", self.suggested_action);
        if let Some(ctx) = &self.context {
            println!();
            println!("Context: {}", ctx);
        }
        println!();
        println!("Options:");
        println!("  [r] Retry the task");
        println!("  [s] Skip this task");
        println!("  [a] Abort execution");
        println!("  [e] Edit task and retry");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_classification() {
        assert_eq!(
            ErrorType::classify(&anyhow::anyhow!("rate limit exceeded")),
            ErrorType::RateLimited
        );
        assert_eq!(
            ErrorType::classify(&anyhow::anyhow!("connection timeout")),
            ErrorType::Transient
        );
        assert_eq!(
            ErrorType::classify(&anyhow::anyhow!("unauthorized"))),
            ErrorType::Permanent
        );
    }

    #[test]
    fn test_retryable_errors() {
        assert!(ErrorType::Transient.is_retryable());
        assert!(ErrorType::RateLimited.is_retryable());
        assert!(!ErrorType::Permanent.is_retryable());
        assert!(ErrorType::Conflict.is_retryable());
    }
}
