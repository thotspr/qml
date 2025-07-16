//! Retry logic and policies
//!
//! This module contains retry policies and strategies for handling failed jobs.

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

/// Retry strategy for failed jobs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RetryStrategy {
    /// No retry attempts
    None,
    /// Fixed interval between retries
    Fixed {
        /// Interval between retry attempts
        interval: Duration,
        /// Maximum number of retry attempts
        max_attempts: u32,
    },
    /// Exponential backoff with optional jitter
    ExponentialBackoff {
        /// Initial retry delay
        initial_delay: Duration,
        /// Multiplier for each subsequent retry
        multiplier: f64,
        /// Maximum delay between retries
        max_delay: Duration,
        /// Maximum number of retry attempts
        max_attempts: u32,
        /// Add random jitter to delays (helps avoid thundering herd)
        jitter: bool,
    },
    /// Linear backoff (delay increases linearly)
    LinearBackoff {
        /// Initial retry delay
        initial_delay: Duration,
        /// Amount to add to delay for each retry
        increment: Duration,
        /// Maximum delay between retries
        max_delay: Duration,
        /// Maximum number of retry attempts
        max_attempts: u32,
    },
}

impl Default for RetryStrategy {
    fn default() -> Self {
        Self::ExponentialBackoff {
            initial_delay: Duration::seconds(1),
            multiplier: 2.0,
            max_delay: Duration::minutes(15),
            max_attempts: 5,
            jitter: true,
        }
    }
}

impl RetryStrategy {
    /// Create a no-retry strategy
    pub fn none() -> Self {
        Self::None
    }

    /// Create a fixed interval retry strategy
    pub fn fixed(interval: Duration, max_attempts: u32) -> Self {
        Self::Fixed {
            interval,
            max_attempts,
        }
    }

    /// Create an exponential backoff strategy
    pub fn exponential_backoff(
        initial_delay: Duration,
        multiplier: f64,
        max_delay: Duration,
        max_attempts: u32,
    ) -> Self {
        Self::ExponentialBackoff {
            initial_delay,
            multiplier,
            max_delay,
            max_attempts,
            jitter: true,
        }
    }

    /// Create a linear backoff strategy
    pub fn linear_backoff(
        initial_delay: Duration,
        increment: Duration,
        max_delay: Duration,
        max_attempts: u32,
    ) -> Self {
        Self::LinearBackoff {
            initial_delay,
            increment,
            max_delay,
            max_attempts,
        }
    }

    /// Calculate the delay for the next retry attempt
    pub fn calculate_delay(&self, attempt: u32) -> Option<Duration> {
        match self {
            RetryStrategy::None => None,
            RetryStrategy::Fixed {
                interval,
                max_attempts,
            } => {
                if attempt <= *max_attempts {
                    Some(*interval)
                } else {
                    None
                }
            }
            RetryStrategy::ExponentialBackoff {
                initial_delay,
                multiplier,
                max_delay,
                max_attempts,
                jitter,
            } => {
                if attempt > *max_attempts {
                    return None;
                }

                let mut delay = initial_delay.num_milliseconds() as f64;
                for _ in 1..attempt {
                    delay *= multiplier;
                }

                // Apply maximum delay cap
                delay = delay.min(max_delay.num_milliseconds() as f64);

                // Add jitter if enabled (±25% of the delay)
                if *jitter {
                    let jitter_amount = delay * 0.25;
                    let random_factor = fastrand::f64() * 2.0 - 1.0; // Random between -1 and 1
                    delay += jitter_amount * random_factor;
                }

                Some(Duration::milliseconds(delay as i64))
            }
            RetryStrategy::LinearBackoff {
                initial_delay,
                increment,
                max_delay,
                max_attempts,
            } => {
                if attempt > *max_attempts {
                    return None;
                }

                let delay = *initial_delay + *increment * (attempt as i32 - 1);
                Some(delay.min(*max_delay))
            }
        }
    }

    /// Get the maximum number of retry attempts
    pub fn max_attempts(&self) -> u32 {
        match self {
            RetryStrategy::None => 0,
            RetryStrategy::Fixed { max_attempts, .. }
            | RetryStrategy::ExponentialBackoff { max_attempts, .. }
            | RetryStrategy::LinearBackoff { max_attempts, .. } => *max_attempts,
        }
    }

    /// Check if retries are enabled
    pub fn is_retry_enabled(&self) -> bool {
        !matches!(self, RetryStrategy::None)
    }
}

/// Retry policy that determines when and how to retry failed jobs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryPolicy {
    /// Retry strategy to use
    pub strategy: RetryStrategy,
    /// Whether to retry on all exceptions or only specific ones
    pub retry_on_all_exceptions: bool,
    /// Specific exception types to retry on (if retry_on_all_exceptions is false)
    pub retryable_exceptions: Vec<String>,
    /// Exception types that should never be retried
    pub non_retryable_exceptions: Vec<String>,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            strategy: RetryStrategy::default(),
            retry_on_all_exceptions: true,
            retryable_exceptions: vec![],
            non_retryable_exceptions: vec![
                "ArgumentError".to_string(),
                "ValidationError".to_string(),
                "AuthenticationError".to_string(),
                "AuthorizationError".to_string(),
            ],
        }
    }
}

impl RetryPolicy {
    /// Create a new retry policy with the specified strategy
    pub fn new(strategy: RetryStrategy) -> Self {
        Self {
            strategy,
            ..Default::default()
        }
    }

    /// Create a no-retry policy
    pub fn no_retry() -> Self {
        Self {
            strategy: RetryStrategy::None,
            retry_on_all_exceptions: false,
            retryable_exceptions: vec![],
            non_retryable_exceptions: vec![],
        }
    }

    /// Set whether to retry on all exceptions
    pub fn retry_on_all_exceptions(mut self, retry_all: bool) -> Self {
        self.retry_on_all_exceptions = retry_all;
        self
    }

    /// Add a retryable exception type
    pub fn add_retryable_exception(mut self, exception_type: String) -> Self {
        self.retryable_exceptions.push(exception_type);
        self
    }

    /// Add a non-retryable exception type
    pub fn add_non_retryable_exception(mut self, exception_type: String) -> Self {
        self.non_retryable_exceptions.push(exception_type);
        self
    }

    /// Check if a job should be retried based on the exception
    pub fn should_retry(&self, exception_type: Option<&str>, attempt: u32) -> bool {
        // Check if we've exceeded max attempts
        if attempt > self.strategy.max_attempts() {
            return false;
        }

        // Check if retries are disabled
        if !self.strategy.is_retry_enabled() {
            return false;
        }

        // Check if exception type is provided
        let exception_type = match exception_type {
            Some(ex) => ex,
            None => return self.retry_on_all_exceptions, // No exception info, use default policy
        };

        // Check if this exception type is explicitly non-retryable
        if self
            .non_retryable_exceptions
            .contains(&exception_type.to_string())
        {
            return false;
        }

        // If we retry on all exceptions, allow retry (unless it was in non-retryable list)
        if self.retry_on_all_exceptions {
            return true;
        }

        // Otherwise, only retry if it's in the retryable list
        self.retryable_exceptions
            .contains(&exception_type.to_string())
    }

    /// Calculate the next retry time
    pub fn calculate_retry_time(&self, attempt: u32) -> Option<DateTime<Utc>> {
        self.strategy
            .calculate_delay(attempt)
            .map(|delay| Utc::now() + delay)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exponential_backoff_calculation() {
        let strategy = RetryStrategy::ExponentialBackoff {
            initial_delay: Duration::seconds(1),
            multiplier: 2.0,
            max_delay: Duration::minutes(5),
            max_attempts: 3,
            jitter: false, // Disable jitter for predictable testing
        };

        // First retry
        let delay1 = strategy.calculate_delay(1).unwrap();
        assert!(delay1.num_seconds() >= 1);

        // Second retry should be roughly 2x the first
        let delay2 = strategy.calculate_delay(2).unwrap();
        assert!(delay2.num_seconds() >= 2);

        // Fourth retry should return None (exceeds max attempts)
        assert!(strategy.calculate_delay(4).is_none());
    }

    #[test]
    fn test_fixed_retry_calculation() {
        let strategy = RetryStrategy::fixed(Duration::seconds(5), 2);

        let delay1 = strategy.calculate_delay(1).unwrap();
        assert_eq!(delay1.num_seconds(), 5);

        let delay2 = strategy.calculate_delay(2).unwrap();
        assert_eq!(delay2.num_seconds(), 5);

        // Third retry should return None
        assert!(strategy.calculate_delay(3).is_none());
    }

    #[test]
    fn test_retry_policy_should_retry() {
        let policy = RetryPolicy::default();

        // Should retry on first attempt with any exception
        assert!(policy.should_retry(Some("NetworkError"), 1));

        // Should not retry on non-retryable exception
        assert!(!policy.should_retry(Some("ArgumentError"), 1));

        // Should not retry after max attempts
        assert!(!policy.should_retry(Some("NetworkError"), 10));
    }

    #[test]
    fn test_linear_backoff_calculation() {
        let strategy = RetryStrategy::linear_backoff(
            Duration::seconds(1),
            Duration::seconds(2),
            Duration::minutes(1),
            3,
        );

        let delay1 = strategy.calculate_delay(1).unwrap();
        assert_eq!(delay1.num_seconds(), 1); // 1 + 2*(1-1) = 1

        let delay2 = strategy.calculate_delay(2).unwrap();
        assert_eq!(delay2.num_seconds(), 3); // 1 + 2*(2-1) = 3

        let delay3 = strategy.calculate_delay(3).unwrap();
        assert_eq!(delay3.num_seconds(), 5); // 1 + 2*(3-1) = 5
    }
}
