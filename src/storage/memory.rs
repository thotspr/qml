use async_trait::async_trait;
use chrono::Utc;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};

use super::{MemoryConfig, Storage, StorageError};
use crate::core::{Job, JobState};

/// Job lock information for MemoryStorage
#[derive(Debug, Clone)]
struct JobLock {
    worker_id: String,
    expires_at: chrono::DateTime<chrono::Utc>,
}

/// In-memory storage implementation for jobs
///
/// This storage keeps all jobs in memory using a HashMap with RwLock for thread safety.
/// It's primarily intended for development, testing, and simple scenarios where persistence
/// is not required.
#[derive(Debug)]
pub struct MemoryStorage {
    jobs: RwLock<HashMap<String, Job>>,
    locks: Arc<Mutex<HashMap<String, JobLock>>>,
    config: MemoryConfig,
}

impl MemoryStorage {
    /// Create a new memory storage with default configuration
    pub fn new() -> Self {
        Self::with_config(MemoryConfig::default())
    }

    /// Create a new memory storage with the specified configuration
    pub fn with_config(config: MemoryConfig) -> Self {
        Self {
            jobs: RwLock::new(HashMap::new()),
            locks: Arc::new(Mutex::new(HashMap::new())),
            config,
        }
    }

    /// Get the number of jobs currently stored
    pub fn len(&self) -> usize {
        self.jobs.read().unwrap().len()
    }

    /// Check if the storage is empty
    pub fn is_empty(&self) -> bool {
        self.jobs.read().unwrap().is_empty()
    }

    /// Clear all jobs from storage
    pub fn clear(&self) {
        self.jobs.write().unwrap().clear();
    }

    /// Check if we've exceeded the maximum job limit
    fn is_at_capacity(&self) -> bool {
        if let Some(max_jobs) = self.config.max_jobs {
            self.len() >= max_jobs
        } else {
            false
        }
    }

    /// Remove completed jobs if auto-cleanup is enabled
    fn maybe_cleanup(&self) {
        if !self.config.auto_cleanup {
            return;
        }

        let mut jobs = self.jobs.write().unwrap();
        jobs.retain(|_, job| {
            !matches!(
                job.state,
                JobState::Succeeded { .. } | JobState::Deleted { .. }
            )
        });
    }

    /// Filter jobs by state
    fn filter_jobs_by_state(jobs: &HashMap<String, Job>, state: &JobState) -> Vec<Job> {
        jobs.values()
            .filter(|job| std::mem::discriminant(&job.state) == std::mem::discriminant(state))
            .cloned()
            .collect()
    }

    /// Get jobs available for processing (enqueued, scheduled for now, or awaiting retry)
    fn get_available_jobs_internal(jobs: &HashMap<String, Job>, limit: Option<usize>) -> Vec<Job> {
        let now = Utc::now();
        let mut available_jobs: Vec<Job> = jobs
            .values()
            .filter(|job| match &job.state {
                JobState::Enqueued { .. } => true,
                JobState::Scheduled { enqueue_at, .. } => *enqueue_at <= now,
                JobState::AwaitingRetry { retry_at, .. } => *retry_at <= now,
                _ => false,
            })
            .cloned()
            .collect();

        // Sort by priority (higher priority first), then by creation time (older first)
        available_jobs.sort_by(|a, b| {
            b.priority
                .cmp(&a.priority)
                .then_with(|| a.created_at.cmp(&b.created_at))
        });

        if let Some(limit) = limit {
            available_jobs.truncate(limit);
        }

        available_jobs
    }
}

impl Default for MemoryStorage {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Storage for MemoryStorage {
    async fn enqueue(&self, job: &Job) -> Result<(), StorageError> {
        // Check capacity before adding
        if self.is_at_capacity() {
            return Err(StorageError::capacity_exceeded(format!(
                "Memory storage is at capacity ({} jobs)",
                self.len()
            )));
        }

        // Perform cleanup if enabled
        self.maybe_cleanup();

        // Store the job
        let mut jobs = self.jobs.write().unwrap();
        jobs.insert(job.id.clone(), job.clone());

        Ok(())
    }

    async fn get(&self, job_id: &str) -> Result<Option<Job>, StorageError> {
        let jobs = self.jobs.read().unwrap();
        Ok(jobs.get(job_id).cloned())
    }

    async fn update(&self, job: &Job) -> Result<(), StorageError> {
        let mut jobs = self.jobs.write().unwrap();

        if jobs.contains_key(&job.id) {
            jobs.insert(job.id.clone(), job.clone());
            Ok(())
        } else {
            Err(StorageError::job_not_found(job.id.clone()))
        }
    }

    async fn delete(&self, job_id: &str) -> Result<bool, StorageError> {
        let mut jobs = self.jobs.write().unwrap();
        Ok(jobs.remove(job_id).is_some())
    }

    async fn list(
        &self,
        state_filter: Option<&JobState>,
        limit: Option<usize>,
        offset: Option<usize>,
    ) -> Result<Vec<Job>, StorageError> {
        let jobs = self.jobs.read().unwrap();

        let mut filtered_jobs: Vec<Job> = if let Some(state) = state_filter {
            Self::filter_jobs_by_state(&jobs, state)
        } else {
            jobs.values().cloned().collect()
        };

        // Sort by creation time (newest first)
        filtered_jobs.sort_by(|a, b| b.created_at.cmp(&a.created_at));

        // Apply offset and limit
        let start = offset.unwrap_or(0);
        let end = if let Some(limit) = limit {
            std::cmp::min(start + limit, filtered_jobs.len())
        } else {
            filtered_jobs.len()
        };

        if start >= filtered_jobs.len() {
            Ok(vec![])
        } else {
            Ok(filtered_jobs[start..end].to_vec())
        }
    }

    async fn get_job_counts(&self) -> Result<HashMap<JobState, usize>, StorageError> {
        let jobs = self.jobs.read().unwrap();
        let mut counts = HashMap::new();

        for job in jobs.values() {
            let key = match &job.state {
                JobState::Enqueued { .. } => JobState::enqueued(""),
                JobState::Processing { .. } => JobState::processing("", ""),
                JobState::Succeeded { .. } => JobState::succeeded(0, None),
                JobState::Failed { .. } => JobState::failed("", None, 0),
                JobState::Deleted { .. } => JobState::deleted(None),
                JobState::Scheduled { .. } => JobState::scheduled(Utc::now(), ""),
                JobState::AwaitingRetry { .. } => JobState::awaiting_retry(Utc::now(), 0, ""),
            };
            *counts.entry(key).or_insert(0) += 1;
        }

        Ok(counts)
    }

    async fn get_available_jobs(&self, limit: Option<usize>) -> Result<Vec<Job>, StorageError> {
        let jobs = self.jobs.read().unwrap();
        Ok(Self::get_available_jobs_internal(&jobs, limit))
    }

    async fn fetch_and_lock_job(
        &self,
        worker_id: &str,
        queues: Option<&[String]>,
    ) -> Result<Option<Job>, StorageError> {
        // Clean up expired locks first
        self.cleanup_expired_locks();

        let mut jobs = self.jobs.write().unwrap();
        let mut locks = self.locks.lock().unwrap();

        // Find an available job that's not locked
        let available_jobs = Self::get_available_jobs_internal(&jobs, None);

        for mut job in available_jobs {
            // Check if job matches queue filter
            if let Some(queues) = queues {
                if !queues.is_empty() && !queues.contains(&job.queue) {
                    continue;
                }
            }

            // Check if job is already locked
            if locks.contains_key(&job.id) {
                continue;
            }

            // Lock the job and mark as processing
            let lock = JobLock {
                worker_id: worker_id.to_string(),
                expires_at: chrono::Utc::now() + chrono::Duration::minutes(30), // 30 minute default timeout
            };
            locks.insert(job.id.clone(), lock);

            // Update job state
            job.state = JobState::Processing {
                worker_id: worker_id.to_string(),
                started_at: chrono::Utc::now(),
                server_name: "memory-storage".to_string(),
            };

            // Store updated job
            jobs.insert(job.id.clone(), job.clone());

            return Ok(Some(job));
        }

        Ok(None)
    }

    async fn try_acquire_job_lock(
        &self,
        job_id: &str,
        worker_id: &str,
        timeout_seconds: u64,
    ) -> Result<bool, StorageError> {
        self.cleanup_expired_locks();

        let mut locks = self.locks.lock().unwrap();

        // Check if job is already locked
        if locks.contains_key(job_id) {
            return Ok(false);
        }

        // Acquire the lock
        let lock = JobLock {
            worker_id: worker_id.to_string(),
            expires_at: chrono::Utc::now() + chrono::Duration::seconds(timeout_seconds as i64),
        };
        locks.insert(job_id.to_string(), lock);

        Ok(true)
    }

    async fn release_job_lock(&self, job_id: &str, worker_id: &str) -> Result<bool, StorageError> {
        let mut locks = self.locks.lock().unwrap();

        if let Some(lock) = locks.get(job_id) {
            if lock.worker_id == worker_id {
                locks.remove(job_id);
                return Ok(true);
            }
        }

        Ok(false)
    }

    async fn fetch_available_jobs_atomic(
        &self,
        worker_id: &str,
        limit: Option<usize>,
        queues: Option<&[String]>,
    ) -> Result<Vec<Job>, StorageError> {
        let mut jobs = Vec::new();
        let fetch_limit = limit.unwrap_or(10).min(100); // Cap at 100 jobs

        // Fetch jobs one by one to ensure proper locking
        for _ in 0..fetch_limit {
            match self.fetch_and_lock_job(worker_id, queues).await? {
                Some(job) => jobs.push(job),
                None => break, // No more available jobs
            }
        }

        Ok(jobs)
    }
}

impl MemoryStorage {
    /// Clean up expired locks
    fn cleanup_expired_locks(&self) {
        let mut locks = self.locks.lock().unwrap();
        let now = chrono::Utc::now();
        locks.retain(|_, lock| lock.expires_at > now);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::Job;
    use chrono::Duration;

    fn create_test_job() -> Job {
        Job::new("test_job", vec!["test_arg".to_string()])
    }

    #[tokio::test]
    async fn test_memory_storage_basic_operations() {
        let storage = MemoryStorage::new();
        let job = create_test_job();

        // Test enqueue
        assert!(storage.enqueue(&job).await.is_ok());
        assert_eq!(storage.len(), 1);

        // Test get
        let retrieved = storage.get(&job.id).await.unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().id, job.id);

        // Test update
        let mut updated_job = job.clone();
        updated_job.state = JobState::processing("worker1", "server1");
        assert!(storage.update(&updated_job).await.is_ok());

        let retrieved = storage.get(&job.id).await.unwrap().unwrap();
        assert!(matches!(retrieved.state, JobState::Processing { .. }));

        // Test delete
        let deleted = storage.delete(&job.id).await.unwrap();
        assert!(deleted);
        assert_eq!(storage.len(), 0);

        // Test delete non-existent
        let deleted = storage.delete(&job.id).await.unwrap();
        assert!(!deleted);
    }

    #[tokio::test]
    async fn test_memory_storage_list_operations() {
        let storage = MemoryStorage::new();

        // Create jobs with different states
        let mut job1 = create_test_job();
        job1.state = JobState::enqueued("default");

        let mut job2 = create_test_job();
        job2.state = JobState::processing("worker1", "server1");

        let mut job3 = create_test_job();
        job3.state = JobState::succeeded(100, None);

        storage.enqueue(&job1).await.unwrap();
        storage.enqueue(&job2).await.unwrap();
        storage.enqueue(&job3).await.unwrap();

        // Test list all
        let all_jobs = storage.list(None, None, None).await.unwrap();
        assert_eq!(all_jobs.len(), 3);

        // Test list by state
        let enqueued_state = JobState::enqueued("default");
        let enqueued_jobs = storage
            .list(Some(&enqueued_state), None, None)
            .await
            .unwrap();
        assert_eq!(enqueued_jobs.len(), 1);
        assert!(matches!(enqueued_jobs[0].state, JobState::Enqueued { .. }));

        // Test list with limit
        let limited_jobs = storage.list(None, Some(2), None).await.unwrap();
        assert_eq!(limited_jobs.len(), 2);

        // Test list with offset
        let offset_jobs = storage.list(None, None, Some(1)).await.unwrap();
        assert_eq!(offset_jobs.len(), 2);

        // Test list with limit and offset
        let paginated_jobs = storage.list(None, Some(1), Some(1)).await.unwrap();
        assert_eq!(paginated_jobs.len(), 1);
    }

    #[tokio::test]
    async fn test_memory_storage_job_counts() {
        let storage = MemoryStorage::new();

        let mut job1 = create_test_job();
        job1.state = JobState::enqueued("default");

        let mut job2 = create_test_job();
        job2.state = JobState::enqueued("default");

        let mut job3 = create_test_job();
        job3.state = JobState::processing("worker1", "server1");

        storage.enqueue(&job1).await.unwrap();
        storage.enqueue(&job2).await.unwrap();
        storage.enqueue(&job3).await.unwrap();

        let counts = storage.get_job_counts().await.unwrap();

        // Check that we have the right number of different state types
        assert!(counts.len() >= 2);

        // Since we're grouping by state type, we should have some enqueued and processing
        let has_enqueued = counts
            .keys()
            .any(|k| matches!(k, JobState::Enqueued { .. }));
        let has_processing = counts
            .keys()
            .any(|k| matches!(k, JobState::Processing { .. }));
        assert!(has_enqueued);
        assert!(has_processing);
    }

    #[tokio::test]
    async fn test_memory_storage_available_jobs() {
        let storage = MemoryStorage::new();

        // Create jobs with different states and schedules
        let mut job1 = create_test_job();
        job1.state = JobState::enqueued("default");

        let mut job2 = create_test_job();
        job2.state = JobState::scheduled(Utc::now() - Duration::hours(1), "delay");

        let mut job3 = create_test_job();
        job3.state = JobState::scheduled(Utc::now() + Duration::hours(1), "delay");

        let mut job4 = create_test_job();
        job4.state = JobState::processing("worker1", "server1");

        storage.enqueue(&job1).await.unwrap();
        storage.enqueue(&job2).await.unwrap();
        storage.enqueue(&job3).await.unwrap();
        storage.enqueue(&job4).await.unwrap();

        let available = storage.get_available_jobs(None).await.unwrap();
        assert_eq!(available.len(), 2); // job1 (enqueued) and job2 (scheduled for past)

        // Test with limit
        let limited_available = storage.get_available_jobs(Some(1)).await.unwrap();
        assert_eq!(limited_available.len(), 1);
    }

    #[tokio::test]
    async fn test_memory_storage_capacity_limit() {
        let config = MemoryConfig::new().with_max_jobs(2);
        let storage = MemoryStorage::with_config(config);

        let job1 = create_test_job();
        let job2 = create_test_job();
        let job3 = create_test_job();

        assert!(storage.enqueue(&job1).await.is_ok());
        assert!(storage.enqueue(&job2).await.is_ok());

        // Third job should fail due to capacity
        let result = storage.enqueue(&job3).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            StorageError::CapacityExceeded { .. }
        ));
    }

    #[tokio::test]
    async fn test_memory_storage_auto_cleanup() {
        let config = MemoryConfig::new().with_max_jobs(3).with_auto_cleanup(true);
        let storage = MemoryStorage::with_config(config);

        let mut job1 = create_test_job();
        job1.state = JobState::succeeded(100, None); // Will be cleaned up

        let mut job2 = create_test_job();
        job2.state = JobState::enqueued("default"); // Will remain

        let job3 = create_test_job(); // New job

        storage.enqueue(&job1).await.unwrap();
        storage.enqueue(&job2).await.unwrap();

        // This should trigger cleanup and succeed
        assert!(storage.enqueue(&job3).await.is_ok());
        assert_eq!(storage.len(), 2); // job1 should be cleaned up
    }

    #[tokio::test]
    async fn test_memory_storage_update_nonexistent() {
        let storage = MemoryStorage::new();
        let job = create_test_job();

        let result = storage.update(&job).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            StorageError::JobNotFound { .. }
        ));
    }
}
