//! Job activator for method invocation
//!
//! This module contains the JobActivator that provides a mechanism to invoke
//! job methods dynamically. This is a foundation for future reflection-based
//! job execution systems.

use crate::error::{QmlError, Result};
use std::collections::HashMap;

/// Trait for activating and invoking job methods
///
/// This trait provides an abstraction for different ways of invoking job methods.
/// Implementations might use reflection, function registries, or other mechanisms.
pub trait JobActivator: Send + Sync {
    /// Invoke a job method with the given arguments
    fn invoke(&self, method: &str, arguments: &[String]) -> Result<String>;

    /// Check if a method can be activated
    fn can_activate(&self, method: &str) -> bool;
}

/// Simple function-based job activator
///
/// This activator uses a registry of function pointers to invoke job methods.
/// It's a simple implementation suitable for basic use cases.
pub struct FunctionActivator {
    functions: HashMap<String, Box<dyn Fn(&[String]) -> Result<String> + Send + Sync>>,
}

impl FunctionActivator {
    /// Create a new function activator
    pub fn new() -> Self {
        Self {
            functions: HashMap::new(),
        }
    }

    /// Register a function for a method name
    pub fn register<F>(&mut self, method: String, function: F)
    where
        F: Fn(&[String]) -> Result<String> + Send + Sync + 'static,
    {
        self.functions.insert(method, Box::new(function));
    }

    /// Get the number of registered functions
    pub fn len(&self) -> usize {
        self.functions.len()
    }

    /// Check if any functions are registered
    pub fn is_empty(&self) -> bool {
        self.functions.is_empty()
    }
}

impl Default for FunctionActivator {
    fn default() -> Self {
        Self::new()
    }
}

impl JobActivator for FunctionActivator {
    fn invoke(&self, method: &str, arguments: &[String]) -> Result<String> {
        match self.functions.get(method) {
            Some(function) => function(arguments),
            None => Err(QmlError::WorkerError {
                message: format!("No function registered for method: {}", method),
            }),
        }
    }

    fn can_activate(&self, method: &str) -> bool {
        self.functions.contains_key(method)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_function_activator() {
        let mut activator = FunctionActivator::new();

        // Register a simple function
        activator.register("greet".to_string(), |args| {
            if args.is_empty() {
                Ok("Hello, World!".to_string())
            } else {
                Ok(format!("Hello, {}!", args[0]))
            }
        });

        // Test function invocation
        assert!(activator.can_activate("greet"));
        assert!(!activator.can_activate("unknown"));

        let result = activator.invoke("greet", &[]).unwrap();
        assert_eq!(result, "Hello, World!");

        let result = activator.invoke("greet", &["Alice".to_string()]).unwrap();
        assert_eq!(result, "Hello, Alice!");

        // Test unknown method
        assert!(activator.invoke("unknown", &[]).is_err());
    }
}
