//! Background job server for managing job processing
//!
//! This module contains the BackgroundJobServer that coordinates job processing,
//! manages worker threads, and handles the overall job processing lifecycle.

use chrono::Duration;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::task::JoinHandle;
use tokio::time::{interval, sleep};
use tracing::{debug, error, info};

use super::{
    processor::JobProcessor, scheduler::JobScheduler, worker::WorkerConfig, RetryPolicy,
    WorkerRegistry,
};
use crate::error::{QmlError, Result};
use crate::storage::Storage;

/// Configuration for the background job server
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    /// Server name identifier
    pub server_name: String,
    /// Number of worker threads to run
    pub worker_count: usize,
    /// Polling interval for checking new jobs
    pub polling_interval: Duration,
    /// Timeout for job execution
    pub job_timeout: Duration,
    /// Queues to process (empty means all queues)
    pub queues: Vec<String>,
    /// Whether the server should start automatically
    pub auto_start: bool,
    /// Maximum number of jobs to fetch per polling cycle
    pub fetch_batch_size: usize,
    /// Enable the job scheduler
    pub enable_scheduler: bool,
    /// Scheduler polling interval
    pub scheduler_poll_interval: Duration,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            server_name: "qml-server".to_string(),
            worker_count: 5,
            polling_interval: Duration::seconds(1),
            job_timeout: Duration::minutes(5),
            queues: vec!["default".to_string()],
            auto_start: true,
            fetch_batch_size: 10,
            enable_scheduler: true,
            scheduler_poll_interval: Duration::seconds(30),
        }
    }
}

impl ServerConfig {
    /// Create a new server configuration
    pub fn new(server_name: impl Into<String>) -> Self {
        Self {
            server_name: server_name.into(),
            ..Default::default()
        }
    }

    /// Set the number of workers
    pub fn worker_count(mut self, count: usize) -> Self {
        self.worker_count = count;
        self
    }

    /// Set the polling interval
    pub fn polling_interval(mut self, interval: Duration) -> Self {
        self.polling_interval = interval;
        self
    }

    /// Set the job timeout
    pub fn job_timeout(mut self, timeout: Duration) -> Self {
        self.job_timeout = timeout;
        self
    }

    /// Set the queues to process
    pub fn queues(mut self, queues: Vec<String>) -> Self {
        self.queues = queues;
        self
    }

    /// Set the fetch batch size
    pub fn fetch_batch_size(mut self, size: usize) -> Self {
        self.fetch_batch_size = size;
        self
    }

    /// Enable or disable the scheduler
    pub fn enable_scheduler(mut self, enable: bool) -> Self {
        self.enable_scheduler = enable;
        self
    }
}

/// Background job server that manages job processing
pub struct BackgroundJobServer {
    config: ServerConfig,
    storage: Arc<dyn Storage>,
    worker_registry: Arc<WorkerRegistry>,
    retry_policy: RetryPolicy,
    #[allow(dead_code)]
    scheduler: Option<JobScheduler>,
    is_running: Arc<tokio::sync::RwLock<bool>>,
    worker_handles: Arc<tokio::sync::RwLock<Vec<JoinHandle<()>>>>,
}

impl BackgroundJobServer {
    /// Create a new background job server
    pub fn new(
        config: ServerConfig,
        storage: Arc<dyn Storage>,
        worker_registry: Arc<WorkerRegistry>,
    ) -> Self {
        let scheduler = if config.enable_scheduler {
            Some(JobScheduler::with_poll_interval(
                storage.clone(),
                config.scheduler_poll_interval,
            ))
        } else {
            None
        };

        Self {
            config,
            storage,
            worker_registry,
            retry_policy: RetryPolicy::default(),
            scheduler,
            is_running: Arc::new(tokio::sync::RwLock::new(false)),
            worker_handles: Arc::new(tokio::sync::RwLock::new(Vec::new())),
        }
    }

    /// Create a new background job server with custom retry policy
    pub fn with_retry_policy(
        config: ServerConfig,
        storage: Arc<dyn Storage>,
        worker_registry: Arc<WorkerRegistry>,
        retry_policy: RetryPolicy,
    ) -> Self {
        let mut server = Self::new(config, storage, worker_registry);
        server.retry_policy = retry_policy;
        server
    }

    /// Start the background job server
    pub async fn start(&self) -> Result<()> {
        let mut is_running = self.is_running.write().await;
        if *is_running {
            return Err(QmlError::ConfigurationError {
                message: "Server is already running".to_string(),
            });
        }

        info!(
            "Starting background job server '{}' with {} workers",
            self.config.server_name, self.config.worker_count
        );

        *is_running = true;
        drop(is_running);

        // Start scheduler if enabled
        if self.config.enable_scheduler {
            let scheduler = JobScheduler::with_poll_interval(
                self.storage.clone(),
                self.config.scheduler_poll_interval,
            );
            let is_running_clone = self.is_running.clone();

            let scheduler_handle = tokio::spawn(async move {
                while *is_running_clone.read().await {
                    if let Err(e) = scheduler.run().await {
                        error!("Scheduler error: {}", e);
                        sleep(std::time::Duration::from_secs(5)).await;
                    }
                }
            });

            self.worker_handles.write().await.push(scheduler_handle);
        }

        // Start worker threads
        self.start_workers().await?;

        info!("Background job server started successfully");
        Ok(())
    }

    /// Stop the background job server
    pub async fn stop(&self) -> Result<()> {
        let mut is_running = self.is_running.write().await;
        if !*is_running {
            return Ok(());
        }

        info!(
            "Stopping background job server '{}'",
            self.config.server_name
        );

        *is_running = false;
        drop(is_running);

        // Wait for all workers to complete
        let mut handles = self.worker_handles.write().await;
        for handle in handles.drain(..) {
            handle.abort();
        }

        info!("Background job server stopped");
        Ok(())
    }

    /// Check if the server is running
    pub async fn is_running(&self) -> bool {
        *self.is_running.read().await
    }

    /// Get server configuration
    pub fn config(&self) -> &ServerConfig {
        &self.config
    }

    /// Start worker threads
    async fn start_workers(&self) -> Result<()> {
        let mut handles = self.worker_handles.write().await;

        for worker_id in 0..self.config.worker_count {
            let worker_config =
                WorkerConfig::new(format!("{}:worker:{}", self.config.server_name, worker_id))
                    .server_name(&self.config.server_name)
                    .queues(self.config.queues.clone())
                    .job_timeout(self.config.job_timeout)
                    .polling_interval(self.config.polling_interval);

            let processor = JobProcessor::with_retry_policy(
                self.worker_registry.clone(),
                self.storage.clone(),
                worker_config,
                self.retry_policy.clone(),
            );

            let storage_clone = self.storage.clone();
            let config_clone = self.config.clone();
            let is_running_clone = self.is_running.clone();

            let handle = tokio::spawn(async move {
                Self::worker_loop(processor, storage_clone, config_clone, is_running_clone).await;
            });

            handles.push(handle);
        }

        info!("Started {} worker threads", self.config.worker_count);
        Ok(())
    }

    /// Main worker loop for processing jobs
    async fn worker_loop(
        processor: JobProcessor,
        storage: Arc<dyn Storage>,
        config: ServerConfig,
        is_running: Arc<tokio::sync::RwLock<bool>>,
    ) {
        debug!("Worker thread started");

        let mut interval = interval(
            config
                .polling_interval
                .to_std()
                .unwrap_or(std::time::Duration::from_secs(1)),
        );

        while *is_running.read().await {
            interval.tick().await;

            // Fetch available jobs
            match storage
                .get_available_jobs(Some(config.fetch_batch_size))
                .await
            {
                Ok(jobs) => {
                    if !jobs.is_empty() {
                        debug!("Fetched {} jobs for processing", jobs.len());
                    }

                    for job in jobs {
                        // Check if we should process this job based on queue filter
                        if !config.queues.is_empty() && !config.queues.contains(&job.queue) {
                            continue;
                        }

                        // Process the job
                        if let Err(e) = processor.process_job(job).await {
                            error!("Error processing job: {}", e);
                        }

                        // Check if we should stop
                        if !*is_running.read().await {
                            break;
                        }
                    }
                }
                Err(e) => {
                    error!("Error fetching jobs: {}", e);
                    // Back off on error
                    sleep(std::time::Duration::from_secs(5)).await;
                }
            }
        }

        debug!("Worker thread stopped");
    }
}

impl Clone for BackgroundJobServer {
    fn clone(&self) -> Self {
        let scheduler = if self.config.enable_scheduler {
            Some(JobScheduler::with_poll_interval(
                self.storage.clone(),
                self.config.scheduler_poll_interval,
            ))
        } else {
            None
        };

        Self {
            config: self.config.clone(),
            storage: self.storage.clone(),
            worker_registry: self.worker_registry.clone(),
            retry_policy: self.retry_policy.clone(),
            scheduler,
            is_running: Arc::new(tokio::sync::RwLock::new(false)),
            worker_handles: Arc::new(tokio::sync::RwLock::new(Vec::new())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::processing::{Worker, WorkerContext, WorkerResult};
    use crate::storage::MemoryStorage;
    use async_trait::async_trait;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct TestWorker {
        method: String,
        call_count: Arc<AtomicUsize>,
    }

    impl TestWorker {
        fn new(method: &str) -> Self {
            Self {
                method: method.to_string(),
                call_count: Arc::new(AtomicUsize::new(0)),
            }
        }

        #[allow(dead_code)]
        fn call_count(&self) -> usize {
            self.call_count.load(Ordering::Relaxed)
        }
    }

    #[async_trait]
    impl Worker for TestWorker {
        async fn execute(
            &self,
            _job: &crate::core::Job,
            _context: &WorkerContext,
        ) -> Result<WorkerResult> {
            self.call_count.fetch_add(1, Ordering::Relaxed);
            tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
            Ok(WorkerResult::success(
                Some("Test completed".to_string()),
                10,
            ))
        }

        fn method_name(&self) -> &str {
            &self.method
        }
    }

    #[tokio::test]
    async fn test_server_start_stop() {
        let storage = Arc::new(MemoryStorage::new());
        let mut registry = WorkerRegistry::new();
        registry.register(TestWorker::new("test_method"));
        let registry = Arc::new(registry);

        let config = ServerConfig::new("test-server")
            .worker_count(2)
            .polling_interval(Duration::milliseconds(100))
            .enable_scheduler(false);

        let server = BackgroundJobServer::new(config, storage, registry);

        // Start server
        server.start().await.unwrap();
        assert!(server.is_running().await);

        // Stop server
        server.stop().await.unwrap();
        assert!(!server.is_running().await);
    }

    #[tokio::test]
    async fn test_job_processing() {
        let storage = Arc::new(MemoryStorage::new());
        let worker = TestWorker::new("test_method");
        let call_count = worker.call_count.clone();

        let mut registry = WorkerRegistry::new();
        registry.register(worker);
        let registry = Arc::new(registry);

        let config = ServerConfig::new("test-server")
            .worker_count(1)
            .polling_interval(Duration::milliseconds(10))
            .fetch_batch_size(1)
            .enable_scheduler(false);

        let server = BackgroundJobServer::new(config, storage.clone(), registry);

        // Enqueue a test job
        let job = crate::core::Job::new("test_method", vec!["arg1".to_string()]);
        storage.enqueue(&job).await.unwrap();

        // Start server
        server.start().await.unwrap();

        // Wait for job to be processed
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        // Stop server
        server.stop().await.unwrap();

        // Check that the job was processed
        assert!(call_count.load(Ordering::Relaxed) > 0);
    }
}
