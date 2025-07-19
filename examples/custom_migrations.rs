//! Custom Migration Paths Example
//!
//! This example demonstrates different ways to configure custom migration paths:
//! - Using PostgresConfig::with_migrations_path()
//! - Using environment variables
//! - Running migrations manually

#[cfg(feature = "postgres")]
use qml_rs::storage::{PostgresConfig, settings::Settings};

#[cfg(feature = "postgres")]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 Custom Migration Paths Demo");

    // Method 1: Direct configuration with custom path
    println!("\n📁 Method 1: Direct configuration");
    let config1 = PostgresConfig::with_defaults()
        .with_database_url("postgresql://postgres:password@localhost:5432/qml_test")
        .with_migrations_path("./custom_migrations")
        .with_auto_migrate(false); // Disable auto-migrate for manual control

    println!("  📝 Config created with custom path: {}", config1.migrations_path);

    // Method 2: Using environment variables
    println!("\n🌍 Method 2: Environment variable configuration");
    unsafe {
        std::env::set_var("QML_MIGRATIONS_PATH", "./env_migrations");
        std::env::set_var("DATABASE_URL", "postgresql://postgres:password@localhost:5432/qml_test");
    }
    
    let settings = Settings::from_env()?;
    let config2 = PostgresConfig::with_defaults()
        .with_database_url("postgresql://postgres:password@localhost:5432/qml_test")
        .with_migrations_path(&settings.migrations_path)
        .with_auto_migrate(false);

    println!("  📝 Config created from env var: {}", config2.migrations_path);

    // Method 3: Default path (when not specified)
    println!("\n⚙️  Method 3: Default path");
    let config3 = PostgresConfig::with_defaults()
        .with_database_url("postgresql://postgres:password@localhost:5432/qml_test")
        .with_auto_migrate(false);

    println!("  📝 Default migration path: {}", config3.migrations_path);

    // For demonstration purposes, we'll just show the configs
    // In real usage, you would create the storage and run migrations:
    /*
    println!("\n🔗 Creating storage with custom migration path...");
    let storage = PostgresStorage::new(config1).await?;
    
    println!("🔄 Running migrations manually...");
    storage.migrate().await?;
    
    println!("✅ Migrations completed!");
    */

    println!("\n📋 Summary of configuration methods:");
    println!("  1. Direct: config.with_migrations_path(\"./custom_migrations\")");
    println!("  2. Environment: QML_MIGRATIONS_PATH=./env_migrations");
    println!("  3. Default: ./migrations (when not specified)");
    
    println!("\n💡 Tips:");
    println!("  • Use relative paths from your project root");
    println!("  • Ensure migration files exist in the specified directory");
    println!("  • Set auto_migrate(false) for manual migration control");
    println!("  • Call storage.migrate() manually when needed");

    println!("\n✅ Custom migration paths demo completed!");

    Ok(())
}

#[cfg(not(feature = "postgres"))]
fn main() {
    println!("This example requires the 'postgres' feature to be enabled.");
    println!("Run with: cargo run --example custom_migrations --features postgres");
}
