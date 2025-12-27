//! TOON Protocol Type System
//! Advanced type definitions and optimizations

use super::ToonType;
use base64::Engine;
use bytes::Bytes;
use std::collections::HashMap;

/// TOON Type Analyzer - Chooses optimal encoding for values
pub struct ToonTypeAnalyzer;

impl ToonTypeAnalyzer {
    /// Analyze and optimize a value for TOON encoding
    pub fn optimize_value(value: &ToonType) -> ToonType {
        match value {
            ToonType::Int64(v) => {
                // Choose smallest integer type that fits
                if *v >= i8::MIN as i64 && *v <= i8::MAX as i64 {
                    ToonType::Int8(*v as i8)
                } else if *v >= i16::MIN as i64 && *v <= i16::MAX as i64 {
                    ToonType::Int16(*v as i16)
                } else if *v >= i32::MIN as i64 && *v <= i32::MAX as i64 {
                    ToonType::Int32(*v as i32)
                } else {
                    value.clone()
                }
            },
            ToonType::UInt64(v) => {
                // Choose smallest unsigned integer type that fits
                if *v <= u8::MAX as u64 {
                    ToonType::UInt8(*v as u8)
                } else if *v <= u16::MAX as u64 {
                    ToonType::UInt16(*v as u16)
                } else if *v <= u32::MAX as u64 {
                    ToonType::UInt32(*v as u32)
                } else {
                    value.clone()
                }
            },
            ToonType::Float64(v) => {
                // Use Float32 if precision loss is acceptable
                let f32_val = *v as f32;
                if (f32_val as f64 - v).abs() < f64::EPSILON {
                    ToonType::Float32(f32_val)
                } else {
                    value.clone()
                }
            },
            ToonType::Array(arr) => {
                // Optimize each element in the array
                let optimized: Vec<ToonType> = arr.iter()
                    .map(|item| Self::optimize_value(item))
                    .collect();
                ToonType::Array(optimized)
            },
            ToonType::Object(obj) => {
                // Optimize each value in the object
                let optimized: HashMap<String, ToonType> = obj.iter()
                    .map(|(k, v)| (k.clone(), Self::optimize_value(v)))
                    .collect();
                ToonType::Object(optimized)
            },
            _ => value.clone(),
        }
    }
    
    /// Calculate compression ratio for a value
    pub fn calculate_compression_ratio(original: &ToonType, optimized: &ToonType) -> f64 {
        let original_size = original.estimated_size() as f64;
        let optimized_size = optimized.estimated_size() as f64;
        
        if original_size == 0.0 {
            1.0
        } else {
            (original_size - optimized_size) / original_size
        }
    }
    
    /// Check if a value should be interned
    pub fn should_intern_string(s: &str, usage_count: usize) -> bool {
        // Intern if string is long enough and used multiple times
        s.len() > 4 && usage_count > 1
    }
}

/// TOON Value Builder - Fluent API for creating TOON values
pub struct ToonValueBuilder {
    value: ToonType,
}

impl ToonValueBuilder {
    pub fn new() -> Self {
        Self {
            value: ToonType::Null,
        }
    }
    
    pub fn null() -> Self {
        Self {
            value: ToonType::Null,
        }
    }
    
    pub fn bool(value: bool) -> Self {
        Self {
            value: ToonType::Bool(value),
        }
    }
    
    pub fn int(value: i64) -> Self {
        Self {
            value: ToonTypeAnalyzer::optimize_value(&ToonType::Int64(value)),
        }
    }
    
    pub fn uint(value: u64) -> Self {
        Self {
            value: ToonTypeAnalyzer::optimize_value(&ToonType::UInt64(value)),
        }
    }
    
    pub fn float(value: f64) -> Self {
        Self {
            value: ToonTypeAnalyzer::optimize_value(&ToonType::Float64(value)),
        }
    }
    
    pub fn string<S: Into<String>>(value: S) -> Self {
        Self {
            value: ToonType::String(value.into()),
        }
    }
    
    pub fn bytes<B: Into<Bytes>>(value: B) -> Self {
        Self {
            value: ToonType::Bytes(value.into()),
        }
    }
    
    pub fn array() -> ToonArrayBuilder {
        ToonArrayBuilder::new()
    }
    
    pub fn object() -> ToonObjectBuilder {
        ToonObjectBuilder::new()
    }
    
    pub fn build(self) -> ToonType {
        self.value
    }
}

impl Default for ToonValueBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// TOON Array Builder
pub struct ToonArrayBuilder {
    items: Vec<ToonType>,
}

impl ToonArrayBuilder {
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
        }
    }
    
    pub fn push(mut self, value: ToonType) -> Self {
        self.items.push(ToonTypeAnalyzer::optimize_value(&value));
        self
    }
    
    pub fn push_null(mut self) -> Self {
        self.items.push(ToonType::Null);
        self
    }
    
    pub fn push_bool(mut self, value: bool) -> Self {
        self.items.push(ToonType::Bool(value));
        self
    }
    
    pub fn push_int(mut self, value: i64) -> Self {
        self.items.push(ToonTypeAnalyzer::optimize_value(&ToonType::Int64(value)));
        self
    }
    
    pub fn push_string<S: Into<String>>(mut self, value: S) -> Self {
        self.items.push(ToonType::String(value.into()));
        self
    }
    
    pub fn build(self) -> ToonType {
        ToonType::Array(self.items)
    }
}

impl Default for ToonArrayBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// TOON Object Builder
pub struct ToonObjectBuilder {
    fields: HashMap<String, ToonType>,
}

impl ToonObjectBuilder {
    pub fn new() -> Self {
        Self {
            fields: HashMap::new(),
        }
    }
    
    pub fn field<K: Into<String>>(mut self, key: K, value: ToonType) -> Self {
        self.fields.insert(key.into(), ToonTypeAnalyzer::optimize_value(&value));
        self
    }
    
    pub fn field_null<K: Into<String>>(mut self, key: K) -> Self {
        self.fields.insert(key.into(), ToonType::Null);
        self
    }
    
    pub fn field_bool<K: Into<String>>(mut self, key: K, value: bool) -> Self {
        self.fields.insert(key.into(), ToonType::Bool(value));
        self
    }
    
    pub fn field_int<K: Into<String>>(mut self, key: K, value: i64) -> Self {
        self.fields.insert(key.into(), ToonTypeAnalyzer::optimize_value(&ToonType::Int64(value)));
        self
    }
    
    pub fn field_string<K: Into<String>, V: Into<String>>(mut self, key: K, value: V) -> Self {
        self.fields.insert(key.into(), ToonType::String(value.into()));
        self
    }
    
    pub fn field_bytes<K: Into<String>, V: Into<Bytes>>(mut self, key: K, value: V) -> Self {
        self.fields.insert(key.into(), ToonType::Bytes(value.into()));
        self
    }
    
    pub fn build(self) -> ToonType {
        ToonType::Object(self.fields)
    }
}

impl Default for ToonObjectBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// TOON Type Utilities
pub struct ToonTypeUtils;

impl ToonTypeUtils {
    /// Convert JSON-like structure to TOON
    pub fn from_json_value(value: &serde_json::Value) -> ToonType {
        match value {
            serde_json::Value::Null => ToonType::Null,
            serde_json::Value::Bool(b) => ToonType::Bool(*b),
            serde_json::Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    ToonTypeAnalyzer::optimize_value(&ToonType::Int64(i))
                } else if let Some(u) = n.as_u64() {
                    ToonTypeAnalyzer::optimize_value(&ToonType::UInt64(u))
                } else if let Some(f) = n.as_f64() {
                    ToonTypeAnalyzer::optimize_value(&ToonType::Float64(f))
                } else {
                    ToonType::Null
                }
            },
            serde_json::Value::String(s) => ToonType::String(s.clone()),
            serde_json::Value::Array(arr) => {
                let toon_array: Vec<ToonType> = arr.iter()
                    .map(|v| Self::from_json_value(v))
                    .collect();
                ToonType::Array(toon_array)
            },
            serde_json::Value::Object(obj) => {
                let toon_object: HashMap<String, ToonType> = obj.iter()
                    .map(|(k, v)| (k.clone(), Self::from_json_value(v)))
                    .collect();
                ToonType::Object(toon_object)
            },
        }
    }
    
    /// Convert TOON to JSON-like structure
    pub fn to_json_value(value: &ToonType) -> serde_json::Value {
        match value {
            ToonType::Null => serde_json::Value::Null,
            ToonType::Bool(b) => serde_json::Value::Bool(*b),
            ToonType::Int8(i) => serde_json::Value::Number((*i as i64).into()),
            ToonType::Int16(i) => serde_json::Value::Number((*i as i64).into()),
            ToonType::Int32(i) => serde_json::Value::Number((*i as i64).into()),
            ToonType::Int64(i) => serde_json::Value::Number((*i).into()),
            ToonType::UInt8(u) => serde_json::Value::Number((*u as u64).into()),
            ToonType::UInt16(u) => serde_json::Value::Number((*u as u64).into()),
            ToonType::UInt32(u) => serde_json::Value::Number((*u as u64).into()),
            ToonType::UInt64(u) => serde_json::Value::Number((*u).into()),
            ToonType::Float32(f) => serde_json::Value::Number(
                serde_json::Number::from_f64(*f as f64).unwrap_or_else(|| 0.into())
            ),
            ToonType::Float64(f) => serde_json::Value::Number(
                serde_json::Number::from_f64(*f).unwrap_or_else(|| 0.into())
            ),
            ToonType::String(s) => serde_json::Value::String(s.clone()),
            ToonType::Bytes(b) => {
                // Convert bytes to base64 string
                serde_json::Value::String(base64::prelude::BASE64_STANDARD.encode(b))
            },
            ToonType::Array(arr) => {
                let json_array: Vec<serde_json::Value> = arr.iter()
                    .map(|v| Self::to_json_value(v))
                    .collect();
                serde_json::Value::Array(json_array)
            },
            ToonType::Object(obj) => {
                let json_object: serde_json::Map<String, serde_json::Value> = obj.iter()
                    .map(|(k, v)| (k.clone(), Self::to_json_value(v)))
                    .collect();
                serde_json::Value::Object(json_object)
            },
            ToonType::InternedString(_) => {
                // This should be resolved by the decoder
                serde_json::Value::String("INTERNED_STRING".to_string())
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_type_optimization() {
        // Test integer optimization
        let large_int = ToonType::Int64(42);
        let optimized = ToonTypeAnalyzer::optimize_value(&large_int);
        assert_eq!(optimized, ToonType::Int8(42));
        
        // Test float optimization
        let large_float = ToonType::Float64(3.14);
        let optimized = ToonTypeAnalyzer::optimize_value(&large_float);
        assert_eq!(optimized, ToonType::Float32(3.14));
    }
    
    #[test]
    fn test_value_builder() {
        let value = ToonValueBuilder::int(42).build();
        assert_eq!(value, ToonType::Int8(42));
        
        let value = ToonValueBuilder::string("hello").build();
        assert_eq!(value, ToonType::String("hello".to_string()));
    }
    
    #[test]
    fn test_array_builder() {
        let array = ToonArrayBuilder::new()
            .push_int(1)
            .push_int(2)
            .push_string("test")
            .build();
            
        match array {
            ToonType::Array(arr) => {
                assert_eq!(arr.len(), 3);
                assert_eq!(arr[0], ToonType::Int8(1));
                assert_eq!(arr[1], ToonType::Int8(2));
                assert_eq!(arr[2], ToonType::String("test".to_string()));
            },
            _ => panic!("Expected array"),
        }
    }
    
    #[test]
    fn test_object_builder() {
        let obj = ToonObjectBuilder::new()
            .field_string("name", "Alice")
            .field_int("age", 30)
            .field_bool("active", true)
            .build();
            
        match obj {
            ToonType::Object(map) => {
                assert_eq!(map.len(), 3);
                assert_eq!(map["name"], ToonType::String("Alice".to_string()));
                assert_eq!(map["age"], ToonType::Int8(30));
                assert_eq!(map["active"], ToonType::Bool(true));
            },
            _ => panic!("Expected object"),
        }
    }
    
    #[test]
    fn test_compression_ratio() {
        let original = ToonType::Int64(42);
        let optimized = ToonTypeAnalyzer::optimize_value(&original);
        let ratio = ToonTypeAnalyzer::calculate_compression_ratio(&original, &optimized);
        
        // Should have some compression due to smaller integer type
        assert!(ratio > 0.0);
    }
}