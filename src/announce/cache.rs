//! Announce cache for duplicate detection.
//!
//! Reticulum announces are broadcast packets that propagate through the network.
//! Each transport node must track which announces it has already seen to:
//! 1. Avoid rebroadcasting the same announce multiple times
//! 2. Prevent infinite loops in the network
//! 3. Track the best (lowest hop count) path for each destination
//!
//! This implementation uses an LRU (Least Recently Used) eviction policy when
//! the cache reaches capacity, ensuring bounded memory usage.

use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Hash identifying an announce (typically 16 bytes in Reticulum).
pub type AnnounceHash = [u8; 16];

/// Configuration for the announce cache.
///
/// Note: This is `Copy` for efficient passing to constructors.
#[derive(Debug, Clone, Copy)]
pub struct AnnounceCacheConfig {
    /// Maximum number of entries in the cache.
    pub max_entries: usize,
    /// Time-to-live for cache entries.
    pub ttl: Duration,
}

impl Default for AnnounceCacheConfig {
    fn default() -> Self {
        Self {
            max_entries: 256,
            ttl: Duration::from_secs(3600), // 1 hour
        }
    }
}

impl AnnounceCacheConfig {
    /// Validate configuration parameters.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - `max_entries` is 0
    /// - `ttl` is 0
    pub fn validate(&self) -> Result<(), AnnounceCacheError> {
        if self.max_entries == 0 {
            return Err(AnnounceCacheError::InvalidConfig(
                "max_entries must be greater than 0",
            ));
        }
        if self.ttl.is_zero() {
            return Err(AnnounceCacheError::InvalidConfig(
                "ttl must be greater than 0",
            ));
        }
        Ok(())
    }
}

/// Error type for announce cache operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnnounceCacheError {
    /// Invalid configuration parameter.
    InvalidConfig(&'static str),
}

impl std::fmt::Display for AnnounceCacheError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidConfig(msg) => write!(f, "invalid config: {}", msg),
        }
    }
}

impl std::error::Error for AnnounceCacheError {}

/// Entry stored in the announce cache.
#[derive(Debug, Clone)]
pub struct AnnounceEntry {
    /// When this announce was first seen.
    pub first_seen: Instant,
    /// When this entry was last accessed (for LRU eviction).
    pub last_accessed: Instant,
    /// Hop count when received (lower is better).
    pub hops: u8,
    /// Number of times this announce has been seen.
    pub seen_count: u32,
}

impl AnnounceEntry {
    fn new(hops: u8, now: Instant) -> Self {
        Self {
            first_seen: now,
            last_accessed: now,
            hops,
            seen_count: 1,
        }
    }
}

/// Result of inserting an announce into the cache.
#[derive(Debug, Clone, PartialEq, Eq)]
#[must_use = "ignoring insert result may cause duplicate rebroadcasts"]
pub enum InsertResult {
    /// First time seeing this announce - should rebroadcast.
    New,
    /// Seen before with same or worse hop count - don't rebroadcast.
    Duplicate,
    /// Seen before but with better (lower) hop count - should rebroadcast.
    BetterPath {
        /// Previous hop count.
        old_hops: u8,
        /// New (better) hop count.
        new_hops: u8,
    },
}

/// LRU cache for tracking recently seen announces.
///
/// This cache helps transport nodes decide whether to rebroadcast an announce:
/// - New announces should be rebroadcast (with incremented hop count)
/// - Previously seen announces should be dropped (duplicate)
/// - Announces with a better path (lower hops) should be rebroadcast
///
/// # Example
///
/// ```
/// use reticulum_rs_esp32::announce::{AnnounceCache, AnnounceCacheConfig, InsertResult};
///
/// let config = AnnounceCacheConfig::default();
/// let mut cache = AnnounceCache::new(config).unwrap();
///
/// let hash = [0u8; 16];
///
/// // First time seeing this announce
/// assert_eq!(cache.insert(hash, 3), InsertResult::New);
///
/// // Duplicate - same or worse hop count
/// assert_eq!(cache.insert(hash, 4), InsertResult::Duplicate);
///
/// // Better path found
/// assert_eq!(
///     cache.insert(hash, 2),
///     InsertResult::BetterPath { old_hops: 3, new_hops: 2 }
/// );
/// ```
pub struct AnnounceCache {
    config: AnnounceCacheConfig,
    entries: HashMap<AnnounceHash, AnnounceEntry>,
}

impl Default for AnnounceCache {
    fn default() -> Self {
        Self::new(AnnounceCacheConfig::default()).expect("default config should be valid")
    }
}

impl AnnounceCache {
    /// Create a new announce cache with the given configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if the configuration is invalid.
    pub fn new(config: AnnounceCacheConfig) -> Result<Self, AnnounceCacheError> {
        config.validate()?;
        Ok(Self {
            config,
            entries: HashMap::with_capacity(config.max_entries),
        })
    }

    /// Insert an announce into the cache.
    ///
    /// Returns whether this is a new announce, duplicate, or better path.
    /// This helps determine whether to rebroadcast the announce.
    pub fn insert(&mut self, hash: AnnounceHash, hops: u8) -> InsertResult {
        let now = Instant::now();

        // First, clean up expired entries if we're at capacity
        if self.entries.len() >= self.config.max_entries {
            self.evict_expired_or_lru(now);
        }

        if let Some(entry) = self.entries.get_mut(&hash) {
            entry.last_accessed = now;
            entry.seen_count = entry.seen_count.saturating_add(1);

            if hops < entry.hops {
                let old_hops = entry.hops;
                entry.hops = hops;
                InsertResult::BetterPath {
                    old_hops,
                    new_hops: hops,
                }
            } else {
                InsertResult::Duplicate
            }
        } else {
            // New entry - may need to evict if still at capacity
            if self.entries.len() >= self.config.max_entries {
                self.evict_lru();
            }

            self.entries.insert(hash, AnnounceEntry::new(hops, now));
            InsertResult::New
        }
    }

    /// Check if an announce is in the cache (without updating access time).
    pub fn contains(&self, hash: &AnnounceHash) -> bool {
        self.entries.contains_key(hash)
    }

    /// Get an announce entry (updates last_accessed time).
    pub fn get(&mut self, hash: &AnnounceHash) -> Option<&AnnounceEntry> {
        let now = Instant::now();
        if let Some(entry) = self.entries.get_mut(hash) {
            entry.last_accessed = now;
            Some(entry)
        } else {
            None
        }
    }

    /// Get an announce entry without updating access time.
    pub fn peek(&self, hash: &AnnounceHash) -> Option<&AnnounceEntry> {
        self.entries.get(hash)
    }

    /// Remove an announce from the cache.
    pub fn remove(&mut self, hash: &AnnounceHash) -> Option<AnnounceEntry> {
        self.entries.remove(hash)
    }

    /// Clear all entries from the cache.
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Get the number of entries in the cache.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if the cache is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Get the cache configuration.
    pub fn config(&self) -> &AnnounceCacheConfig {
        &self.config
    }

    /// Remove all expired entries.
    ///
    /// Returns the number of entries removed.
    pub fn cleanup_expired(&mut self) -> usize {
        let now = Instant::now();
        let ttl = self.config.ttl;
        let before = self.entries.len();

        self.entries
            .retain(|_, entry| now.duration_since(entry.first_seen) < ttl);

        before - self.entries.len()
    }

    /// Evict expired entries, or LRU entry if none expired.
    fn evict_expired_or_lru(&mut self, now: Instant) {
        let ttl = self.config.ttl;

        // First try to remove expired entries
        let before = self.entries.len();
        self.entries
            .retain(|_, entry| now.duration_since(entry.first_seen) < ttl);

        // If nothing was removed, evict the LRU entry
        if self.entries.len() == before && !self.entries.is_empty() {
            self.evict_lru();
        }
    }

    /// Evict the least recently used entry.
    ///
    /// This performs a full scan to find the LRU entry (O(n) in number of entries).
    /// For the expected cache size (~256 entries), this is acceptable.
    fn evict_lru(&mut self) {
        if self.entries.is_empty() {
            return;
        }

        let lru_hash = self
            .entries
            .iter()
            .min_by_key(|(_, entry)| entry.last_accessed)
            .map(|(hash, _)| *hash);

        if let Some(hash) = lru_hash {
            self.entries.remove(&hash);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use reticulum_rs_esp32_macros::esp32_test;

    fn make_hash(id: u8) -> AnnounceHash {
        let mut hash = [0u8; 16];
        hash[0] = id;
        hash
    }

    #[esp32_test]
    fn test_new_cache() {
        let config = AnnounceCacheConfig::default();
        let cache = AnnounceCache::new(config).unwrap();
        assert!(cache.is_empty());
        assert_eq!(cache.len(), 0);
    }

    #[esp32_test]
    fn test_invalid_config_zero_entries() {
        let config = AnnounceCacheConfig {
            max_entries: 0,
            ttl: Duration::from_secs(60),
        };
        let result = AnnounceCache::new(config);
        assert!(matches!(result, Err(AnnounceCacheError::InvalidConfig(_))));
    }

    #[esp32_test]
    fn test_invalid_config_zero_ttl() {
        let config = AnnounceCacheConfig {
            max_entries: 100,
            ttl: Duration::ZERO,
        };
        let result = AnnounceCache::new(config);
        assert!(matches!(result, Err(AnnounceCacheError::InvalidConfig(_))));
    }

    #[esp32_test]
    fn test_insert_new() {
        let mut cache = AnnounceCache::new(AnnounceCacheConfig::default()).unwrap();
        let hash = make_hash(1);

        let result = cache.insert(hash, 3);
        assert_eq!(result, InsertResult::New);
        assert_eq!(cache.len(), 1);
        assert!(cache.contains(&hash));
    }

    #[esp32_test]
    fn test_insert_duplicate() {
        let mut cache = AnnounceCache::new(AnnounceCacheConfig::default()).unwrap();
        let hash = make_hash(1);

        let _ = cache.insert(hash, 3);
        let result = cache.insert(hash, 3);
        assert_eq!(result, InsertResult::Duplicate);

        // Worse hop count is also duplicate
        let result = cache.insert(hash, 5);
        assert_eq!(result, InsertResult::Duplicate);
    }

    #[esp32_test]
    fn test_insert_better_path() {
        let mut cache = AnnounceCache::new(AnnounceCacheConfig::default()).unwrap();
        let hash = make_hash(1);

        let _ = cache.insert(hash, 5);
        let result = cache.insert(hash, 3);
        assert_eq!(
            result,
            InsertResult::BetterPath {
                old_hops: 5,
                new_hops: 3
            }
        );

        // Hop count should be updated
        let entry = cache.peek(&hash).unwrap();
        assert_eq!(entry.hops, 3);
    }

    #[esp32_test]
    fn test_seen_count_increments() {
        let mut cache = AnnounceCache::new(AnnounceCacheConfig::default()).unwrap();
        let hash = make_hash(1);

        let _ = cache.insert(hash, 3);
        assert_eq!(cache.peek(&hash).unwrap().seen_count, 1);

        let _ = cache.insert(hash, 3);
        assert_eq!(cache.peek(&hash).unwrap().seen_count, 2);

        let _ = cache.insert(hash, 2);
        assert_eq!(cache.peek(&hash).unwrap().seen_count, 3);
    }

    #[esp32_test]
    fn test_get_updates_access_time() {
        let mut cache = AnnounceCache::new(AnnounceCacheConfig::default()).unwrap();
        let hash = make_hash(1);

        let _ = cache.insert(hash, 3);
        let first_access = cache.peek(&hash).unwrap().last_accessed;

        // Small delay to ensure time difference
        std::thread::sleep(Duration::from_millis(10));

        cache.get(&hash);
        let second_access = cache.peek(&hash).unwrap().last_accessed;

        assert!(second_access > first_access);
    }

    #[esp32_test]
    fn test_peek_does_not_update_access_time() {
        let mut cache = AnnounceCache::new(AnnounceCacheConfig::default()).unwrap();
        let hash = make_hash(1);

        let _ = cache.insert(hash, 3);
        let first_access = cache.peek(&hash).unwrap().last_accessed;

        // Small delay
        std::thread::sleep(Duration::from_millis(10));

        cache.peek(&hash);
        let second_access = cache.peek(&hash).unwrap().last_accessed;

        assert_eq!(first_access, second_access);
    }

    #[esp32_test]
    fn test_remove() {
        let mut cache = AnnounceCache::new(AnnounceCacheConfig::default()).unwrap();
        let hash = make_hash(1);

        let _ = cache.insert(hash, 3);
        assert!(cache.contains(&hash));

        let removed = cache.remove(&hash);
        assert!(removed.is_some());
        assert!(!cache.contains(&hash));
        assert!(cache.is_empty());
    }

    #[esp32_test]
    fn test_clear() {
        let mut cache = AnnounceCache::new(AnnounceCacheConfig::default()).unwrap();

        for i in 0..5 {
            let _ = cache.insert(make_hash(i), 3);
        }
        assert_eq!(cache.len(), 5);

        cache.clear();
        assert!(cache.is_empty());
    }

    #[esp32_test]
    fn test_lru_eviction() {
        let config = AnnounceCacheConfig {
            max_entries: 3,
            ttl: Duration::from_secs(3600),
        };
        let mut cache = AnnounceCache::new(config).unwrap();

        // Insert 3 entries
        let _ = cache.insert(make_hash(1), 3);
        std::thread::sleep(Duration::from_millis(5));
        let _ = cache.insert(make_hash(2), 3);
        std::thread::sleep(Duration::from_millis(5));
        let _ = cache.insert(make_hash(3), 3);

        // Access first entry to make it recently used
        cache.get(&make_hash(1));

        // Insert fourth entry - should evict hash 2 (LRU)
        let _ = cache.insert(make_hash(4), 3);

        assert_eq!(cache.len(), 3);
        assert!(cache.contains(&make_hash(1))); // Recently accessed
        assert!(!cache.contains(&make_hash(2))); // Evicted (LRU)
        assert!(cache.contains(&make_hash(3)));
        assert!(cache.contains(&make_hash(4)));
    }

    #[esp32_test]
    fn test_multiple_entries() {
        let mut cache = AnnounceCache::new(AnnounceCacheConfig::default()).unwrap();

        for i in 0..10 {
            let result = cache.insert(make_hash(i), i);
            assert_eq!(result, InsertResult::New);
        }

        assert_eq!(cache.len(), 10);

        for i in 0..10 {
            assert!(cache.contains(&make_hash(i)));
            assert_eq!(cache.peek(&make_hash(i)).unwrap().hops, i);
        }
    }

    #[esp32_test]
    fn test_config_accessors() {
        let config = AnnounceCacheConfig {
            max_entries: 500,
            ttl: Duration::from_secs(7200),
        };
        let cache = AnnounceCache::new(config).unwrap();

        assert_eq!(cache.config().max_entries, 500);
        assert_eq!(cache.config().ttl, Duration::from_secs(7200));
    }

    #[esp32_test]
    fn test_default_config() {
        let config = AnnounceCacheConfig::default();
        assert_eq!(config.max_entries, 256);
        assert_eq!(config.ttl, Duration::from_secs(3600));
    }

    #[esp32_test]
    fn test_error_display() {
        let err = AnnounceCacheError::InvalidConfig("test message");
        assert_eq!(format!("{}", err), "invalid config: test message");
    }
}
