//! Configuration settings loaded from environment variables
//!
//! This module provides a centralized way to load application settings
//! from environment variables using serde::Deserialize and std::env.

use serde::Deserialize;
use std::env;

/// Application settings loaded from environment variables
#[derive(Debug, Deserialize)]
pub struct Settings {
    /// PostgreSQL database URL
    pub database_url: String,

    /// Redis URL
    pub redis_url: String,

    /// Dashboard server port
    pub dashboard_port: u16,
    /// Dashboard server host
    pub dashboard_host: String,

    /// Maximum number of workers
    pub max_workers: u32,
    /// Maximum database connections
    pub max_connections: u32,
    /// Log level
    pub log_level: String,

    /// Auto migrate flag
    pub auto_migrate: bool,
    /// Require SSL flag
    pub require_ssl: bool,
}

impl Settings {
    /// Load settings from environment variables
    pub fn from_env() -> Result<Self, Box<dyn std::error::Error>> {
        // Helper function to get env var with expect
        let get_env = |key: &str| -> String {
            env::var(key).unwrap_or_else(|_| panic!("Environment variable {} must be set", key))
        };

        // Helper function to parse env var
        let parse_env = |key: &str, default: &str| -> Result<String, Box<dyn std::error::Error>> {
            Ok(env::var(key).unwrap_or_else(|_| default.to_string()))
        };

        // Helper function to parse numeric env var
        let parse_env_num = |key: &str, default: &str| -> Result<u32, Box<dyn std::error::Error>> {
            let value = env::var(key).unwrap_or_else(|_| default.to_string());
            Ok(value.parse()?)
        };

        // Helper function to parse u16 env var
        let parse_env_u16 = |key: &str, default: &str| -> Result<u16, Box<dyn std::error::Error>> {
            let value = env::var(key).unwrap_or_else(|_| default.to_string());
            Ok(value.parse()?)
        };

        // Helper function to parse bool env var
        let parse_env_bool =
            |key: &str, default: &str| -> Result<bool, Box<dyn std::error::Error>> {
                let value = env::var(key).unwrap_or_else(|_| default.to_string());
                match value.to_lowercase().as_str() {
                    "true" | "yes" | "1" | "on" => Ok(true),
                    "false" | "no" | "0" | "off" => Ok(false),
                    _ => Err(format!("Invalid boolean value for {}: {}", key, value).into()),
                }
            };

        Ok(Settings {
            database_url: get_env("DATABASE_URL"),
            redis_url: parse_env("REDIS_URL", "redis://localhost:6379")?,
            dashboard_port: parse_env_u16("QML_DASHBOARD_PORT", "8080")?,
            dashboard_host: parse_env("QML_DASHBOARD_HOST", "127.0.0.1")?,
            max_workers: parse_env_num("QML_MAX_WORKERS", "10")?,
            max_connections: parse_env_num("QML_MAX_CONNECTIONS", "20")?,
            log_level: parse_env("QML_LOG_LEVEL", "info")?,
            // Optional settings with defaults
            auto_migrate: parse_env_bool("QML_AUTO_MIGRATE", "true")?,
            require_ssl: parse_env_bool("QML_REQUIRE_SSL", "false")?,
        })
    }

    /// Load settings with defaults for development
    pub fn from_env_with_defaults() -> Self {
        // Try to load .env file if it exists (optional)
        Self::load_dotenv_if_exists();

        // Set default environment variables if not present
        // IMPORTANT: These are DEVELOPMENT-ONLY defaults for testing/examples
        // NEVER use these values in production - always set proper environment variables
        if env::var("DATABASE_URL").is_err() {
            unsafe {
                env::set_var(
                    "DATABASE_URL",
                    "postgresql://qml_user:password@localhost:5432/qml_db",
                );
            }
        }
        if env::var("REDIS_URL").is_err() {
            unsafe {
                env::set_var("REDIS_URL", "redis://localhost:6379");
            }
        }

        Self::from_env().expect("Failed to load settings even with defaults")
    }

    /// Load .env file if it exists (basic implementation without external dependencies)
    fn load_dotenv_if_exists() {
        use std::fs;
        use std::path::Path;

        let env_path = Path::new(".env");
        if !env_path.exists() {
            return; // .env file is optional
        }

        if let Ok(content) = fs::read_to_string(env_path) {
            for line in content.lines() {
                let line = line.trim();

                // Skip empty lines and comments
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }

                // Parse KEY=VALUE format
                if let Some((key, value)) = line.split_once('=') {
                    let key = key.trim();
                    let value = value.trim();

                    // Remove quotes if present
                    let value = if (value.starts_with('"') && value.ends_with('"'))
                        || (value.starts_with('\'') && value.ends_with('\''))
                    {
                        &value[1..value.len() - 1]
                    } else {
                        value
                    };

                    // Only set if not already set in environment
                    if env::var(key).is_err() {
                        unsafe {
                            env::set_var(key, value);
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_settings_from_env_with_defaults() {
        let settings = Settings::from_env_with_defaults();
        assert!(!settings.database_url.is_empty());
        assert_eq!(settings.dashboard_port, 8080);
    }
}
