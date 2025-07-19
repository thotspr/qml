#[cfg(feature = "postgres")]
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde_json;
use sqlx::{PgPool, Row};
use std::collections::HashMap;
use std::str::FromStr;
use uuid::Uuid;

use super::{PostgresConfig, Storage, StorageError};
use crate::core::{Job, JobState};

/// PostgreSQL storage implementation for jobs
///
/// This storage implementation uses PostgreSQL with sqlx for persistence.
/// It provides robust, ACID-compliant storage with proper indexing for
/// high-performance job processing.
#[derive(Debug, Clone)]
pub struct PostgresStorage {
    pool: PgPool,
    config: PostgresConfig,
}

impl PostgresStorage {
    /// Create a new PostgreSQL storage with the given configuration
    pub async fn new(config: PostgresConfig) -> Result<Self, StorageError> {
        // Create connection pool
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(config.max_connections)
            .min_connections(config.min_connections)
            .acquire_timeout(config.connect_timeout)
            .idle_timeout(config.idle_timeout)
            .max_lifetime(config.max_lifetime)
            .connect(&config.database_url)
            .await
            .map_err(|e| StorageError::ConnectionError {
                message: format!("Failed to connect to PostgreSQL: {}", e),
            })?;

        let storage = Self { pool, config };

        // Run migrations if auto_migrate is enabled
        if storage.config.auto_migrate {
            storage.migrate().await?;
        }

        Ok(storage)
    }

    /// Run database migrations
    /// 
    /// This method runs SQL migrations from the configured migrations directory.
    /// The migration path can be customized using `PostgresConfig::with_migrations_path()`.
    /// 
    /// # Example
    /// 
    /// ```rust,no_run
    /// use qml::storage::{PostgresConfig, PostgresStorage};
    /// 
    /// #[tokio::main]
    /// async fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     // Use custom migration path
    ///     let config = PostgresConfig::new()
    ///         .with_database_url("postgresql://user:pass@localhost/db")
    ///         .with_migrations_path("./custom_migrations")
    ///         .with_auto_migrate(true);
    ///     
    ///     let storage = PostgresStorage::new(config).await?;
    ///     
    ///     // Manually run migrations if auto_migrate is disabled
    ///     // storage.migrate().await?;
    ///     
    ///     Ok(())
    /// }
    /// ```
    pub async fn migrate(&self) -> Result<(), StorageError> {
        // Use the configured migration path or default to "./migrations"
        let migration_path = if self.config.migrations_path.is_empty() {
            "./migrations"
        } else {
            &self.config.migrations_path
        };

        // Create migrator from the specified path
        let migrator = sqlx::migrate::Migrator::new(std::path::Path::new(migration_path))
            .await
            .map_err(|e| StorageError::MigrationError {
                message: format!("Failed to create migrator from path '{}': {}", migration_path, e),
            })?;

        // Run migrations
        migrator
            .run(&self.pool)
            .await
            .map_err(|e| StorageError::MigrationError {
                message: format!("Migration failed: {}", e),
            })?;

        Ok(())
    }

    /// Get a reference to the connection pool
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// Get the configuration
    pub fn config(&self) -> &PostgresConfig {
        &self.config
    }

    /// Close the connection pool
    pub async fn close(&self) {
        self.pool.close().await;
    }

    /// Convert Job to database row values
    fn job_to_row_values(
        job: &Job,
    ) -> Result<(String, serde_json::Value, serde_json::Value), StorageError> {
        let state_name = Self::job_state_to_name(&job.state);
        let state_data = Self::job_state_to_data(&job.state)?;
        let arguments =
            serde_json::to_value(&job.arguments).map_err(|e| StorageError::SerializationError {
                message: format!("Failed to serialize job arguments: {}", e),
            })?;

        Ok((state_name, state_data, arguments))
    }

    /// Convert database row to Job
    fn row_to_job(row: &sqlx::postgres::PgRow) -> Result<Job, StorageError> {
        let id: Uuid = row
            .try_get("id")
            .map_err(|e| StorageError::DeserializationError {
                message: format!("Failed to get job ID: {}", e),
            })?;

        let method_name: String =
            row.try_get("method_name")
                .map_err(|e| StorageError::DeserializationError {
                    message: format!("Failed to get method name: {}", e),
                })?;

        let arguments_json: serde_json::Value =
            row.try_get("arguments")
                .map_err(|e| StorageError::DeserializationError {
                    message: format!("Failed to get arguments: {}", e),
                })?;

        let arguments: Vec<String> = serde_json::from_value(arguments_json).map_err(|e| {
            StorageError::DeserializationError {
                message: format!("Failed to deserialize arguments: {}", e),
            }
        })?;

        let created_at: DateTime<Utc> =
            row.try_get("created_at")
                .map_err(|e| StorageError::DeserializationError {
                    message: format!("Failed to get created_at: {}", e),
                })?;

        let state_name: String =
            row.try_get("state_name")
                .map_err(|e| StorageError::DeserializationError {
                    message: format!("Failed to get state name: {}", e),
                })?;

        let state_data: serde_json::Value =
            row.try_get("state_data")
                .map_err(|e| StorageError::DeserializationError {
                    message: format!("Failed to get state data: {}", e),
                })?;

        let queue_name: String =
            row.try_get("queue_name")
                .map_err(|e| StorageError::DeserializationError {
                    message: format!("Failed to get queue name: {}", e),
                })?;

        let priority: i32 =
            row.try_get("priority")
                .map_err(|e| StorageError::DeserializationError {
                    message: format!("Failed to get priority: {}", e),
                })?;

        let max_retries: i32 =
            row.try_get("max_retries")
                .map_err(|e| StorageError::DeserializationError {
                    message: format!("Failed to get max_retries: {}", e),
                })?;

        let metadata_json: Option<serde_json::Value> =
            row.try_get("metadata")
                .map_err(|e| StorageError::DeserializationError {
                    message: format!("Failed to get metadata: {}", e),
                })?;

        let metadata: HashMap<String, String> = if let Some(meta) = metadata_json {
            serde_json::from_value(meta).map_err(|e| StorageError::DeserializationError {
                message: format!("Failed to deserialize metadata: {}", e),
            })?
        } else {
            HashMap::new()
        };

        let job_type: Option<String> =
            row.try_get("job_type")
                .map_err(|e| StorageError::DeserializationError {
                    message: format!("Failed to get job_type: {}", e),
                })?;

        let timeout_seconds: Option<i32> =
            row.try_get("timeout_seconds")
                .map_err(|e| StorageError::DeserializationError {
                    message: format!("Failed to get timeout_seconds: {}", e),
                })?;

        let state = Self::data_to_job_state(&state_name, &state_data)?;

        Ok(Job {
            id: id.to_string(),
            method: method_name,
            arguments,
            created_at,
            state,
            queue: queue_name,
            priority,
            max_retries: max_retries as u32,
            metadata,
            job_type,
            timeout_seconds: timeout_seconds.map(|t| t as u64),
        })
    }

    /// Convert JobState to state name string
    fn job_state_to_name(state: &JobState) -> String {
        match state {
            JobState::Enqueued { .. } => "Enqueued".to_string(),
            JobState::Processing { .. } => "Processing".to_string(),
            JobState::Succeeded { .. } => "Succeeded".to_string(),
            JobState::Failed { .. } => "Failed".to_string(),
            JobState::Deleted { .. } => "Deleted".to_string(),
            JobState::Scheduled { .. } => "Scheduled".to_string(),
            JobState::AwaitingRetry { .. } => "AwaitingRetry".to_string(),
        }
    }

    /// Convert JobState to JSON data
    fn job_state_to_data(state: &JobState) -> Result<serde_json::Value, StorageError> {
        serde_json::to_value(state).map_err(|e| StorageError::SerializationError {
            message: format!("Failed to serialize job state: {}", e),
        })
    }

    /// Convert state name and JSON data back to JobState
    fn data_to_job_state(
        state_name: &str,
        state_data: &serde_json::Value,
    ) -> Result<JobState, StorageError> {
        serde_json::from_value(state_data.clone()).map_err(|e| StorageError::DeserializationError {
            message: format!("Failed to deserialize job state {}: {}", state_name, e),
        })
    }

    /// Check if a job is available for processing
    fn is_job_available(state: &JobState) -> bool {
        let now = Utc::now();
        match state {
            JobState::Enqueued { .. } => true,
            JobState::Scheduled { enqueue_at, .. } => *enqueue_at <= now,
            JobState::AwaitingRetry { retry_at, .. } => *retry_at <= now,
            _ => false,
        }
    }

    /// Build the full table name with schema
    fn table_name(&self) -> String {
        self.config.full_table_name()
    }
}

#[async_trait]
impl Storage for PostgresStorage {
    async fn enqueue(&self, job: &Job) -> Result<(), StorageError> {
        let (state_name, state_data, arguments) = Self::job_to_row_values(job)?;
        let metadata = if job.metadata.is_empty() {
            None
        } else {
            Some(serde_json::to_value(&job.metadata).map_err(|e| {
                StorageError::SerializationError {
                    message: format!("Failed to serialize metadata: {}", e),
                }
            })?)
        };

        let job_id = Uuid::from_str(&job.id).map_err(|e| StorageError::InvalidJobData {
            message: format!("Invalid job ID format: {}", e),
        })?;

        let query = format!(
            r#"
            INSERT INTO {} (
                id, method_name, arguments, created_at, state_name, state_data,
                queue_name, priority, max_retries, current_retries, metadata,
                job_type, timeout_seconds
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
            "#,
            self.table_name()
        );

        sqlx::query(&query)
            .bind(job_id)
            .bind(&job.method)
            .bind(arguments)
            .bind(job.created_at)
            .bind(state_name)
            .bind(state_data)
            .bind(&job.queue)
            .bind(job.priority)
            .bind(job.max_retries as i32)
            .bind(0i32) // current_retries starts at 0
            .bind(metadata)
            .bind(&job.job_type)
            .bind(job.timeout_seconds.map(|t| t as i32))
            .execute(&self.pool)
            .await
            .map_err(|e| StorageError::OperationError {
                message: format!("Failed to enqueue job: {}", e),
            })?;

        Ok(())
    }

    async fn get(&self, job_id: &str) -> Result<Option<Job>, StorageError> {
        let job_uuid = Uuid::from_str(job_id).map_err(|e| StorageError::InvalidJobData {
            message: format!("Invalid job ID format: {}", e),
        })?;

        let query = format!(
            r#"
            SELECT id, method_name, arguments, created_at, state_name, state_data,
                   queue_name, priority, max_retries, metadata, job_type, timeout_seconds
            FROM {}
            WHERE id = $1
            "#,
            self.table_name()
        );

        let row = sqlx::query(&query)
            .bind(job_uuid)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| StorageError::OperationError {
                message: format!("Failed to get job: {}", e),
            })?;

        match row {
            Some(row) => Ok(Some(Self::row_to_job(&row)?)),
            None => Ok(None),
        }
    }

    async fn update(&self, job: &Job) -> Result<(), StorageError> {
        let (state_name, state_data, arguments) = Self::job_to_row_values(job)?;
        let metadata = if job.metadata.is_empty() {
            None
        } else {
            Some(serde_json::to_value(&job.metadata).map_err(|e| {
                StorageError::SerializationError {
                    message: format!("Failed to serialize metadata: {}", e),
                }
            })?)
        };

        let job_id = Uuid::from_str(&job.id).map_err(|e| StorageError::InvalidJobData {
            message: format!("Invalid job ID format: {}", e),
        })?;

        let query = format!(
            r#"
            UPDATE {}
            SET method_name = $2, arguments = $3, state_name = $4, state_data = $5,
                queue_name = $6, priority = $7, max_retries = $8, metadata = $9,
                job_type = $10, timeout_seconds = $11, updated_at = NOW()
            WHERE id = $1
            "#,
            self.table_name()
        );

        let result = sqlx::query(&query)
            .bind(job_id)
            .bind(&job.method)
            .bind(arguments)
            .bind(state_name)
            .bind(state_data)
            .bind(&job.queue)
            .bind(job.priority)
            .bind(job.max_retries as i32)
            .bind(metadata)
            .bind(&job.job_type)
            .bind(job.timeout_seconds.map(|t| t as i32))
            .execute(&self.pool)
            .await
            .map_err(|e| StorageError::OperationError {
                message: format!("Failed to update job: {}", e),
            })?;

        if result.rows_affected() == 0 {
            return Err(StorageError::JobNotFound {
                job_id: job.id.clone(),
            });
        }

        Ok(())
    }

    async fn delete(&self, job_id: &str) -> Result<bool, StorageError> {
        let job_uuid = Uuid::from_str(job_id).map_err(|e| StorageError::InvalidJobData {
            message: format!("Invalid job ID format: {}", e),
        })?;

        let query = format!("DELETE FROM {} WHERE id = $1", self.table_name());

        let result = sqlx::query(&query)
            .bind(job_uuid)
            .execute(&self.pool)
            .await
            .map_err(|e| StorageError::OperationError {
                message: format!("Failed to delete job: {}", e),
            })?;

        Ok(result.rows_affected() > 0)
    }

    async fn list(
        &self,
        state_filter: Option<&JobState>,
        limit: Option<usize>,
        offset: Option<usize>,
    ) -> Result<Vec<Job>, StorageError> {
        let mut query = format!(
            r#"
            SELECT id, method_name, arguments, created_at, state_name, state_data,
                   queue_name, priority, max_retries, metadata, job_type, timeout_seconds
            FROM {}
            "#,
            self.table_name()
        );

        let mut param_count = 0;

        if let Some(_state) = state_filter {
            param_count += 1;
            query.push_str(&format!(" WHERE state_name = ${}", param_count));
        }

        query.push_str(" ORDER BY created_at DESC");

        if let Some(_limit) = limit {
            param_count += 1;
            query.push_str(&format!(" LIMIT ${}", param_count));
        }

        if let Some(_offset) = offset {
            param_count += 1;
            query.push_str(&format!(" OFFSET ${}", param_count));
        }

        let mut sqlx_query = sqlx::query(&query);

        if let Some(state) = state_filter {
            sqlx_query = sqlx_query.bind(Self::job_state_to_name(state));
        }

        if let Some(limit_val) = limit {
            sqlx_query = sqlx_query.bind(limit_val as i64);
        }

        if let Some(offset_val) = offset {
            sqlx_query = sqlx_query.bind(offset_val as i64);
        }

        let rows =
            sqlx_query
                .fetch_all(&self.pool)
                .await
                .map_err(|e| StorageError::OperationError {
                    message: format!("Failed to list jobs: {}", e),
                })?;

        let mut jobs = Vec::new();
        for row in rows {
            jobs.push(Self::row_to_job(&row)?);
        }

        Ok(jobs)
    }

    async fn get_job_counts(&self) -> Result<HashMap<JobState, usize>, StorageError> {
        let query = format!(
            "SELECT state_name, COUNT(*) as count FROM {} GROUP BY state_name",
            self.table_name()
        );

        let rows = sqlx::query(&query)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| StorageError::OperationError {
                message: format!("Failed to get job counts: {}", e),
            })?;

        let mut counts = HashMap::new();

        for row in rows {
            let state_name: String =
                row.try_get("state_name")
                    .map_err(|e| StorageError::DeserializationError {
                        message: format!("Failed to get state name from count query: {}", e),
                    })?;

            let count: i64 =
                row.try_get("count")
                    .map_err(|e| StorageError::DeserializationError {
                        message: format!("Failed to get count from count query: {}", e),
                    })?;

            // Create a dummy state for the count lookup
            let dummy_state = match state_name.as_str() {
                "Enqueued" => JobState::enqueued("default"),
                "Processing" => JobState::processing("dummy", "dummy"),
                "Succeeded" => JobState::succeeded(0, None),
                "Failed" => JobState::failed("dummy", None, 0),
                "Deleted" => JobState::deleted(None),
                "Scheduled" => JobState::scheduled(Utc::now(), "dummy"),
                "AwaitingRetry" => JobState::awaiting_retry(Utc::now(), 0, "dummy"),
                _ => continue, // Skip unknown states
            };

            counts.insert(dummy_state, count as usize);
        }

        Ok(counts)
    }

    async fn get_available_jobs(&self, limit: Option<usize>) -> Result<Vec<Job>, StorageError> {
        let mut query = format!(
            r#"
            SELECT id, method_name, arguments, created_at, state_name, state_data,
                   queue_name, priority, max_retries, metadata, job_type, timeout_seconds
            FROM {}
            WHERE state_name IN ('Enqueued', 'Scheduled', 'AwaitingRetry')
            AND (
                state_name = 'Enqueued' OR
                (state_name = 'Scheduled' AND (state_data->>'enqueue_at')::timestamp <= NOW()) OR
                (state_name = 'AwaitingRetry' AND (state_data->>'retry_at')::timestamp <= NOW())
            )
            ORDER BY priority DESC, created_at ASC
            "#,
            self.table_name()
        );

        if let Some(limit) = limit {
            query.push_str(&format!(" LIMIT {}", limit));
        }

        let rows = sqlx::query(&query)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| StorageError::OperationError {
                message: format!("Failed to get available jobs: {}", e),
            })?;

        let mut jobs = Vec::new();
        for row in rows {
            let job = Self::row_to_job(&row)?;
            if Self::is_job_available(&job.state) {
                jobs.push(job);
            }
        }

        Ok(jobs)
    }

    async fn fetch_and_lock_job(
        &self,
        worker_id: &str,
        queues: Option<&[String]>,
    ) -> Result<Option<Job>, StorageError> {
        let mut transaction =
            self.pool
                .begin()
                .await
                .map_err(|e| StorageError::OperationError {
                    message: format!("Failed to start transaction: {}", e),
                })?;

        // Use SELECT FOR UPDATE SKIP LOCKED for atomic job fetching
        let mut query = format!(
            r#"
            SELECT id, method_name, arguments, state_name, state_data, metadata, 
                   created_at, updated_at, priority, queue_name, scheduled_at
            FROM {}
            WHERE state_name IN ('enqueued', 'retrying')
        "#,
            self.table_name()
        );

        // Add queue filtering if specified
        if let Some(queues) = queues {
            if !queues.is_empty() {
                let queue_placeholders: Vec<String> =
                    (1..=queues.len()).map(|i| format!("${}", i)).collect();
                query.push_str(&format!(
                    " AND queue_name = ANY(ARRAY[{}])",
                    queue_placeholders.join(",")
                ));
            }
        }

        // Order by priority and creation time, then lock and skip locked rows
        query.push_str(" ORDER BY priority DESC, created_at ASC FOR UPDATE SKIP LOCKED LIMIT 1");

        let mut sqlx_query = sqlx::query(&query);

        // Bind queue names if provided
        if let Some(queues) = queues {
            for queue in queues {
                sqlx_query = sqlx_query.bind(queue);
            }
        }

        let row = sqlx_query
            .fetch_optional(&mut *transaction)
            .await
            .map_err(|e| StorageError::OperationError {
                message: format!("Failed to fetch and lock job: {}", e),
            })?;

        if let Some(row) = row {
            let mut job = Self::row_to_job(&row)?;

            // Mark as processing and add worker metadata
            job.state = JobState::Processing {
                worker_id: worker_id.to_string(),
                started_at: chrono::Utc::now(),
                server_name: "postgres-storage".to_string(),
            };

            // Update the job in the same transaction
            let (state_name, state_data, metadata) = Self::job_to_row_values(&job)?;

            let update_query = format!(
                r#"
                UPDATE {} 
                SET state_name = $2, state_data = $3, metadata = $4, updated_at = NOW()
                WHERE id = $1
            "#,
                self.table_name()
            );

            sqlx::query(&update_query)
                .bind(&job.id)
                .bind(state_name)
                .bind(state_data)
                .bind(metadata)
                .execute(&mut *transaction)
                .await
                .map_err(|e| StorageError::OperationError {
                    message: format!("Failed to update job state: {}", e),
                })?;

            transaction
                .commit()
                .await
                .map_err(|e| StorageError::OperationError {
                    message: format!("Failed to commit transaction: {}", e),
                })?;

            Ok(Some(job))
        } else {
            // No available jobs
            transaction
                .commit()
                .await
                .map_err(|e| StorageError::OperationError {
                    message: format!("Failed to commit transaction: {}", e),
                })?;
            Ok(None)
        }
    }

    async fn try_acquire_job_lock(
        &self,
        job_id: &str,
        worker_id: &str,
        timeout_seconds: u64,
    ) -> Result<bool, StorageError> {
        let expires_at = chrono::Utc::now() + chrono::Duration::seconds(timeout_seconds as i64);

        // Create a lock table entry using INSERT ... ON CONFLICT DO NOTHING for atomic locking
        let insert_query = r#"
            INSERT INTO qml_job_locks (job_id, worker_id, expires_at, created_at)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (job_id) DO NOTHING
            RETURNING job_id
        "#;

        let result = sqlx::query(insert_query)
            .bind(job_id)
            .bind(worker_id)
            .bind(expires_at)
            .bind(chrono::Utc::now())
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| StorageError::OperationError {
                message: format!("Failed to acquire job lock: {}", e),
            })?;

        Ok(result.is_some())
    }

    async fn release_job_lock(&self, job_id: &str, worker_id: &str) -> Result<bool, StorageError> {
        let delete_query = r#"
            DELETE FROM qml_job_locks 
            WHERE job_id = $1 AND worker_id = $2
            RETURNING job_id
        "#;

        let result = sqlx::query(delete_query)
            .bind(job_id)
            .bind(worker_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| StorageError::OperationError {
                message: format!("Failed to release job lock: {}", e),
            })?;

        Ok(result.is_some())
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
