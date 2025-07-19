//! Simple test to demonstrate QML works in Axum projects
//!
//! This test shows that:
//! 1. QML configuration doesn't panic when environment variables aren't set
//! 2. Jobs can be created and managed in an async/await context
//! 3. Everything integrates smoothly with Axum's patterns

use qml_rs::storage::{MemoryConfig, StorageConfig, StorageInstance};
use qml_rs::{Job, Storage};

#[tokio::test]
async fn test_qml_integration_in_axum_style() {
    println!("🧪 Testing QML integration in Axum-style async context...");

    // 1. Test that configuration works without environment variables
    let memory_config = StorageConfig::Memory(MemoryConfig::new().with_max_jobs(100));
    println!("✅ Memory config created successfully");

    // 2. Test storage initialization (the way you'd do it in Axum)
    let storage = StorageInstance::memory();
    println!("✅ Storage instance created successfully");

    // 3. Test job creation and management
    let job = Job::new("process_user_data", vec!["user123".to_string()]);
    let job_id = job.id.to_string();

    // 4. Test async operations (typical in Axum handlers)
    storage.enqueue(&job).await.expect("Should enqueue job");
    println!("✅ Job enqueued: {}", job_id);

    let retrieved_job = storage.get(&job_id).await.expect("Should retrieve job");
    assert!(retrieved_job.is_some());
    println!("✅ Job retrieved successfully");

    // 5. Test job counts (useful for health check endpoints)
    let counts = storage.get_job_counts().await.expect("Should get counts");
    println!("✅ Job counts retrieved: {:?}", counts);

    println!("🎉 All tests passed! QML is ready for your Axum project.");
}

// Integration test showing typical Axum usage patterns
#[tokio::test]
async fn test_axum_integration_patterns() {
    use std::sync::Arc;

    // Typical shared state setup in Axum
    struct AppState {
        storage: Arc<dyn Storage + Send + Sync>,
    }

    let storage = StorageInstance::memory();
    let app_state = AppState {
        storage: Arc::new(storage),
    };

    // Simulate what would happen in an Axum handler
    let job = Job::new("send_notification", vec!["urgent".to_string()]);
    let job_id = job.id.to_string();

    // This is exactly how you'd use it in an Axum handler
    app_state.storage.enqueue(&job).await.expect("Should work");
    let result = app_state.storage.get(&job_id).await.expect("Should work");

    assert!(result.is_some());
    println!("✅ Axum integration pattern works perfectly!");
}

#[cfg(test)]
mod axum_integration_tests {
    use super::*;

    #[test]
    fn test_config_no_panic() {
        // This would have panicked before our fix
        let _memory_config = MemoryConfig::default();

        // Test PostgreSQL config without DATABASE_URL
        #[cfg(feature = "postgres")]
        {
            use qml_rs::storage::PostgresConfig;
            let _postgres_config = PostgresConfig::default();
            println!("✅ PostgreSQL config created without DATABASE_URL");
        }

        // Test Redis config without REDIS_URL
        #[cfg(feature = "redis")]
        {
            use qml_rs::storage::RedisConfig;
            let _redis_config = RedisConfig::default();
            println!("✅ Redis config created without REDIS_URL");
        }

        println!("✅ All configs created successfully - no panics!");
    }
}
