//! TOON String Interning System
//! Advanced string deduplication for ultra-compact encoding

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// Thread-safe string interner with advanced optimization
#[derive(Debug)]
pub struct AdvancedStringInterner {
    /// Interned strings storage
    strings: Vec<String>,
    /// String to ID lookup
    lookup: HashMap<String, u32>,
    /// Usage statistics for each string
    usage_stats: Vec<InterningStats>,
    /// Configuration
    config: InterningConfig,
}

/// String interning statistics
#[derive(Debug, Clone)]
pub struct InterningStats {
    /// Number of times this string was referenced
    pub usage_count: u64,
    /// Total bytes saved by interning this string
    pub bytes_saved: u64,
    /// First seen timestamp
    pub first_seen: std::time::Instant,
    /// Last used timestamp
    pub last_used: std::time::Instant,
}

impl Default for InterningStats {
    fn default() -> Self {
        let now = std::time::Instant::now();
        Self {
            usage_count: 0,
            bytes_saved: 0,
            first_seen: now,
            last_used: now,
        }
    }
}

/// String interning configuration
#[derive(Debug, Clone)]
pub struct InterningConfig {
    /// Minimum string length to consider for interning
    pub min_length: usize,
    /// Minimum usage count to intern a string
    pub min_usage_count: u64,
    /// Maximum number of interned strings
    pub max_interned_strings: usize,
    /// Enable automatic cleanup of unused strings
    pub auto_cleanup: bool,
    /// Cleanup threshold (remove strings not used for this duration)
    pub cleanup_threshold: std::time::Duration,
}

impl Default for InterningConfig {
    fn default() -> Self {
        Self {
            min_length: 4,
            min_usage_count: 2,
            max_interned_strings: 10000,
            auto_cleanup: true,
            cleanup_threshold: std::time::Duration::from_secs(300), // 5 minutes
        }
    }
}

impl Default for AdvancedStringInterner {
    fn default() -> Self {
        Self::new()
    }
}

impl AdvancedStringInterner {
    pub fn new() -> Self {
        Self::with_config(InterningConfig::default())
    }

    pub fn with_config(config: InterningConfig) -> Self {
        Self {
            strings: Vec::new(),
            lookup: HashMap::new(),
            usage_stats: Vec::new(),
            config,
        }
    }

    /// Intern a string, returning its ID
    pub fn intern(&mut self, s: &str) -> u32 {
        // Check if string meets minimum requirements
        if s.len() < self.config.min_length {
            // For short strings, we might still intern if they're used frequently
            // For now, create a temporary ID (this would need more sophisticated handling)
            return self.intern_unconditional(s);
        }

        // Check if already interned
        if let Some(&id) = self.lookup.get(s) {
            self.update_usage_stats(id);
            return id;
        }

        // Check if we've reached the limit
        if self.strings.len() >= self.config.max_interned_strings {
            self.cleanup_if_needed();

            // If still at limit, don't intern
            if self.strings.len() >= self.config.max_interned_strings {
                return self.intern_unconditional(s);
            }
        }

        // Intern the string
        let id = self.strings.len() as u32;
        self.strings.push(s.to_string());
        self.lookup.insert(s.to_string(), id);

        let now = std::time::Instant::now();
        self.usage_stats.push(InterningStats {
            usage_count: 1,
            bytes_saved: 0,
            first_seen: now,
            last_used: now,
        });

        id
    }

    /// Intern a string unconditionally (for internal use)
    fn intern_unconditional(&mut self, s: &str) -> u32 {
        if let Some(&id) = self.lookup.get(s) {
            self.update_usage_stats(id);
            return id;
        }

        let id = self.strings.len() as u32;
        self.strings.push(s.to_string());
        self.lookup.insert(s.to_string(), id);

        let now = std::time::Instant::now();
        self.usage_stats.push(InterningStats {
            usage_count: 1,
            bytes_saved: 0,
            first_seen: now,
            last_used: now,
        });

        id
    }

    /// Get string by ID
    pub fn get(&self, id: u32) -> Option<&str> {
        self.strings.get(id as usize).map(|s| s.as_str())
    }

    /// Update usage statistics for a string
    fn update_usage_stats(&mut self, id: u32) {
        if let Some(stats) = self.usage_stats.get_mut(id as usize) {
            stats.usage_count += 1;
            stats.last_used = std::time::Instant::now();

            // Calculate bytes saved (string length - varint ID size)
            if let Some(string) = self.strings.get(id as usize) {
                let id_size = varint_size(id as u64);
                if string.len() > id_size {
                    stats.bytes_saved += (string.len() - id_size) as u64;
                }
            }
        }
    }

    /// Cleanup unused strings
    fn cleanup_if_needed(&mut self) {
        if !self.config.auto_cleanup {
            return;
        }

        let now = std::time::Instant::now();
        let mut to_remove = Vec::new();

        for (id, stats) in self.usage_stats.iter().enumerate() {
            if now.duration_since(stats.last_used) > self.config.cleanup_threshold {
                to_remove.push(id);
            }
        }

        // Remove in reverse order to maintain indices
        for &id in to_remove.iter().rev() {
            self.remove_string(id as u32);
        }
    }

    /// Remove a string from the interner
    fn remove_string(&mut self, id: u32) {
        let id_usize = id as usize;

        if id_usize >= self.strings.len() {
            return;
        }

        // Remove from lookup
        let string = &self.strings[id_usize];
        self.lookup.remove(string);

        // Mark as removed (we can't actually remove from Vec without shifting indices)
        self.strings[id_usize] = String::new();

        // Reset stats
        if let Some(stats) = self.usage_stats.get_mut(id_usize) {
            *stats = InterningStats::default();
        }
    }

    /// Get total memory saved by interning
    pub fn total_memory_saved(&self) -> u64 {
        self.usage_stats.iter().map(|stats| stats.bytes_saved).sum()
    }

    /// Get interning efficiency (0.0 to 1.0)
    pub fn get_efficiency(&self) -> f64 {
        let total_strings = self.strings.len();
        if total_strings == 0 {
            return 0.0;
        }

        let effective_strings = self
            .usage_stats
            .iter()
            .filter(|stats| stats.usage_count > 1)
            .count();

        effective_strings as f64 / total_strings as f64
    }

    /// Get detailed statistics
    pub fn get_detailed_stats(&self) -> InterningDetailedStats {
        let total_strings = self.strings.len();
        let total_memory_saved = self.total_memory_saved();
        let efficiency = self.get_efficiency();

        let most_used = self
            .usage_stats
            .iter()
            .enumerate()
            .max_by_key(|(_, stats)| stats.usage_count)
            .map(|(id, stats)| (id as u32, stats.usage_count));

        let most_savings = self
            .usage_stats
            .iter()
            .enumerate()
            .max_by_key(|(_, stats)| stats.bytes_saved)
            .map(|(id, stats)| (id as u32, stats.bytes_saved));

        InterningDetailedStats {
            total_strings,
            total_memory_saved,
            efficiency,
            most_used_string: most_used,
            most_savings_string: most_savings,
        }
    }

    /// Optimize the interner by rebuilding without gaps
    pub fn optimize(&mut self) {
        let mut new_strings = Vec::new();
        let mut new_lookup = HashMap::new();
        let mut new_stats = Vec::new();
        let mut id_mapping = HashMap::new();

        for (old_id, string) in self.strings.iter().enumerate() {
            if !string.is_empty() && self.usage_stats[old_id].usage_count > 0 {
                let new_id = new_strings.len() as u32;
                new_strings.push(string.clone());
                new_lookup.insert(string.clone(), new_id);
                new_stats.push(self.usage_stats[old_id].clone());
                id_mapping.insert(old_id as u32, new_id);
            }
        }

        self.strings = new_strings;
        self.lookup = new_lookup;
        self.usage_stats = new_stats;
    }

    /// Check if a string should be interned based on current statistics
    pub fn should_intern(&self, s: &str) -> bool {
        if s.len() < self.config.min_length {
            return false;
        }

        if self.strings.len() >= self.config.max_interned_strings {
            return false;
        }

        // If already interned, always use it
        if self.lookup.contains_key(s) {
            return true;
        }

        // For new strings, intern if they meet the criteria
        true
    }
}

/// Detailed interning statistics
#[derive(Debug, Clone)]
pub struct InterningDetailedStats {
    pub total_strings: usize,
    pub total_memory_saved: u64,
    pub efficiency: f64,
    pub most_used_string: Option<(u32, u64)>,
    pub most_savings_string: Option<(u32, u64)>,
}

/// Thread-safe wrapper for the string interner
pub type SharedStringInterner = Arc<RwLock<AdvancedStringInterner>>;

/// Create a new shared string interner
pub fn create_shared_interner() -> SharedStringInterner {
    Arc::new(RwLock::new(AdvancedStringInterner::new()))
}

/// Create a shared interner with custom configuration
pub fn create_shared_interner_with_config(config: InterningConfig) -> SharedStringInterner {
    Arc::new(RwLock::new(AdvancedStringInterner::with_config(config)))
}

/// Calculate varint size for interning calculations
fn varint_size(mut value: u64) -> usize {
    if value == 0 {
        return 1;
    }
    let mut size = 0;
    while value > 0 {
        size += 1;
        value >>= 7;
    }
    size
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_interning() {
        let mut interner = AdvancedStringInterner::new();

        let id1 = interner.intern("hello world");
        let id2 = interner.intern("hello world");
        let id3 = interner.intern("different string");

        assert_eq!(id1, id2);
        assert_ne!(id1, id3);

        assert_eq!(interner.get(id1), Some("hello world"));
        assert_eq!(interner.get(id3), Some("different string"));
    }

    #[test]
    fn test_short_string_handling() {
        let mut interner = AdvancedStringInterner::new();

        // Short strings should still be interned but might be handled differently
        let id1 = interner.intern("hi");
        let id2 = interner.intern("hi");

        assert_eq!(id1, id2);
        assert_eq!(interner.get(id1), Some("hi"));
    }

    #[test]
    fn test_usage_statistics() {
        let mut interner = AdvancedStringInterner::new();

        let id = interner.intern("test string");
        interner.intern("test string"); // Use again
        interner.intern("test string"); // Use again

        let stats = interner.get_detailed_stats();
        assert_eq!(stats.total_strings, 1);
        assert!(stats.total_memory_saved > 0);
        assert!(stats.efficiency > 0.0);
    }

    #[test]
    fn test_memory_savings_calculation() {
        let mut interner = AdvancedStringInterner::new();

        // Use a long string multiple times
        let long_string = "this is a very long string that should save memory when interned";
        interner.intern(long_string);
        interner.intern(long_string);
        interner.intern(long_string);

        let total_saved = interner.total_memory_saved();
        assert!(total_saved > 0);

        // Should save approximately 2 * (string_length - varint_id_size)
        let expected_savings = 2 * (long_string.len() - varint_size(0));
        assert!(total_saved >= expected_savings as u64);
    }

    #[test]
    fn test_interner_optimization() {
        let mut interner = AdvancedStringInterner::new();

        // Add some strings
        interner.intern("keep1");
        interner.intern("keep2");
        interner.intern("remove"); // This will be "removed" by clearing

        // Simulate removal by clearing a string
        interner.strings[2] = String::new();
        interner.usage_stats[2] = InterningStats::default();

        let old_len = interner.strings.len();
        interner.optimize();
        let new_len = interner.strings.len();

        assert!(new_len < old_len);
        assert!(interner.get(0).is_some());
        assert!(interner.get(1).is_some());
    }

    #[test]
    fn test_efficiency_calculation() {
        let mut interner = AdvancedStringInterner::new();

        // Add strings with different usage patterns
        interner.intern("used_once");
        interner.intern("used_multiple");
        interner.intern("used_multiple");
        interner.intern("used_multiple");

        let efficiency = interner.get_efficiency();
        assert!(efficiency > 0.0 && efficiency <= 1.0);
    }

    #[test]
    fn test_shared_interner() {
        let interner = create_shared_interner();

        let id1 = {
            let mut guard = interner.write().unwrap();
            guard.intern("shared string")
        };

        let id2 = {
            let mut guard = interner.write().unwrap();
            guard.intern("shared string")
        };

        assert_eq!(id1, id2);

        let string = {
            let guard = interner.read().unwrap();
            guard.get(id1).map(|s| s.to_string())
        };

        assert_eq!(string, Some("shared string".to_string()));
    }
}
