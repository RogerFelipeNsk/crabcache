//! Schema Registry for Protobuf
//! Phase 8.1 - Dynamic schema management and caching

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// Schema registry for managing Protobuf schemas
pub struct SchemaRegistry {
    schemas: Arc<RwLock<HashMap<String, Vec<u8>>>>,
    cache_size: usize,
}

impl SchemaRegistry {
    pub fn new(cache_size: usize) -> Self {
        Self {
            schemas: Arc::new(RwLock::new(HashMap::new())),
            cache_size,
        }
    }

    /// Register a schema
    pub fn register_schema(&self, name: String, schema_data: Vec<u8>) -> Result<(), String> {
        let mut schemas = self.schemas.write().map_err(|_| "Lock poisoned")?;

        if schemas.len() >= self.cache_size {
            // Simple eviction: remove oldest (first) entry
            if let Some(key) = schemas.keys().next().cloned() {
                schemas.remove(&key);
            }
        }

        schemas.insert(name, schema_data);
        Ok(())
    }

    /// Get a schema
    pub fn get_schema(&self, name: &str) -> Option<Vec<u8>> {
        let schemas = self.schemas.read().ok()?;
        schemas.get(name).cloned()
    }

    /// Check if schema exists
    pub fn has_schema(&self, name: &str) -> bool {
        if let Ok(schemas) = self.schemas.read() {
            schemas.contains_key(name)
        } else {
            false
        }
    }

    /// Get schema count
    pub fn schema_count(&self) -> usize {
        if let Ok(schemas) = self.schemas.read() {
            schemas.len()
        } else {
            0
        }
    }
}

impl Default for SchemaRegistry {
    fn default() -> Self {
        Self::new(100)
    }
}
