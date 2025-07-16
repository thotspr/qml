//! Simple PostgreSQL Storage Example
//!
//! This example demonstrates basic PostgreSQL storage operations:
//! - Connecting to PostgreSQL
//! - Running migrations
//! - Creating and storing jobs
//! - Retrieving jobs and statistics

#[cfg(feature = "postgres")]
use qml::{Job, JobState, PostgresConfig, PostgresStorage, Storage};

#[cfg(feature = "postgres")]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 Starting Simple PostgreSQL Storage Demo");

    // Configure PostgreSQL connection
    let config = PostgresConfig::new()
        .with_database_url("postgresql://postgres:password@localhost:5432/qml")
        .with_max_connections(10)
        .with_auto_migrate(true);

    // Create storage instance
    println!("🔗 Connecting to PostgreSQL...");
    let storage = PostgresStorage::new(config).await?;
    println!("✅ Connected successfully!");

    // Create sample jobs
    println!("📝 Creating sample jobs...");

    let job1 = Job::new("send_email", vec!["user@example.com".to_string()]);
    let job2 = Job::with_config(
        "process_payment",
        vec!["order-123".to_string(), "99.99".to_string()],
        "payments",
        5,
        3,
    );
    let mut job3 = Job::new("generate_report", vec!["monthly".to_string()]);
    job3.queue = "reports".to_string();

    // Store jobs
    storage.enqueue(&job1).await?;
    storage.enqueue(&job2).await?;
    storage.enqueue(&job3).await?;

    println!("✅ Created 3 jobs");

    // Create a scheduled job
    let mut scheduled_job = Job::new("cleanup_task", vec![]);
    scheduled_job.state = JobState::scheduled(
        chrono::Utc::now() + chrono::Duration::minutes(30),
        "Daily cleanup".to_string(),
    );
    storage.enqueue(&scheduled_job).await?;
    println!("⏰ Created 1 scheduled job");

    // Retrieve jobs
    println!("\n📋 Job Statistics:");
    let counts = storage.get_job_counts().await?;
    for (state, count) in counts {
        println!("  {}: {}", state.name(), count);
    }

    // List all jobs
    println!("\n📑 All Jobs:");
    let all_jobs = storage.list(None, Some(10), None).await?;
    for job in &all_jobs {
        println!("  {} - {} ({})", &job.id[..8], job.method, job.state.name());
    }

    // Get available jobs
    println!("\n⚡ Available Jobs for Processing:");
    let available = storage.get_available_jobs(Some(5)).await?;
    for job in &available {
        println!("  {} - {} (Queue: {})", &job.id[..8], job.method, job.queue);
    }

    // Update a job state (simulate processing)
    if let Some(job) = all_jobs.first() {
        let mut updated_job = job.clone();
        updated_job.state = JobState::processing("worker-1", "server-1");
        storage.update(&updated_job).await?;
        println!("🔄 Updated job {} to Processing state", &job.id[..8]);
    }

    // Final statistics
    println!("\n📊 Final Statistics:");
    let final_counts = storage.get_job_counts().await?;
    for (state, count) in final_counts {
        println!("  {}: {}", state.name(), count);
    }

    println!("\n✅ PostgreSQL demo completed successfully!");

    Ok(())
}

#[cfg(not(feature = "postgres"))]
fn main() {
    println!("This example requires the 'postgres' feature to be enabled.");
    println!("Run with: cargo run --example postgres_simple --features postgres");
}
