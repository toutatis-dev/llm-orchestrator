use std::future::Future;
use std::time::Duration;
use tokio::time::sleep;
use rand::Rng;

/// Rate limiter with exponential backoff and jitter
/// 
/// This handles rate limiting from the OpenRouter API and other
/// external services. It implements exponential backoff with
/// randomized jitter to prevent thundering herd problems.
pub struct RateLimiter {
    config: RateLimitConfig,
}

#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    /// Maximum number of retries before giving up
    pub max_retries: usize,
    /// Initial backoff duration in milliseconds
    pub initial_backoff_ms: u64,
    /// Maximum backoff duration in milliseconds
    pub max_backoff_ms: u64,
    /// Multiplier for exponential backoff
    pub multiplier: f64,
    /// Jitter factor (0.0 to 1.0) - adds randomness to backoff
    pub jitter: f64,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_backoff_ms: 1000,
            max_backoff_ms: 60000,
            multiplier: 2.0,
            jitter: 0.25,
        }
    }
}

impl RateLimiter {
    /// Create a new RateLimiter with default config
    pub fn new() -> Self {
        Self {
            config: RateLimitConfig::default(),
        }
    }

    /// Create a new RateLimiter with custom config
    pub fn with_config(config: RateLimitConfig) -> Self {
        Self { config }
    }

    /// Execute an operation with rate limit handling
    /// 
    /// If the operation returns RateLimitError, this will retry with
    /// exponential backoff and jitter. Other errors are returned immediately.
    pub async fn execute<F, Fut, T>(&self, operation: F) -> Result<T, RateLimitError>
    where
        F: Fn() -> Fut,
        Fut: Future<Output = Result<T, RateLimitError>>,
    {
        let mut backoff_ms = self.config.initial_backoff_ms;

        for attempt in 0..=self.config.max_retries {
            match operation().await {
                Ok(result) => return Ok(result),
                Err(RateLimitError::RateLimited { retry_after }) => {
                    if attempt >= self.config.max_retries {
                        return Err(RateLimitError::MaxRetriesExceeded);
                    }

                    // Calculate wait time
                    let wait_ms = retry_after
                        .map(|d| d.as_millis() as u64)
                        .unwrap_or(backoff_ms);

                    // Add jitter
                    let jittered_wait = self.add_jitter(wait_ms);

                    tracing::warn!(
                        "Rate limited (attempt {}/{}). Waiting {}ms (with jitter)...",
                        attempt + 1,
                        self.config.max_retries + 1,
                        jittered_wait
                    );

                    sleep(Duration::from_millis(jittered_wait)).await;

                    // Exponential backoff
                    backoff_ms = ((backoff_ms as f64 * self.config.multiplier) as u64)
                        .min(self.config.max_backoff_ms);
                }
                Err(e) => return Err(e),
            }
        }

        Err(RateLimitError::MaxRetriesExceeded)
    }

    /// Execute an operation that returns a standard Result
    /// 
    /// This wraps the operation to convert errors into RateLimitError
    pub async fn execute_with_conversion<F, Fut, T, E>(
        &self,
        operation: F,
        is_rate_limited: impl Fn(&E) -> bool,
    ) -> Result<T, RateLimitError>
    where
        F: Fn() -> Fut,
        Fut: Future<Output = Result<T, E>>,
        E: std::fmt::Display,
    {
        self.execute(|| async {
            match operation().await {
                Ok(result) => Ok(result),
                Err(e) => {
                    if is_rate_limited(&e) {
                        Err(RateLimitError::RateLimited { retry_after: None })
                    } else {
                        Err(RateLimitError::Other(e.to_string()))
                    }
                }
            }
        })
        .await
    }

    /// Add jitter to a duration
    fn add_jitter(&self, duration_ms: u64) -> u64 {
        if self.config.jitter <= 0.0 {
            return duration_ms;
        }

        let jitter_range = duration_ms as f64 * self.config.jitter;
        let jitter = rand::thread_rng().gen_range(-jitter_range..=jitter_range);
        let result = (duration_ms as f64 + jitter) as u64;
        result.max(0)
    }

    /// Get the current config
    pub fn config(&self) -> &RateLimitConfig {
        &self.config
    }

    /// Calculate backoff for a specific attempt
    pub fn calculate_backoff(&self, attempt: usize) -> u64 {
        let backoff = self.config.initial_backoff_ms as f64
            * self.config.multiplier.powi(attempt as i32);
        let backoff = backoff.min(self.config.max_backoff_ms as f64) as u64;
        self.add_jitter(backoff)
    }
}

impl Default for RateLimiter {
    fn default() -> Self {
        Self::new()
    }
}

/// Errors that can occur during rate-limited operations
#[derive(Debug, Clone)]
pub enum RateLimitError {
    /// Rate limited by the service
    RateLimited { retry_after: Option<Duration> },
    /// Maximum retries exceeded
    MaxRetriesExceeded,
    /// Other error (converted from operation error)
    Other(String),
}

impl std::fmt::Display for RateLimitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RateLimitError::RateLimited { retry_after } => {
                if let Some(duration) = retry_after {
                    write!(f, "Rate limited. Retry after {:?}", duration)
                } else {
                    write!(f, "Rate limited")
                }
            }
            RateLimitError::MaxRetriesExceeded => {
                write!(f, "Maximum retries exceeded")
            }
            RateLimitError::Other(msg) => {
                write!(f, "{}", msg)
            }
        }
    }
}

impl std::error::Error for RateLimitError {}

/// Token bucket rate limiter for client-side rate limiting
/// 
/// This can be used to proactively limit requests to stay within
/// API rate limits.
pub struct TokenBucket {
    capacity: usize,
    tokens: usize,
    refill_rate: f64, // tokens per second
    last_refill: std::time::Instant,
}

impl TokenBucket {
    /// Create a new token bucket
    pub fn new(capacity: usize, refill_rate: f64) -> Self {
        Self {
            capacity,
            tokens: capacity,
            refill_rate,
            last_refill: std::time::Instant::now(),
        }
    }

    /// Try to consume tokens from the bucket
    /// 
    /// Returns the number of tokens consumed (0 if not enough available)
    pub fn try_consume(&mut self, amount: usize) -> usize {
        self.refill();

        if self.tokens >= amount {
            self.tokens -= amount;
            amount
        } else {
            0
        }
    }

    /// Consume tokens, waiting if necessary
    pub async fn consume(&mut self, amount: usize) {
        loop {
            let consumed = self.try_consume(amount);
            if consumed == amount {
                return;
            }

            // Wait for tokens to refill
            let needed = amount - self.tokens;
            let wait_secs = needed as f64 / self.refill_rate;
            sleep(Duration::from_secs_f64(wait_secs)).await;
        }
    }

    /// Refill tokens based on elapsed time
    fn refill(&mut self) {
        let now = std::time::Instant::now();
        let elapsed = now.duration_since(self.last_refill).as_secs_f64();
        let tokens_to_add = (elapsed * self.refill_rate) as usize;

        self.tokens = (self.tokens + tokens_to_add).min(self.capacity);
        self.last_refill = now;
    }

    /// Get current token count
    pub fn available_tokens(&mut self) -> usize {
        self.refill();
        self.tokens
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rate_limiter_config() {
        let config = RateLimitConfig::default();
        let limiter = RateLimiter::with_config(config.clone());

        assert_eq!(limiter.config().max_retries, config.max_retries);
    }

    #[test]
    fn test_jitter_calculation() {
        let config = RateLimitConfig {
            jitter: 0.25,
            ..Default::default()
        };
        let limiter = RateLimiter::with_config(config);

        let base = 1000;
        let jittered = limiter.add_jitter(base);

        // Jittered value should be within 25% of base
        assert!(jittered >= 750 && jittered <= 1250);
    }

    #[test]
    fn test_backoff_calculation() {
        let limiter = RateLimiter::new();

        // Attempt 0 should be close to initial
        let backoff0 = limiter.calculate_backoff(0);
        assert!(backoff0 >= 750 && backoff0 <= 1250); // With jitter

        // Attempt 1 should be 2x
        let backoff1 = limiter.calculate_backoff(1);
        assert!(backoff1 >= 1500 && backoff1 <= 2500);

        // Attempt 2 should be 4x
        let backoff2 = limiter.calculate_backoff(2);
        assert!(backoff2 >= 3000 && backoff2 <= 5000);
    }

    #[test]
    fn test_token_bucket() {
        let mut bucket = TokenBucket::new(10, 1.0); // 10 tokens, 1 per second

        assert_eq!(bucket.try_consume(5), 5);
        assert_eq!(bucket.available_tokens(), 5);

        assert_eq!(bucket.try_consume(10), 0); // Not enough tokens
        assert_eq!(bucket.available_tokens(), 5);
    }

    #[tokio::test]
    async fn test_rate_limit_retry() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;

        let limiter = RateLimiter::with_config(RateLimitConfig {
            max_retries: 2,
            initial_backoff_ms: 10,
            ..Default::default()
        });

        let attempts = Arc::new(AtomicUsize::new(0));
        let attempts_clone = attempts.clone();
        let result: Result<i32, RateLimitError> = limiter
            .execute(move || {
                let attempts = attempts_clone.clone();
                async move {
                    let count = attempts.fetch_add(1, Ordering::SeqCst) + 1;
                    if count < 3 {
                        Err(RateLimitError::RateLimited { retry_after: None })
                    } else {
                        Ok(42)
                    }
                }
            })
            .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 42);
        assert_eq!(attempts.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn test_rate_limit_max_retries() {
        let limiter = RateLimiter::with_config(RateLimitConfig {
            max_retries: 2,
            initial_backoff_ms: 1,
            ..Default::default()
        });

        let result: Result<i32, RateLimitError> = limiter
            .execute(|| async { Err::<i32, _>(RateLimitError::RateLimited { retry_after: None }) })
            .await;

        assert!(matches!(result, Err(RateLimitError::MaxRetriesExceeded)));
    }
}
