//! LoRa time-on-air calculation.
//!
//! Calculates the exact transmission duration for a LoRa packet based on
//! payload size and modulation parameters. Uses the formula from Semtech
//! SX1262 datasheet (Section 6.1.4).
//!
//! # Example
//!
//! ```
//! use reticulum_rs_esp32::lora::{calculate_airtime_us, LoRaParams};
//!
//! let params = LoRaParams::default();
//! let airtime = calculate_airtime_us(50, &params);
//! println!("50-byte packet takes {} us ({:.2} ms)", airtime, airtime as f64 / 1000.0);
//! ```

/// LoRa modulation parameters for airtime calculation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LoRaParams {
    /// Spreading factor (7-12)
    pub spreading_factor: u8,
    /// Bandwidth in Hz (typically 125000, 250000, or 500000)
    pub bandwidth_hz: u32,
    /// Coding rate denominator (5-8 for 4/5 to 4/8)
    pub coding_rate: u8,
    /// Preamble length in symbols (typically 8)
    pub preamble_symbols: u8,
    /// Whether explicit header mode is used
    pub explicit_header: bool,
    /// Whether CRC is enabled
    pub crc_enabled: bool,
}

impl Default for LoRaParams {
    /// Default parameters matching Reticulum/RNode defaults.
    fn default() -> Self {
        Self {
            spreading_factor: 7,
            bandwidth_hz: 125_000,
            coding_rate: 5, // 4/5
            preamble_symbols: 8,
            explicit_header: true,
            crc_enabled: true,
        }
    }
}

impl LoRaParams {
    /// Check if low data rate optimization should be enabled.
    ///
    /// Required when symbol time exceeds 16ms (SF11/SF12 at 125kHz).
    pub fn low_data_rate_optimize(&self) -> bool {
        let symbol_time_us = self.symbol_duration_us();
        symbol_time_us > 16_000 // 16ms in microseconds
    }

    /// Calculate symbol duration in microseconds.
    pub fn symbol_duration_us(&self) -> u64 {
        // T_sym = 2^SF / BW (in seconds)
        // Convert to microseconds: 2^SF * 1_000_000 / BW
        let sf = self.spreading_factor as u64;
        let bw = self.bandwidth_hz as u64;
        if bw == 0 {
            return 0;
        }
        (1u64 << sf) * 1_000_000 / bw
    }
}

/// Calculate LoRa packet airtime in microseconds.
///
/// Uses the formula from Semtech SX1262 datasheet (Section 6.1.4).
///
/// # Arguments
///
/// * `payload_bytes` - Payload size in bytes
/// * `params` - LoRa modulation parameters
///
/// # Returns
///
/// Transmission time in microseconds.
pub fn calculate_airtime_us(payload_bytes: usize, params: &LoRaParams) -> u64 {
    let sf = params.spreading_factor as f64;
    let bw = params.bandwidth_hz as f64;

    if bw == 0.0 {
        return 0;
    }

    // Symbol duration in microseconds
    let t_sym_us = (2.0_f64.powf(sf) / bw) * 1_000_000.0;

    // Preamble duration: (preamble_symbols + 4.25) * symbol_duration
    let preamble = params.preamble_symbols as f64;
    let t_preamble_us = (preamble + 4.25) * t_sym_us;

    // Calculate payload symbols using Semtech formula
    let de = if params.low_data_rate_optimize() {
        1.0
    } else {
        0.0
    };
    let h = if params.explicit_header { 0.0 } else { 1.0 };
    let crc_bits = if params.crc_enabled { 16.0 } else { 0.0 };

    // Numerator: 8*PL - 4*SF + 28 + 16*CRC - 20*H
    // Where PL = payload length in bytes
    let pl = payload_bytes as f64;
    let numerator = 8.0 * pl - 4.0 * sf + 28.0 + crc_bits - 20.0 * h;

    // Denominator: 4 * (SF - 2*DE)
    let denominator = 4.0 * (sf - 2.0 * de);

    // Payload symbols = 8 + max(ceil(numerator/denominator) * CR, 0)
    // Where CR is the coding rate (5, 6, 7, or 8 for 4/5, 4/6, 4/7, 4/8)
    let cr = params.coding_rate as f64;
    let payload_symbols = if denominator > 0.0 {
        8.0 + (numerator / denominator).ceil().max(0.0) * cr
    } else {
        8.0
    };

    let t_payload_us = payload_symbols * t_sym_us;

    (t_preamble_us + t_payload_us) as u64
}

/// Calculate airtime in milliseconds (convenience wrapper).
pub fn calculate_airtime_ms(payload_bytes: usize, params: &LoRaParams) -> f64 {
    calculate_airtime_us(payload_bytes, params) as f64 / 1000.0
}

#[cfg(feature = "tap-tests")]
mod tap_tests {
    use super::*;
    use reticulum_rs_esp32_macros::tap_test;

    #[tap_test]
    fn test_default_params() {
        let params = LoRaParams::default();
        assert_eq!(params.spreading_factor, 7);
        assert_eq!(params.bandwidth_hz, 125_000);
        assert_eq!(params.coding_rate, 5);
        assert_eq!(params.preamble_symbols, 8);
        assert!(params.explicit_header);
        assert!(params.crc_enabled);
    }

    #[tap_test]
    fn test_symbol_duration_sf7_125khz() {
        let params = LoRaParams {
            spreading_factor: 7,
            bandwidth_hz: 125_000,
            ..Default::default()
        };
        // 2^7 / 125000 = 128/125000 = 0.001024 seconds = 1024 us
        assert_eq!(params.symbol_duration_us(), 1024);
    }

    #[tap_test]
    fn test_symbol_duration_sf12_125khz() {
        let params = LoRaParams {
            spreading_factor: 12,
            bandwidth_hz: 125_000,
            ..Default::default()
        };
        // 2^12 / 125000 = 4096/125000 = 0.032768 seconds = 32768 us
        assert_eq!(params.symbol_duration_us(), 32768);
    }

    #[tap_test]
    fn test_symbol_duration_sf7_500khz() {
        let params = LoRaParams {
            spreading_factor: 7,
            bandwidth_hz: 500_000,
            ..Default::default()
        };
        // 2^7 / 500000 = 128/500000 = 0.000256 seconds = 256 us
        assert_eq!(params.symbol_duration_us(), 256);
    }

    #[tap_test]
    fn test_low_data_rate_optimize() {
        // SF7 at 125kHz: symbol time = 1.024ms, no LDRO needed
        let params = LoRaParams {
            spreading_factor: 7,
            bandwidth_hz: 125_000,
            ..Default::default()
        };
        assert!(!params.low_data_rate_optimize());

        // SF11 at 125kHz: symbol time = 16.384ms, LDRO needed
        let params = LoRaParams {
            spreading_factor: 11,
            bandwidth_hz: 125_000,
            ..Default::default()
        };
        assert!(params.low_data_rate_optimize());

        // SF12 at 125kHz: symbol time = 32.768ms, LDRO needed
        let params = LoRaParams {
            spreading_factor: 12,
            bandwidth_hz: 125_000,
            ..Default::default()
        };
        assert!(params.low_data_rate_optimize());

        // SF12 at 500kHz: symbol time = 8.192ms, no LDRO needed
        let params = LoRaParams {
            spreading_factor: 12,
            bandwidth_hz: 500_000,
            ..Default::default()
        };
        assert!(!params.low_data_rate_optimize());
    }

    #[tap_test]
    fn test_airtime_increases_with_payload() {
        let params = LoRaParams::default();

        let airtime_10 = calculate_airtime_us(10, &params);
        let airtime_50 = calculate_airtime_us(50, &params);
        let airtime_100 = calculate_airtime_us(100, &params);

        assert!(
            airtime_50 > airtime_10,
            "Larger payload should take longer: {} > {}",
            airtime_50,
            airtime_10
        );
        assert!(
            airtime_100 > airtime_50,
            "Larger payload should take longer: {} > {}",
            airtime_100,
            airtime_50
        );
    }

    #[tap_test]
    fn test_airtime_increases_with_sf() {
        let payload = 50;

        let params_sf7 = LoRaParams {
            spreading_factor: 7,
            ..Default::default()
        };
        let airtime_sf7 = calculate_airtime_us(payload, &params_sf7);

        let params_sf10 = LoRaParams {
            spreading_factor: 10,
            ..Default::default()
        };
        let airtime_sf10 = calculate_airtime_us(payload, &params_sf10);

        let params_sf12 = LoRaParams {
            spreading_factor: 12,
            ..Default::default()
        };
        let airtime_sf12 = calculate_airtime_us(payload, &params_sf12);

        assert!(
            airtime_sf10 > airtime_sf7,
            "Higher SF should take longer: SF10 {} > SF7 {}",
            airtime_sf10,
            airtime_sf7
        );
        assert!(
            airtime_sf12 > airtime_sf10,
            "Higher SF should take longer: SF12 {} > SF10 {}",
            airtime_sf12,
            airtime_sf10
        );
    }

    #[tap_test]
    fn test_airtime_decreases_with_bandwidth() {
        let payload = 50;

        let params_125k = LoRaParams {
            bandwidth_hz: 125_000,
            ..Default::default()
        };
        let params_250k = LoRaParams {
            bandwidth_hz: 250_000,
            ..Default::default()
        };
        let params_500k = LoRaParams {
            bandwidth_hz: 500_000,
            ..Default::default()
        };

        let airtime_125k = calculate_airtime_us(payload, &params_125k);
        let airtime_250k = calculate_airtime_us(payload, &params_250k);
        let airtime_500k = calculate_airtime_us(payload, &params_500k);

        assert!(
            airtime_125k > airtime_250k,
            "Higher bandwidth should be faster"
        );
        assert!(
            airtime_250k > airtime_500k,
            "Higher bandwidth should be faster"
        );
    }

    #[tap_test]
    fn test_airtime_reasonable_range_sf7() {
        // At SF7/125kHz, a 100-byte packet should be in the 100-200ms range
        let params = LoRaParams::default();
        let airtime_ms = calculate_airtime_ms(100, &params);

        assert!(
            airtime_ms > 50.0 && airtime_ms < 300.0,
            "Expected 50-300ms for 100-byte packet at SF7, got {:.2}ms",
            airtime_ms
        );
    }

    #[tap_test]
    fn test_airtime_empty_packet() {
        let params = LoRaParams::default();
        let airtime_us = calculate_airtime_us(0, &params);

        // Even empty packet has preamble overhead
        assert!(airtime_us > 0, "Empty packet should still have airtime");

        // At SF7/125kHz, preamble alone should be ~12.5 symbols * 1024us = ~12.8ms
        let airtime_ms = airtime_us as f64 / 1000.0;
        assert!(
            airtime_ms > 10.0 && airtime_ms < 100.0,
            "Empty packet should be 10-100ms at SF7, got {:.2}ms",
            airtime_ms
        );
    }

    #[tap_test]
    fn test_airtime_max_reticulum_mdu() {
        // Maximum Reticulum MDU: 500 bytes
        let params = LoRaParams::default();
        let airtime_ms = calculate_airtime_ms(500, &params);

        // At SF7/125kHz, 500 bytes should be under 1 second
        assert!(
            airtime_ms < 1000.0,
            "Expected < 1000ms for 500-byte packet at SF7, got {:.2}ms",
            airtime_ms
        );
    }

    #[tap_test]
    fn test_airtime_sf12_long_range() {
        // At SF12/125kHz, even small packets take a long time
        let params = LoRaParams {
            spreading_factor: 12,
            bandwidth_hz: 125_000,
            ..Default::default()
        };

        let airtime_ms = calculate_airtime_ms(50, &params);

        // SF12 is ~32x slower than SF7 per symbol
        // Should be multiple seconds for 50 bytes
        assert!(
            airtime_ms > 1000.0,
            "Expected > 1000ms for 50-byte packet at SF12, got {:.2}ms",
            airtime_ms
        );
    }

    #[tap_test]
    fn test_zero_bandwidth_is_safe() {
        let params = LoRaParams {
            bandwidth_hz: 0,
            ..Default::default()
        };
        assert_eq!(params.symbol_duration_us(), 0);
        assert_eq!(calculate_airtime_us(50, &params), 0);
    }

    #[tap_test]
    fn test_ms_conversion() {
        let params = LoRaParams::default();
        let airtime_us = calculate_airtime_us(50, &params);
        let airtime_ms = calculate_airtime_ms(50, &params);

        assert!(
            (airtime_ms - airtime_us as f64 / 1000.0).abs() < 0.001,
            "ms conversion should match"
        );
    }
}
