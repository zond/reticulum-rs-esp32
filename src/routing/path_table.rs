//! Path table for routing decisions.
//!
//! Reticulum transport nodes maintain a path table that tracks known routes
//! to destinations. Each path entry contains:
//! - The interface type (LoRa, BLE, WiFi) and optional next-hop identifier
//! - Routing metrics (hop count, timestamp, signal quality)
//! - Path validation status
//!
//! The path table supports:
//! - Multiple paths to the same destination (different interfaces)
//! - Automatic path selection based on scoring
//! - TTL-based path expiration
//! - Path updates when better routes are discovered

use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Hash identifying a destination (typically 16 bytes in Reticulum).
pub type DestinationHash = [u8; 16];

/// Hash identifying a next hop node.
pub type NextHopHash = [u8; 16];

/// Type of interface for a path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InterfaceType {
    /// LoRa radio interface.
    LoRa,
    /// Bluetooth Low Energy interface.
    Ble,
    /// WiFi/TCP interface.
    Wifi,
}

impl std::fmt::Display for InterfaceType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::LoRa => write!(f, "LoRa"),
            Self::Ble => write!(f, "BLE"),
            Self::Wifi => write!(f, "WiFi"),
        }
    }
}

/// Routing metrics for path scoring.
#[derive(Debug, Clone, Copy, Default)]
pub struct RoutingMetrics {
    /// Number of hops to destination.
    pub hops: u8,
    /// Signal strength when path was learned (if available).
    /// Higher (less negative) is better. None if not measured.
    pub rssi_dbm: Option<i16>,
    /// Whether this path has been validated by a response.
    pub validated: bool,
}

impl RoutingMetrics {
    /// Calculate a score for this path (higher is better).
    ///
    /// Scoring factors:
    /// - Lower hop count is better (primary factor)
    /// - Higher RSSI is better (secondary factor)
    /// - Validated paths get a bonus
    ///
    /// Returns a score where higher values indicate better paths.
    pub fn score(&self) -> i32 {
        // Start with inverse hop count (fewer hops = higher score)
        // Scale by 1000 to leave room for other factors
        let hop_score = (255 - self.hops as i32) * 1000;

        // Add RSSI contribution (normalized to 0-100 range)
        // RSSI typically ranges from -120 to 0 dBm
        let rssi_score = self.rssi_dbm.map(|rssi| (rssi + 120) as i32).unwrap_or(0);

        // Validated paths get a bonus
        let validation_bonus = if self.validated { 500 } else { 0 };

        hop_score + rssi_score + validation_bonus
    }
}

/// Configuration for the path table.
///
/// Note: This is `Copy` for efficient passing to constructors.
#[derive(Debug, Clone, Copy)]
pub struct PathTableConfig {
    /// Maximum number of destinations to track.
    pub max_destinations: usize,
    /// Maximum paths per destination.
    pub max_paths_per_dest: usize,
    /// Time-to-live for path entries.
    pub path_ttl: Duration,
}

impl Default for PathTableConfig {
    fn default() -> Self {
        Self {
            max_destinations: 128,
            max_paths_per_dest: 4,
            path_ttl: Duration::from_secs(1800), // 30 minutes
        }
    }
}

impl PathTableConfig {
    /// Validate configuration parameters.
    pub fn validate(&self) -> Result<(), PathTableError> {
        if self.max_destinations == 0 {
            return Err(PathTableError::InvalidConfig(
                "max_destinations must be greater than 0",
            ));
        }
        if self.max_paths_per_dest == 0 {
            return Err(PathTableError::InvalidConfig(
                "max_paths_per_dest must be greater than 0",
            ));
        }
        if self.path_ttl.is_zero() {
            return Err(PathTableError::InvalidConfig(
                "path_ttl must be greater than 0",
            ));
        }
        Ok(())
    }
}

/// Error type for path table operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathTableError {
    /// Invalid configuration parameter.
    InvalidConfig(&'static str),
}

impl std::fmt::Display for PathTableError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidConfig(msg) => write!(f, "invalid config: {}", msg),
        }
    }
}

impl std::error::Error for PathTableError {}

/// A single path entry in the routing table.
#[derive(Debug, Clone)]
pub struct PathEntry {
    /// Interface type for this path.
    pub interface: InterfaceType,
    /// Next hop node hash (None if direct/local).
    pub next_hop: Option<NextHopHash>,
    /// Routing metrics for scoring.
    pub metrics: RoutingMetrics,
    /// When this path was first learned.
    pub learned_at: Instant,
    /// When this path was last refreshed.
    pub last_refreshed: Instant,
}

impl PathEntry {
    /// Create a new path entry.
    pub fn new(
        interface: InterfaceType,
        next_hop: Option<NextHopHash>,
        metrics: RoutingMetrics,
    ) -> Self {
        let now = Instant::now();
        Self {
            interface,
            next_hop,
            metrics,
            learned_at: now,
            last_refreshed: now,
        }
    }

    /// Check if this path has expired.
    pub fn is_expired(&self, ttl: Duration) -> bool {
        self.last_refreshed.elapsed() > ttl
    }

    /// Refresh this path's timestamp.
    pub fn refresh(&mut self) {
        self.last_refreshed = Instant::now();
    }
}

/// Routing table for tracking paths to destinations.
///
/// The path table maintains known routes to Reticulum destinations. It supports
/// multiple paths per destination (via different interfaces) and automatically
/// selects the best path based on routing metrics.
///
/// # Example
///
/// ```
/// use reticulum_rs_esp32::routing::{
///     PathTable, PathTableConfig, PathEntry, InterfaceType, RoutingMetrics
/// };
///
/// let config = PathTableConfig::default();
/// let mut table = PathTable::new(config).unwrap();
///
/// let dest = [0u8; 16];
/// let next_hop = [1u8; 16];
/// let metrics = RoutingMetrics { hops: 2, rssi_dbm: Some(-80), validated: true };
///
/// // Add a path via LoRa
/// table.add_path(dest, InterfaceType::LoRa, Some(next_hop), metrics);
///
/// // Get the best path to this destination
/// let best = table.best_path(&dest);
/// assert!(best.is_some());
/// ```
pub struct PathTable {
    config: PathTableConfig,
    /// Map from destination hash to list of paths.
    paths: HashMap<DestinationHash, Vec<PathEntry>>,
}

impl Default for PathTable {
    fn default() -> Self {
        Self::new(PathTableConfig::default()).expect("default config should be valid")
    }
}

impl PathTable {
    /// Create a new path table with the given configuration.
    pub fn new(config: PathTableConfig) -> Result<Self, PathTableError> {
        config.validate()?;
        Ok(Self {
            config,
            paths: HashMap::with_capacity(config.max_destinations),
        })
    }

    /// Add or update a path to a destination.
    ///
    /// If a path via the same interface already exists, it will be updated
    /// if the new metrics are better. Otherwise, a new path is added.
    ///
    /// Returns true if the path was added or updated, false if rejected
    /// (e.g., worse metrics than existing path via same interface).
    pub fn add_path(
        &mut self,
        destination: DestinationHash,
        interface: InterfaceType,
        next_hop: Option<NextHopHash>,
        metrics: RoutingMetrics,
    ) -> bool {
        let now = Instant::now();

        // Get or create the path list for this destination
        let path_list = self.paths.entry(destination).or_default();

        // Look for existing path via same interface
        for path in path_list.iter_mut() {
            if path.interface == interface {
                // Update if better metrics or to refresh timestamp
                if metrics.score() >= path.metrics.score() {
                    path.next_hop = next_hop;
                    path.metrics = metrics;
                    path.last_refreshed = now;
                    return true;
                } else {
                    // Existing path is better, just refresh it
                    path.last_refreshed = now;
                    return false;
                }
            }
        }

        // New interface for this destination
        if path_list.len() < self.config.max_paths_per_dest {
            path_list.push(PathEntry::new(interface, next_hop, metrics));
            return true;
        }

        // At capacity - replace worst path if new one is better
        if let Some(worst_idx) = path_list
            .iter()
            .enumerate()
            .min_by_key(|(_, p)| p.metrics.score())
            .map(|(i, _)| i)
        {
            if metrics.score() > path_list[worst_idx].metrics.score() {
                path_list[worst_idx] = PathEntry::new(interface, next_hop, metrics);
                return true;
            }
        }

        false
    }

    /// Get the best path to a destination.
    ///
    /// Returns the path with the highest score, or None if no paths exist
    /// or all paths have expired.
    pub fn best_path(&self, destination: &DestinationHash) -> Option<&PathEntry> {
        let ttl = self.config.path_ttl;
        self.paths
            .get(destination)?
            .iter()
            .filter(|p| !p.is_expired(ttl))
            .max_by_key(|p| p.metrics.score())
    }

    /// Get all paths to a destination, sorted by score (best first).
    pub fn paths_to(&self, destination: &DestinationHash) -> Vec<&PathEntry> {
        let ttl = self.config.path_ttl;
        let mut paths: Vec<_> = self
            .paths
            .get(destination)
            .map(|list| list.iter().filter(|p| !p.is_expired(ttl)).collect())
            .unwrap_or_default();
        paths.sort_by_key(|p| std::cmp::Reverse(p.metrics.score()));
        paths
    }

    /// Check if we have any path to a destination.
    pub fn has_path(&self, destination: &DestinationHash) -> bool {
        self.best_path(destination).is_some()
    }

    /// Remove all paths to a destination.
    pub fn remove_destination(&mut self, destination: &DestinationHash) -> bool {
        self.paths.remove(destination).is_some()
    }

    /// Mark a path as validated.
    ///
    /// Call this when we receive a response from the destination via this path,
    /// confirming the path actually works.
    pub fn validate_path(&mut self, destination: &DestinationHash, interface: InterfaceType) {
        if let Some(path_list) = self.paths.get_mut(destination) {
            for path in path_list.iter_mut() {
                if path.interface == interface {
                    path.metrics.validated = true;
                    path.last_refreshed = Instant::now();
                    break;
                }
            }
        }
    }

    /// Get the number of destinations in the table.
    pub fn destination_count(&self) -> usize {
        self.paths.len()
    }

    /// Get the total number of paths across all destinations.
    pub fn path_count(&self) -> usize {
        self.paths.values().map(|v| v.len()).sum()
    }

    /// Check if the table is empty.
    pub fn is_empty(&self) -> bool {
        self.paths.is_empty()
    }

    /// Clear all paths from the table.
    pub fn clear(&mut self) {
        self.paths.clear();
    }

    /// Remove all expired paths.
    ///
    /// Returns the number of paths removed.
    pub fn cleanup_expired(&mut self) -> usize {
        let ttl = self.config.path_ttl;
        let mut removed = 0;

        self.paths.retain(|_, path_list| {
            let before = path_list.len();
            path_list.retain(|p| !p.is_expired(ttl));
            removed += before - path_list.len();
            !path_list.is_empty()
        });

        removed
    }

    /// Get the table configuration.
    pub fn config(&self) -> &PathTableConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use reticulum_rs_esp32_macros::esp32_test;

    fn make_dest(id: u8) -> DestinationHash {
        let mut hash = [0u8; 16];
        hash[0] = id;
        hash
    }

    fn make_next_hop(id: u8) -> NextHopHash {
        let mut hash = [0u8; 16];
        hash[0] = id;
        hash
    }

    #[esp32_test]
    fn test_new_table() {
        let config = PathTableConfig::default();
        let table = PathTable::new(config).unwrap();
        assert!(table.is_empty());
        assert_eq!(table.destination_count(), 0);
        assert_eq!(table.path_count(), 0);
    }

    #[esp32_test]
    fn test_invalid_config_zero_destinations() {
        let config = PathTableConfig {
            max_destinations: 0,
            ..Default::default()
        };
        assert!(matches!(
            PathTable::new(config),
            Err(PathTableError::InvalidConfig(_))
        ));
    }

    #[esp32_test]
    fn test_invalid_config_zero_paths() {
        let config = PathTableConfig {
            max_paths_per_dest: 0,
            ..Default::default()
        };
        assert!(matches!(
            PathTable::new(config),
            Err(PathTableError::InvalidConfig(_))
        ));
    }

    #[esp32_test]
    fn test_invalid_config_zero_ttl() {
        let config = PathTableConfig {
            path_ttl: Duration::ZERO,
            ..Default::default()
        };
        assert!(matches!(
            PathTable::new(config),
            Err(PathTableError::InvalidConfig(_))
        ));
    }

    #[esp32_test]
    fn test_add_path() {
        let mut table = PathTable::new(PathTableConfig::default()).unwrap();
        let dest = make_dest(1);
        let next_hop = make_next_hop(2);
        let metrics = RoutingMetrics {
            hops: 2,
            rssi_dbm: Some(-80),
            validated: false,
        };

        let added = table.add_path(dest, InterfaceType::LoRa, Some(next_hop), metrics);
        assert!(added);
        assert_eq!(table.destination_count(), 1);
        assert_eq!(table.path_count(), 1);
        assert!(table.has_path(&dest));
    }

    #[esp32_test]
    fn test_multiple_interfaces_same_dest() {
        let mut table = PathTable::new(PathTableConfig::default()).unwrap();
        let dest = make_dest(1);

        // Add LoRa path
        let metrics_lora = RoutingMetrics {
            hops: 3,
            rssi_dbm: Some(-90),
            validated: false,
        };
        table.add_path(dest, InterfaceType::LoRa, None, metrics_lora);

        // Add BLE path
        let metrics_ble = RoutingMetrics {
            hops: 2,
            rssi_dbm: Some(-70),
            validated: true,
        };
        table.add_path(dest, InterfaceType::Ble, None, metrics_ble);

        assert_eq!(table.destination_count(), 1);
        assert_eq!(table.path_count(), 2);

        // BLE should be best (fewer hops + better RSSI + validated)
        let best = table.best_path(&dest).unwrap();
        assert_eq!(best.interface, InterfaceType::Ble);
    }

    #[esp32_test]
    fn test_best_path_by_hops() {
        let mut table = PathTable::new(PathTableConfig::default()).unwrap();
        let dest = make_dest(1);

        // Add path with 5 hops
        table.add_path(
            dest,
            InterfaceType::LoRa,
            None,
            RoutingMetrics {
                hops: 5,
                ..Default::default()
            },
        );

        // Add path with 2 hops
        table.add_path(
            dest,
            InterfaceType::Ble,
            None,
            RoutingMetrics {
                hops: 2,
                ..Default::default()
            },
        );

        let best = table.best_path(&dest).unwrap();
        assert_eq!(best.metrics.hops, 2);
    }

    #[esp32_test]
    fn test_update_better_metrics() {
        let mut table = PathTable::new(PathTableConfig::default()).unwrap();
        let dest = make_dest(1);

        // Add initial path
        table.add_path(
            dest,
            InterfaceType::LoRa,
            None,
            RoutingMetrics {
                hops: 5,
                rssi_dbm: Some(-100),
                validated: false,
            },
        );

        // Update with better metrics
        let updated = table.add_path(
            dest,
            InterfaceType::LoRa,
            None,
            RoutingMetrics {
                hops: 3,
                rssi_dbm: Some(-80),
                validated: true,
            },
        );

        assert!(updated);
        let path = table.best_path(&dest).unwrap();
        assert_eq!(path.metrics.hops, 3);
        assert_eq!(path.metrics.rssi_dbm, Some(-80));
        assert!(path.metrics.validated);
    }

    #[esp32_test]
    fn test_reject_worse_metrics() {
        let mut table = PathTable::new(PathTableConfig::default()).unwrap();
        let dest = make_dest(1);

        // Add good path
        table.add_path(
            dest,
            InterfaceType::LoRa,
            None,
            RoutingMetrics {
                hops: 2,
                rssi_dbm: Some(-60),
                validated: true,
            },
        );

        // Try to update with worse metrics
        let updated = table.add_path(
            dest,
            InterfaceType::LoRa,
            None,
            RoutingMetrics {
                hops: 5,
                rssi_dbm: Some(-100),
                validated: false,
            },
        );

        assert!(!updated);
        // Original metrics should be preserved
        let path = table.best_path(&dest).unwrap();
        assert_eq!(path.metrics.hops, 2);
    }

    #[esp32_test]
    fn test_validate_path() {
        let mut table = PathTable::new(PathTableConfig::default()).unwrap();
        let dest = make_dest(1);

        table.add_path(dest, InterfaceType::LoRa, None, RoutingMetrics::default());

        assert!(!table.best_path(&dest).unwrap().metrics.validated);

        table.validate_path(&dest, InterfaceType::LoRa);

        assert!(table.best_path(&dest).unwrap().metrics.validated);
    }

    #[esp32_test]
    fn test_remove_destination() {
        let mut table = PathTable::new(PathTableConfig::default()).unwrap();
        let dest = make_dest(1);

        table.add_path(dest, InterfaceType::LoRa, None, RoutingMetrics::default());
        assert!(table.has_path(&dest));

        let removed = table.remove_destination(&dest);
        assert!(removed);
        assert!(!table.has_path(&dest));
    }

    #[esp32_test]
    fn test_clear() {
        let mut table = PathTable::new(PathTableConfig::default()).unwrap();

        for i in 0..5 {
            table.add_path(
                make_dest(i),
                InterfaceType::LoRa,
                None,
                RoutingMetrics::default(),
            );
        }
        assert_eq!(table.destination_count(), 5);

        table.clear();
        assert!(table.is_empty());
    }

    #[esp32_test]
    fn test_paths_to_sorted() {
        let mut table = PathTable::new(PathTableConfig::default()).unwrap();
        let dest = make_dest(1);

        // Add paths with different hop counts
        table.add_path(
            dest,
            InterfaceType::LoRa,
            None,
            RoutingMetrics {
                hops: 5,
                ..Default::default()
            },
        );
        table.add_path(
            dest,
            InterfaceType::Ble,
            None,
            RoutingMetrics {
                hops: 2,
                ..Default::default()
            },
        );
        table.add_path(
            dest,
            InterfaceType::Wifi,
            None,
            RoutingMetrics {
                hops: 3,
                ..Default::default()
            },
        );

        let paths = table.paths_to(&dest);
        assert_eq!(paths.len(), 3);
        assert_eq!(paths[0].metrics.hops, 2); // Best
        assert_eq!(paths[1].metrics.hops, 3);
        assert_eq!(paths[2].metrics.hops, 5); // Worst
    }

    #[esp32_test]
    fn test_metrics_score() {
        // Fewer hops is better
        let m1 = RoutingMetrics {
            hops: 2,
            ..Default::default()
        };
        let m2 = RoutingMetrics {
            hops: 5,
            ..Default::default()
        };
        assert!(m1.score() > m2.score());

        // Better RSSI is better (same hops)
        let m3 = RoutingMetrics {
            hops: 2,
            rssi_dbm: Some(-60),
            validated: false,
        };
        let m4 = RoutingMetrics {
            hops: 2,
            rssi_dbm: Some(-90),
            validated: false,
        };
        assert!(m3.score() > m4.score());

        // Validated is better (same hops and RSSI)
        let m5 = RoutingMetrics {
            hops: 2,
            rssi_dbm: Some(-70),
            validated: true,
        };
        let m6 = RoutingMetrics {
            hops: 2,
            rssi_dbm: Some(-70),
            validated: false,
        };
        assert!(m5.score() > m6.score());
    }

    #[esp32_test]
    fn test_interface_type_display() {
        assert_eq!(format!("{}", InterfaceType::LoRa), "LoRa");
        assert_eq!(format!("{}", InterfaceType::Ble), "BLE");
        assert_eq!(format!("{}", InterfaceType::Wifi), "WiFi");
    }

    #[esp32_test]
    fn test_default_config() {
        let config = PathTableConfig::default();
        assert_eq!(config.max_destinations, 128);
        assert_eq!(config.max_paths_per_dest, 4);
        assert_eq!(config.path_ttl, Duration::from_secs(1800));
    }

    #[esp32_test]
    fn test_error_display() {
        let err = PathTableError::InvalidConfig("test message");
        assert_eq!(format!("{}", err), "invalid config: test message");
    }
}
