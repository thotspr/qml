//! Job Processing Engine
//!
//! This module contains the job processing engine that handles job execution,
//! worker management, and background job processing.

use async_trait::async_trait;
use std::collections::HashMap;

use crate::core::Job;
use crate::error::Result;

pub mod activator;
pub mod processor;
pub mod retry;
pub mod scheduler;
pub mod server;
pub mod worker;

pub use activator::JobActivator;
pub use processor::JobProcessor;
pub use retry::{RetryPolicy, RetryStrategy};
pub use scheduler::JobScheduler;
pub use server::{BackgroundJobServer, ServerConfig};
pub use worker::{WorkerConfig, WorkerContext, WorkerResult};

/// Trait for executing jobs
///
/// Implementations of this trait define how specific job types are executed.
/// The worker receives the job and its arguments, and returns a result.
#[async_trait]
pub trait Worker: Send + Sync {
    /// Execute a job with the given arguments
    ///
    /// # Arguments
    /// * `job` - The job to execute
    /// * `context` - Worker context with execution information
    ///
    /// # Returns
    /// * `Ok(WorkerResult)` if the job was executed successfully
    /// * `Err(QmlError)` if there was an error executing the job
    async fn execute(&self, job: &Job, context: &WorkerContext) -> Result<WorkerResult>;

    /// Get the method name this worker handles
    fn method_name(&self) -> &str;

    /// Check if this worker can handle the given job method
    fn can_handle(&self, method: &str) -> bool {
        self.method_name() == method
    }
}

/// Registry for job workers
///
/// This registry maps job method names to their corresponding worker implementations.
/// It's used by the job processor to find the appropriate worker for each job.
#[derive(Default)]
pub struct WorkerRegistry {
    workers: HashMap<String, Box<dyn Worker>>,
}

impl WorkerRegistry {
    /// Create a new worker registry
    pub fn new() -> Self {
        Self {
            workers: HashMap::new(),
        }
    }

    /// Register a worker for a specific method
    pub fn register<W>(&mut self, worker: W)
    where
        W: Worker + 'static,
    {
        let method_name = worker.method_name().to_string();
        self.workers.insert(method_name, Box::new(worker));
    }

    /// Get a worker for the given method name
    pub fn get_worker(&self, method: &str) -> Option<&dyn Worker> {
        self.workers.get(method).map(|w| w.as_ref())
    }

    /// Get all registered method names
    pub fn get_methods(&self) -> Vec<&str> {
        self.workers.keys().map(|s| s.as_str()).collect()
    }

    /// Check if a method is registered
    pub fn has_worker(&self, method: &str) -> bool {
        self.workers.contains_key(method)
    }

    /// Get the number of registered workers
    pub fn len(&self) -> usize {
        self.workers.len()
    }

    /// Check if the registry is empty
    pub fn is_empty(&self) -> bool {
        self.workers.is_empty()
    }
}
