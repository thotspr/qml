//! Integration test to verify that config structs no longer panic
//! when environment variables are not set.
//!
//! This test ensures our panic fix is working correctly.

use qml_rs::storage::MemoryConfig;

#[test]
fn test_memory_config_no_panic() {
    // Test Memory config (should always work)
    let memory_config = MemoryConfig::default();
    assert_eq!(memory_config.max_jobs, Some(10_000));
    assert!(memory_config.auto_cleanup);
    println!(
        "✅ MemoryConfig::default() works: max_jobs = {:?}",
        memory_config.max_jobs
    );
}

#[test]
#[cfg(feature = "redis")]
fn test_redis_config_no_panic() {
    use qml_rs::storage::RedisConfig;

    // This should NOT panic even without REDIS_URL environment variable
    let redis_config = RedisConfig::default();
    assert_eq!(redis_config.url, "redis://localhost:6379");
    assert_eq!(redis_config.pool_size, 10);
    assert_eq!(redis_config.key_prefix, "qml");
    println!(
        "✅ RedisConfig::default() works without REDIS_URL: {}",
        redis_config.url
    );
}

#[test]
#[cfg(feature = "postgres")]
fn test_postgres_config_no_panic() {
    use qml_rs::storage::PostgresConfig;

    // This should NOT panic even without DATABASE_URL environment variable
    let postgres_config = PostgresConfig::default();
    assert_eq!(
        postgres_config.database_url,
        "postgresql://postgres:password@localhost:5432/qml"
    );
    assert_eq!(postgres_config.max_connections, 20);
    assert_eq!(postgres_config.schema_name, "qml");
    println!(
        "✅ PostgresConfig::default() works without DATABASE_URL: {}",
        postgres_config.database_url
    );
}

#[test]
fn test_all_configs_work_together() {
    println!("🧪 Testing that all config structs work without environment variables...");

    // Memory config should always work
    let _memory_config = MemoryConfig::default();
    println!("✅ Memory config created");

    // Redis config should work with defaults
    #[cfg(feature = "redis")]
    {
        use qml_rs::storage::RedisConfig;
        let _redis_config = RedisConfig::default();
        println!("✅ Redis config created with defaults");
    }

    // Postgres config should work with defaults
    #[cfg(feature = "postgres")]
    {
        use qml_rs::storage::PostgresConfig;
        let _postgres_config = PostgresConfig::default();
        println!("✅ Postgres config created with defaults");
    }

    println!("🎉 All config structs work without panicking!");
}
