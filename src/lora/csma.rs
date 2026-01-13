//! CSMA/CA (Carrier Sense Multiple Access with Collision Avoidance) for LoRa.
//!
//! This module implements listen-before-talk logic to avoid packet collisions
//! on shared LoRa frequencies. The implementation is fully host-testable.
//!
//! # Algorithm
//!
//! 1. Read channel RSSI before transmitting
//! 2. If RSSI > threshold, channel is busy - wait random backoff
//! 3. If RSSI <= threshold, channel is clear - transmit
//! 4. On retry, use exponential backoff (doubles each attempt)
//! 5. Give up after max retries exceeded
//!
//! # Example
//!
//! ```
//! use reticulum_rs_esp32::lora::{Csma, CsmaConfig, CsmaResult};
//!
//! let config = CsmaConfig::default();
//! let mut csma = Csma::new(config);
//!
//! // Simulate checking channel (in real code, read RSSI from radio)
//! let rssi = -95; // dBm, below threshold = clear
//!
//! match csma.try_access(rssi) {
//!     CsmaResult::Transmit => println!("Channel clear, transmitting"),
//!     CsmaResult::Wait { ms } => println!("Channel busy, wait {}ms", ms),
//!     CsmaResult::GiveUp => println!("Max retries exceeded"),
//! }
//! ```

/// Configuration for CSMA/CA behavior.
#[derive(Debug, Clone, Copy)]
pub struct CsmaConfig {
    /// RSSI threshold in dBm. Channel is considered busy if RSSI > threshold.
    /// Typical values: -90 to -80 dBm.
    pub rssi_threshold_dbm: i16,

    /// Maximum number of retry attempts before giving up.
    pub max_retries: u8,

    /// Minimum backoff time in milliseconds.
    pub min_backoff_ms: u32,

    /// Maximum backoff time in milliseconds (caps exponential growth).
    pub max_backoff_ms: u32,
}

impl Default for CsmaConfig {
    fn default() -> Self {
        Self {
            rssi_threshold_dbm: -90, // -90 dBm is a common threshold
            max_retries: 5,
            min_backoff_ms: 10,
            max_backoff_ms: 500,
        }
    }
}

impl CsmaConfig {
    /// Create config with custom RSSI threshold.
    pub fn with_threshold(rssi_threshold_dbm: i16) -> Self {
        Self {
            rssi_threshold_dbm,
            ..Default::default()
        }
    }

    /// Validate configuration values.
    pub fn validate(&self) -> Result<(), CsmaError> {
        if self.min_backoff_ms == 0 {
            return Err(CsmaError::InvalidConfig("min_backoff_ms must be > 0"));
        }
        if self.max_backoff_ms < self.min_backoff_ms {
            return Err(CsmaError::InvalidConfig(
                "max_backoff_ms must be >= min_backoff_ms",
            ));
        }
        if self.max_retries == 0 {
            return Err(CsmaError::InvalidConfig("max_retries must be > 0"));
        }
        if self.max_retries > 20 {
            return Err(CsmaError::InvalidConfig("max_retries must be <= 20"));
        }
        if self.rssi_threshold_dbm > -40 {
            return Err(CsmaError::InvalidConfig(
                "rssi_threshold_dbm must be <= -40 dBm",
            ));
        }
        if self.rssi_threshold_dbm < -140 {
            return Err(CsmaError::InvalidConfig(
                "rssi_threshold_dbm must be >= -140 dBm",
            ));
        }
        Ok(())
    }
}

/// Result of a CSMA channel access attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
#[must_use = "ignoring CSMA result could cause transmission on busy channel"]
pub enum CsmaResult {
    /// Channel is clear, proceed with transmission.
    Transmit,

    /// Channel is busy, wait the specified time before retrying.
    Wait {
        /// Backoff time in milliseconds.
        ms: u32,
    },

    /// Maximum retries exceeded, give up on this transmission.
    GiveUp,
}

/// Errors that can occur in CSMA operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CsmaError {
    /// Invalid configuration parameter.
    InvalidConfig(&'static str),
}

impl std::fmt::Display for CsmaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidConfig(msg) => write!(f, "invalid CSMA config: {}", msg),
        }
    }
}

impl std::error::Error for CsmaError {}

/// CSMA/CA state machine for collision avoidance.
///
/// Tracks retry count and generates random backoff times. The state is reset
/// after a successful transmission.
pub struct Csma {
    config: CsmaConfig,
    retries: u8,
    /// Simple LCG PRNG state for backoff randomization.
    /// Using a simple PRNG to avoid dependencies and keep it host-testable.
    rng_state: u32,
}

impl Default for Csma {
    fn default() -> Self {
        Self::new(CsmaConfig::default())
    }
}

impl Csma {
    /// Create a new CSMA instance with the given configuration.
    pub fn new(config: CsmaConfig) -> Self {
        Self {
            config,
            retries: 0,
            // Initialize with a non-zero seed (will be overwritten by seed())
            rng_state: 0x12345678,
        }
    }

    /// Seed the random number generator.
    ///
    /// On ESP32, use hardware RNG for the seed. For testing, use a fixed seed.
    pub fn seed(&mut self, seed: u32) {
        // Ensure non-zero state
        self.rng_state = if seed == 0 { 1 } else { seed };
    }

    /// Check if channel is clear based on RSSI reading.
    ///
    /// Returns `true` if RSSI is at or below threshold (channel clear).
    pub fn is_channel_clear(&self, rssi_dbm: i16) -> bool {
        rssi_dbm <= self.config.rssi_threshold_dbm
    }

    /// Attempt to access the channel.
    ///
    /// Call this before each transmission attempt with the current RSSI reading.
    /// Returns the action to take.
    ///
    /// # Arguments
    ///
    /// * `rssi_dbm` - Current channel RSSI in dBm (read from radio)
    ///
    /// # Returns
    ///
    /// * `CsmaResult::Transmit` - Channel is clear, proceed with transmission
    /// * `CsmaResult::Wait { ms }` - Channel busy, wait before retrying
    /// * `CsmaResult::GiveUp` - Max retries exceeded, drop the packet
    pub fn try_access(&mut self, rssi_dbm: i16) -> CsmaResult {
        if self.is_channel_clear(rssi_dbm) {
            CsmaResult::Transmit
        } else if self.retries >= self.config.max_retries {
            CsmaResult::GiveUp
        } else {
            let backoff = self.calculate_backoff();
            self.retries += 1;
            CsmaResult::Wait { ms: backoff }
        }
    }

    /// Reset state after successful transmission.
    ///
    /// Call this after each successful transmission to reset the retry counter.
    pub fn reset(&mut self) {
        self.retries = 0;
    }

    /// Get current retry count.
    pub fn retries(&self) -> u8 {
        self.retries
    }

    /// Get the configuration.
    pub fn config(&self) -> &CsmaConfig {
        &self.config
    }

    /// Calculate random backoff time with exponential growth.
    ///
    /// Backoff window doubles with each retry:
    /// - Retry 0: [min, min*2)
    /// - Retry 1: [min, min*4)
    /// - Retry 2: [min, min*8)
    /// - etc., capped at max_backoff_ms
    fn calculate_backoff(&mut self) -> u32 {
        // Exponential backoff: window = min * 2^(retries+1)
        let window = self
            .config
            .min_backoff_ms
            .saturating_mul(1 << (self.retries + 1).min(10));
        let window = window.min(self.config.max_backoff_ms);

        // Random value in [min_backoff, window]
        let range = window.saturating_sub(self.config.min_backoff_ms);
        if range == 0 {
            return self.config.min_backoff_ms;
        }

        let random = self.next_random();
        self.config.min_backoff_ms + (random % range)
    }

    /// Simple LCG random number generator.
    ///
    /// Parameters from Numerical Recipes (good enough for backoff jitter).
    fn next_random(&mut self) -> u32 {
        self.rng_state = self
            .rng_state
            .wrapping_mul(1664525)
            .wrapping_add(1013904223);
        self.rng_state
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = CsmaConfig::default();
        assert_eq!(config.rssi_threshold_dbm, -90);
        assert_eq!(config.max_retries, 5);
        assert_eq!(config.min_backoff_ms, 10);
        assert_eq!(config.max_backoff_ms, 500);
    }

    #[test]
    fn test_config_validation_valid() {
        let config = CsmaConfig::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_config_validation_zero_min_backoff() {
        let config = CsmaConfig {
            min_backoff_ms: 0,
            ..Default::default()
        };
        assert!(matches!(
            config.validate(),
            Err(CsmaError::InvalidConfig(_))
        ));
    }

    #[test]
    fn test_config_validation_max_less_than_min() {
        let config = CsmaConfig {
            min_backoff_ms: 100,
            max_backoff_ms: 50,
            ..Default::default()
        };
        assert!(matches!(
            config.validate(),
            Err(CsmaError::InvalidConfig(_))
        ));
    }

    #[test]
    fn test_config_validation_zero_retries() {
        let config = CsmaConfig {
            max_retries: 0,
            ..Default::default()
        };
        assert!(matches!(
            config.validate(),
            Err(CsmaError::InvalidConfig(_))
        ));
    }

    #[test]
    fn test_config_validation_max_retries_too_high() {
        let config = CsmaConfig {
            max_retries: 21,
            ..Default::default()
        };
        assert!(matches!(
            config.validate(),
            Err(CsmaError::InvalidConfig(_))
        ));
    }

    #[test]
    fn test_config_validation_rssi_too_high() {
        let config = CsmaConfig {
            rssi_threshold_dbm: -39,
            ..Default::default()
        };
        assert!(matches!(
            config.validate(),
            Err(CsmaError::InvalidConfig(_))
        ));
    }

    #[test]
    fn test_config_validation_rssi_too_low() {
        let config = CsmaConfig {
            rssi_threshold_dbm: -141,
            ..Default::default()
        };
        assert!(matches!(
            config.validate(),
            Err(CsmaError::InvalidConfig(_))
        ));
    }

    #[test]
    fn test_channel_clear_at_threshold() {
        let csma = Csma::default();
        // At threshold = clear
        assert!(csma.is_channel_clear(-90));
    }

    #[test]
    fn test_channel_clear_below_threshold() {
        let csma = Csma::default();
        // Below threshold (weaker signal) = clear
        assert!(csma.is_channel_clear(-100));
        assert!(csma.is_channel_clear(-120));
    }

    #[test]
    fn test_channel_busy_above_threshold() {
        let csma = Csma::default();
        // Above threshold (stronger signal) = busy
        assert!(!csma.is_channel_clear(-89));
        assert!(!csma.is_channel_clear(-50));
        assert!(!csma.is_channel_clear(0));
    }

    #[test]
    fn test_try_access_channel_clear() {
        let mut csma = Csma::default();
        let result = csma.try_access(-100); // Well below threshold
        assert_eq!(result, CsmaResult::Transmit);
        assert_eq!(csma.retries(), 0); // No retry needed
    }

    #[test]
    fn test_try_access_channel_busy() {
        let mut csma = Csma::default();
        csma.seed(12345);

        let result = csma.try_access(-50); // Above threshold = busy
        assert!(matches!(result, CsmaResult::Wait { ms: _ }));
        assert_eq!(csma.retries(), 1);
    }

    #[test]
    fn test_try_access_max_retries() {
        let config = CsmaConfig {
            max_retries: 3,
            ..Default::default()
        };
        let mut csma = Csma::new(config);
        csma.seed(12345);

        // Exhaust all retries
        for i in 0..3 {
            let result = csma.try_access(-50); // Always busy
            assert!(matches!(result, CsmaResult::Wait { .. }), "retry {}", i);
        }

        // Next attempt should give up
        let result = csma.try_access(-50);
        assert_eq!(result, CsmaResult::GiveUp);
    }

    #[test]
    fn test_reset_clears_retries() {
        let mut csma = Csma::default();
        csma.seed(12345);

        // Accumulate some retries
        let _ = csma.try_access(-50);
        let _ = csma.try_access(-50);
        assert_eq!(csma.retries(), 2);

        // Reset
        csma.reset();
        assert_eq!(csma.retries(), 0);
    }

    #[test]
    fn test_backoff_within_bounds() {
        let config = CsmaConfig {
            min_backoff_ms: 10,
            max_backoff_ms: 500,
            max_retries: 10,
            ..Default::default()
        };
        let mut csma = Csma::new(config);
        csma.seed(12345);

        // Try multiple times and verify backoff is always in bounds
        for _ in 0..10 {
            if let CsmaResult::Wait { ms } = csma.try_access(-50) {
                assert!(ms >= 10, "backoff {} < min 10", ms);
                assert!(ms < 500, "backoff {} >= max 500", ms);
            }
        }
    }

    #[test]
    fn test_backoff_exponential_growth() {
        let config = CsmaConfig {
            min_backoff_ms: 10,
            max_backoff_ms: 10000, // High max to see growth
            max_retries: 5,
            ..Default::default()
        };
        let mut csma = Csma::new(config);
        csma.seed(42); // Fixed seed for reproducibility

        // Collect backoff values (they're random but window grows)
        // We can't test exact values due to randomness, but we can verify
        // the pattern by checking multiple samples have increasing maximums
        let mut max_seen = 0u32;
        for _ in 0..5 {
            if let CsmaResult::Wait { ms } = csma.try_access(-50) {
                max_seen = max_seen.max(ms);
            }
        }
        // With exponential growth, we should see values well above minimum
        assert!(
            max_seen > 10,
            "expected exponential growth, max was {}",
            max_seen
        );
    }

    #[test]
    fn test_backoff_capped_at_max() {
        let config = CsmaConfig {
            min_backoff_ms: 100,
            max_backoff_ms: 200,
            max_retries: 20,
            ..Default::default()
        };
        let mut csma = Csma::new(config);
        csma.seed(99999);

        // Even with many retries, backoff should stay under max
        for _ in 0..20 {
            if let CsmaResult::Wait { ms } = csma.try_access(-50) {
                assert!(ms >= 100, "backoff {} below min", ms);
                assert!(ms < 200, "backoff {} at or above max", ms);
            }
        }
    }

    #[test]
    fn test_deterministic_with_same_seed() {
        let config = CsmaConfig::default();

        let mut csma1 = Csma::new(config.clone());
        let mut csma2 = Csma::new(config);

        csma1.seed(12345);
        csma2.seed(12345);

        // Same seed should produce same sequence
        for _ in 0..5 {
            let r1 = csma1.try_access(-50);
            let r2 = csma2.try_access(-50);
            assert_eq!(r1, r2);
        }
    }

    #[test]
    fn test_different_seeds_different_results() {
        let config = CsmaConfig::default();

        let mut csma1 = Csma::new(config.clone());
        let mut csma2 = Csma::new(config);

        csma1.seed(11111);
        csma2.seed(22222);

        // Different seeds should (almost certainly) produce different values
        let mut any_different = false;
        for _ in 0..5 {
            let r1 = csma1.try_access(-50);
            let r2 = csma2.try_access(-50);
            if r1 != r2 {
                any_different = true;
                break;
            }
        }
        assert!(
            any_different,
            "different seeds should produce different backoffs"
        );
    }

    #[test]
    fn test_custom_threshold() {
        let config = CsmaConfig::with_threshold(-80);
        let csma = Csma::new(config);

        assert!(csma.is_channel_clear(-80)); // At threshold
        assert!(csma.is_channel_clear(-85)); // Below
        assert!(!csma.is_channel_clear(-75)); // Above
    }

    #[test]
    fn test_workflow_success_after_retry() {
        let mut csma = Csma::default();
        csma.seed(12345);

        // First attempt: channel busy
        let result = csma.try_access(-50);
        assert!(matches!(result, CsmaResult::Wait { .. }));

        // Second attempt: still busy
        let result = csma.try_access(-60);
        assert!(matches!(result, CsmaResult::Wait { .. }));

        // Third attempt: channel clear!
        let result = csma.try_access(-100);
        assert_eq!(result, CsmaResult::Transmit);

        // After successful transmit, reset for next packet
        csma.reset();
        assert_eq!(csma.retries(), 0);
    }

    #[test]
    fn test_zero_seed_converted_to_one() {
        let mut csma1 = Csma::default();
        let mut csma2 = Csma::default();

        csma1.seed(0); // Should be converted to 1
        csma2.seed(1);

        // Both should produce same sequence since 0 -> 1
        for _ in 0..5 {
            let r1 = csma1.try_access(-50);
            let r2 = csma2.try_access(-50);
            assert_eq!(r1, r2);
        }
    }
}
