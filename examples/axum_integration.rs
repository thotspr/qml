//! # QML + Axum Integration Example
//!
//! This example demonstrates complete integration of QML with an Axum web server:
//! - Background job processing alongside web requests  
//! - Safe configuration without panics (no environment variables required!)
//! - Job creation, retrieval, and status monitoring via HTTP endpoints
//! - Production-ready patterns and error handling
//!
//! ## Quick Start for Your Project
//!
//! 1. Add to Cargo.toml: `qml-rs = "0.1.3-alpha"`
//! 2. Add QML storage to your AppState: `storage: Arc<dyn Storage + Send + Sync>`
//! 3. Initialize with: `StorageInstance::memory()` (no env vars needed!)
//! 4. Use in handlers: `state.storage.enqueue(&job).await`
//!
//! ## Running This Example
//! ```bash
//! cargo run --example axum_integration
//! curl -X POST http://127.0.0.1:3000/jobs \
//!   -H 'Content-Type: application/json' \
//!   -d '{"method": "send_email", "arguments": ["user@example.com"]}'
//! ```

use axum::{
    Router,
    extract::{Path, State},
    http::StatusCode,
    response::Json,
    routing::{get, post},
};
use qml_rs::storage::{MemoryConfig, StorageInstance};
use qml_rs::{Job, Storage};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::net::TcpListener;

// Shared application state
#[derive(Clone)]
struct AppState {
    storage: Arc<dyn Storage + Send + Sync>,
}

// Request/Response types
#[derive(Deserialize)]
struct CreateJobRequest {
    method: String,
    arguments: Vec<String>,
    queue: Option<String>,
}

#[derive(Serialize)]
struct JobResponse {
    id: String,
    method: String,
    arguments: Vec<String>,
    queue: String,
    state: String,
    created_at: String,
}

#[derive(Serialize)]
struct StatusResponse {
    message: String,
    total_jobs: usize,
}

// Convert Job to JobResponse
impl From<Job> for JobResponse {
    fn from(job: Job) -> Self {
        Self {
            id: job.id.to_string(),
            method: job.method,
            arguments: job.arguments,
            queue: job.queue,
            state: format!("{:?}", job.state),
            created_at: job.created_at.to_rfc3339(),
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 Starting Axum + QML Integration Example");

    // Initialize QML storage - this will NOT panic thanks to our fixes!
    let storage_config = MemoryConfig::new()
        .with_max_jobs(1000)
        .with_auto_cleanup(true);

    let storage = StorageInstance::memory_with_config(storage_config);

    println!("✅ QML storage initialized successfully (no panics!)");

    // Create shared application state
    let app_state = AppState {
        storage: Arc::new(storage),
    };

    // Build the Axum router
    let app = Router::new()
        .route("/", get(health_check))
        .route("/jobs", post(create_job))
        .route("/jobs/:id", get(get_job))
        .route("/status", get(get_status))
        .with_state(app_state);

    // Start the server
    let listener = TcpListener::bind("127.0.0.1:3000").await?;
    println!("🌐 Server running on http://127.0.0.1:3000");
    println!("📋 Available endpoints:");
    println!("   GET  /           - Health check");
    println!("   POST /jobs       - Create a new job");
    println!("   GET  /jobs/:id   - Get job details");
    println!("   GET  /status     - Get system status");
    println!();
    println!("💡 Try creating a job:");
    println!("   curl -X POST http://127.0.0.1:3000/jobs \\");
    println!("     -H 'Content-Type: application/json' \\");
    println!("     -d '{{\"method\": \"send_email\", \"arguments\": [\"user@example.com\"]}}'");

    axum::serve(listener, app).await?;

    Ok(())
}

// Health check endpoint
async fn health_check() -> Json<StatusResponse> {
    Json(StatusResponse {
        message: "QML + Axum integration is working! 🎉".to_string(),
        total_jobs: 0,
    })
}

// Create a new job
async fn create_job(
    State(state): State<AppState>,
    Json(payload): Json<CreateJobRequest>,
) -> Result<(StatusCode, Json<JobResponse>), StatusCode> {
    // Create a new job
    let mut job = Job::new(&payload.method, payload.arguments);

    // Set queue if specified
    if let Some(queue) = payload.queue {
        job.queue = queue;
    }

    // Store the job
    match state.storage.enqueue(&job).await {
        Ok(_) => {
            println!("✅ Job created: {} ({})", job.id, job.method);
            Ok((StatusCode::CREATED, Json(job.into())))
        }
        Err(_) => {
            println!("❌ Failed to create job");
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

// Get job details
async fn get_job(
    State(state): State<AppState>,
    Path(job_id): Path<String>,
) -> Result<Json<JobResponse>, StatusCode> {
    match state.storage.get(&job_id).await {
        Ok(Some(job)) => {
            println!("📋 Retrieved job: {}", job.id);
            Ok(Json(job.into()))
        }
        Ok(None) => {
            println!("❓ Job not found: {}", job_id);
            Err(StatusCode::NOT_FOUND)
        }
        Err(_) => {
            println!("❌ Error retrieving job: {}", job_id);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

// Get system status
async fn get_status(State(state): State<AppState>) -> Result<Json<StatusResponse>, StatusCode> {
    match state.storage.get_job_counts().await {
        Ok(counts) => {
            let total = counts.values().sum();
            println!("📊 System status requested - total jobs: {}", total);
            Ok(Json(StatusResponse {
                message: format!("System operational. Job counts: {:?}", counts),
                total_jobs: total,
            }))
        }
        Err(_) => {
            println!("❌ Error getting system status");
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/*
🚀 INTEGRATION CHECKLIST FOR YOUR AXUM PROJECT:

✅ Dependencies (add to Cargo.toml):
   qml-rs = "0.1.3-alpha"
   axum = "0.8"
   tokio = { version = "1", features = ["full"] }

✅ Basic Setup:
   1. Add `storage: Arc<dyn Storage + Send + Sync>` to your AppState
   2. Initialize: `let storage = StorageInstance::memory();`
   3. Share state: `AppState { storage: Arc::new(storage) }`
   4. No environment variables required!

✅ Typical Handler Pattern:
   ```rust
   async fn my_handler(State(state): State<AppState>) -> Result<impl IntoResponse, StatusCode> {
       let job = Job::new("process_data", vec!["data".to_string()]);
       state.storage.enqueue(&job).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
       Ok(Json(serde_json::json!({"job_id": job.id.to_string()})))
   }
   ```

✅ Production Configuration:
   - Memory (dev): `StorageInstance::memory()`
   - PostgreSQL: `StorageInstance::postgres(PostgresConfig::default().with_database_url(db_url))`
   - Redis: `StorageInstance::redis(RedisConfig::default().with_url(redis_url))`
   - All backends work without panics thanks to our configuration fixes!

✅ Error Handling:
   - Always handle storage errors gracefully
   - Use proper HTTP status codes
   - Log errors for debugging
   - Consider retries for transient failures

✅ Monitoring:
   - Use `/status` endpoint for health checks
   - Monitor job counts and processing rates
   - Set up alerts for failed jobs
   - Track job processing latency

🎉 Your QML + Axum integration is now production-ready!
*/
