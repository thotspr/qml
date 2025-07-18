//! Job scheduler for delayed and recurring jobs
//!
//! This module contains the JobScheduler that handles scheduling jobs for
//! future execution and managing recurring job patterns.

use chrono::{DateTime, Duration, Utc};
use std::sync::Arc;
use tokio::time::interval;
use tracing::{debug, error, info};

use crate::core::{Job, JobState};
use crate::error::{QmlError, Result};
use crate::storage::Storage;

/// Job scheduler for managing delayed and recurring jobs
pub struct JobScheduler {
    storage: Arc<dyn Storage>,
    poll_interval: Duration,
}

impl JobScheduler {
    /// Create a new job scheduler
    pub fn new(storage: Arc<dyn Storage>) -> Self {
        Self {
            storage,
            poll_interval: Duration::seconds(30), // Check every 30 seconds by default
        }
    }

    /// Create a new job scheduler with custom poll interval
    pub fn with_poll_interval(storage: Arc<dyn Storage>, poll_interval: Duration) -> Self {
        Self {
            storage,
            poll_interval,
        }
    }

    /// Start the scheduler loop
    pub async fn run(&self) -> Result<()> {
        info!(
            "Starting job scheduler with poll interval: {:?}",
            self.poll_interval
        );

        let mut interval =
            interval(
                self.poll_interval
                    .to_std()
                    .map_err(|e| QmlError::ConfigurationError {
                        message: format!("Invalid poll interval: {}", e),
                    })?,
            );

        loop {
            interval.tick().await;

            if let Err(e) = self.process_scheduled_jobs().await {
                error!("Error processing scheduled jobs: {}", e);
            }

            if let Err(e) = self.process_retry_jobs().await {
                error!("Error processing retry jobs: {}", e);
            }
        }
    }

    /// Process jobs that are scheduled for execution
    async fn process_scheduled_jobs(&self) -> Result<()> {
        debug!("Checking for scheduled jobs ready for execution");

        let now = Utc::now();

        // Get all scheduled jobs
        let scheduled_state = JobState::scheduled(now, "check");
        let jobs = self
            .storage
            .list(Some(&scheduled_state), None, None)
            .await
            .map_err(|e| QmlError::StorageError {
                message: format!("Failed to list scheduled jobs: {}", e),
            })?;

        let mut ready_jobs = Vec::new();

        for job in jobs {
            if let JobState::Scheduled { enqueue_at, .. } = &job.state {
                if *enqueue_at <= now {
                    ready_jobs.push(job);
                }
            }
        }

        debug!(
            "Found {} scheduled jobs ready for execution",
            ready_jobs.len()
        );

        // Enqueue ready jobs
        for mut job in ready_jobs {
            info!("Enqueueing scheduled job: {}", job.id);

            let enqueued_state = JobState::enqueued(&job.queue);
            if let Err(e) = job.set_state(enqueued_state) {
                error!("Failed to set job state to Enqueued: {}", e);
                continue;
            }

            if let Err(e) = self.storage.update(&job).await {
                error!("Failed to update job in storage: {}", e);
            }
        }

        Ok(())
    }

    /// Process jobs that are awaiting retry
    async fn process_retry_jobs(&self) -> Result<()> {
        debug!("Checking for jobs ready for retry");

        let now = Utc::now();

        // Get all jobs awaiting retry
        let retry_state = JobState::awaiting_retry(now, 1, "check");
        let jobs = self
            .storage
            .list(Some(&retry_state), None, None)
            .await
            .map_err(|e| QmlError::StorageError {
                message: format!("Failed to list retry jobs: {}", e),
            })?;

        let mut ready_jobs = Vec::new();

        for job in jobs {
            if let JobState::AwaitingRetry { retry_at, .. } = &job.state {
                if *retry_at <= now {
                    ready_jobs.push(job);
                }
            }
        }

        debug!("Found {} retry jobs ready for execution", ready_jobs.len());

        // Enqueue ready retry jobs
        for mut job in ready_jobs {
            info!("Enqueueing retry job: {}", job.id);

            let enqueued_state = JobState::enqueued(&job.queue);
            if let Err(e) = job.set_state(enqueued_state) {
                error!("Failed to set job state to Enqueued: {}", e);
                continue;
            }

            if let Err(e) = self.storage.update(&job).await {
                error!("Failed to update job in storage: {}", e);
            }
        }

        Ok(())
    }

    /// Schedule a job for future execution
    pub async fn schedule_job(
        &self,
        mut job: Job,
        execute_at: DateTime<Utc>,
        reason: impl Into<String>,
    ) -> Result<()> {
        let scheduled_state = JobState::scheduled(execute_at, reason);

        job.set_state(scheduled_state)?;

        self.storage
            .enqueue(&job)
            .await
            .map_err(|e| QmlError::StorageError {
                message: format!("Failed to schedule job: {}", e),
            })?;

        info!("Scheduled job {} for execution at {}", job.id, execute_at);
        Ok(())
    }

    /// Schedule a job with a delay from now
    pub async fn schedule_job_in(
        &self,
        job: Job,
        delay: Duration,
        reason: impl Into<String>,
    ) -> Result<()> {
        let execute_at = Utc::now() + delay;
        self.schedule_job(job, execute_at, reason).await
    }

    /// Get the current poll interval
    pub fn poll_interval(&self) -> Duration {
        self.poll_interval
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::MemoryStorage;

    #[tokio::test]
    async fn test_schedule_job() {
        let storage = Arc::new(MemoryStorage::new());
        let scheduler = JobScheduler::new(storage.clone());

        let job = Job::new("test_method", vec!["arg1".to_string()]);
        let job_id = job.id.clone();
        let execute_at = Utc::now() + Duration::seconds(1);

        scheduler
            .schedule_job(job, execute_at, "test")
            .await
            .unwrap();

        // Check that the job is scheduled
        let stored_job = storage.get(&job_id).await.unwrap().unwrap();
        assert!(matches!(stored_job.state, JobState::Scheduled { .. }));
    }

    #[tokio::test]
    async fn test_process_scheduled_jobs() {
        let storage = Arc::new(MemoryStorage::new());
        let scheduler = JobScheduler::new(storage.clone());

        // Create a job scheduled for immediate execution
        let job = Job::new("test_method", vec!["arg1".to_string()]);
        let job_id = job.id.clone();
        let execute_at = Utc::now() - Duration::seconds(1); // Past time

        scheduler
            .schedule_job(job, execute_at, "test")
            .await
            .unwrap();

        // Process scheduled jobs
        scheduler.process_scheduled_jobs().await.unwrap();

        // Check that the job is now enqueued
        let updated_job = storage.get(&job_id).await.unwrap().unwrap();
        assert!(matches!(updated_job.state, JobState::Enqueued { .. }));
    }

    #[tokio::test]
    async fn test_schedule_job_in() {
        let storage = Arc::new(MemoryStorage::new());
        let scheduler = JobScheduler::new(storage.clone());

        let job = Job::new("test_method", vec!["arg1".to_string()]);
        let job_id = job.id.clone();

        scheduler
            .schedule_job_in(job, Duration::minutes(5), "delayed")
            .await
            .unwrap();

        // Check that the job is scheduled
        let stored_job = storage.get(&job_id).await.unwrap().unwrap();
        if let JobState::Scheduled { enqueue_at, .. } = stored_job.state {
            assert!(enqueue_at > Utc::now());
        } else {
            panic!("Job should be in Scheduled state");
        }
    }
}
