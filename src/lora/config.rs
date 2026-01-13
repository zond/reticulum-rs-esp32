//! LoRa region configuration.
//!
//! This module provides region-specific settings for LoRa radio operation.
//! All other LoRa parameters (spreading factor, bandwidth, etc.) use standard
//! Reticulum defaults.

use super::DutyCycleLimiter;
use std::time::Duration;

/// Frequency band region.
///
/// Determines the operating frequency and duty cycle limits for regulatory compliance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Region {
    /// EU 863-870 MHz band (1% duty cycle)
    Eu868,
    /// US 902-928 MHz band (more relaxed duty cycle)
    Us915,
    /// Australia 915-928 MHz
    Au915,
    /// Asia 920-923 MHz
    As923,
}

impl Region {
    /// Get the operating frequency for this region in Hz.
    pub fn frequency(self) -> u32 {
        match self {
            Self::Eu868 => 868_100_000,
            Self::Us915 => 915_000_000,
            Self::Au915 => 915_000_000,
            Self::As923 => 923_200_000,
        }
    }

    /// Get the duty cycle limit for this region (percentage).
    pub fn duty_cycle_percent(self) -> f32 {
        match self {
            Self::Eu868 => 1.0,
            Self::Us915 => 10.0,
            Self::Au915 => 10.0,
            Self::As923 => 1.0,
        }
    }

    /// Create a duty cycle limiter for this region.
    ///
    /// Uses a 1-hour window for duty cycle calculation.
    pub fn duty_cycle_limiter(self) -> DutyCycleLimiter {
        DutyCycleLimiter::new(self.duty_cycle_percent(), Duration::from_secs(3600))
    }
}

impl Default for Region {
    fn default() -> Self {
        #[cfg(feature = "region-us915")]
        return Self::Us915;
        #[cfg(feature = "region-au915")]
        return Self::Au915;
        #[cfg(feature = "region-as923")]
        return Self::As923;
        #[cfg(not(any(
            feature = "region-us915",
            feature = "region-au915",
            feature = "region-as923"
        )))]
        Self::Eu868
    }
}

// ==================== Standard LoRa Parameters ====================
// These are the Reticulum defaults - no need to make them configurable.

/// Spreading factor (SF7 - balanced range/speed).
pub const SPREADING_FACTOR: u8 = 7;

/// Bandwidth in Hz (125 kHz - standard LoRa).
pub const BANDWIDTH_HZ: u32 = 125_000;

/// Coding rate denominator (5 = 4/5 coding rate).
pub const CODING_RATE: u8 = 5;

/// TX power in dBm.
pub const TX_POWER: i8 = 14;

/// Preamble length in symbols.
pub const PREAMBLE_LENGTH: u16 = 8;

/// Sync word (0x12 for private Reticulum network).
pub const SYNC_WORD: u8 = 0x12;

/// Reticulum MTU for LoRa interface.
pub const LORA_MTU: usize = 500;

/// Whether low data rate optimization is needed.
/// For SF7 @ 125kHz, this is false.
pub const LOW_DATA_RATE_OPTIMIZE: bool = false;

#[cfg(test)]
mod tests {
    use super::*;
    use reticulum_rs_esp32_macros::esp32_test;

    #[esp32_test]
    fn test_region_frequencies() {
        assert_eq!(Region::Eu868.frequency(), 868_100_000);
        assert_eq!(Region::Us915.frequency(), 915_000_000);
        assert_eq!(Region::Au915.frequency(), 915_000_000);
        assert_eq!(Region::As923.frequency(), 923_200_000);
    }

    #[esp32_test]
    fn test_region_duty_cycle() {
        assert_eq!(Region::Eu868.duty_cycle_percent(), 1.0);
        assert_eq!(Region::Us915.duty_cycle_percent(), 10.0);
    }

    #[esp32_test]
    fn test_region_duty_cycle_limiter() {
        let limiter = Region::Eu868.duty_cycle_limiter();
        // 1% of 1 hour = 36 seconds = 36_000_000 microseconds
        assert_eq!(limiter.budget(), 36_000_000);

        let limiter = Region::Us915.duty_cycle_limiter();
        // 10% of 1 hour = 360 seconds
        assert_eq!(limiter.budget(), 360_000_000);
    }

    #[esp32_test]
    fn test_default_region() {
        assert_eq!(Region::default(), Region::Eu868);
    }
}
