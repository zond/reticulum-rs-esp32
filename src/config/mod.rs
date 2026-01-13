//! Node configuration management.
//!
//! This module provides configuration types and BLE-based configuration service
//! for the ESP32 transport node.
//!
//! # Components
//!
//! - [`wifi`] - WiFi credential configuration (host-testable)
//! - [`ble_service`] - BLE GATT service for configuration (ESP32 only)
//!
//! # Future Extensions
//!
//! The BLE service will be extended to support:
//! - Testnet server selection
//! - Announce filtering settings (gateway mode)
//! - LoRa region configuration
//! - DHT participation settings
//!
//! See [docs/future-work.md](../../docs/future-work.md) for details.

mod wifi;

#[cfg(feature = "esp32")]
mod ble_service;

// Re-export WiFi configuration types (platform-independent)
pub use wifi::{
    ConfigCommand, ConfigError, WifiConfig, WifiStatus, MAX_PASSWORD_LEN, MAX_SSID_LEN,
    MIN_PASSWORD_LEN,
};

// Re-export BLE service (ESP32 only)
#[cfg(feature = "esp32")]
pub use ble_service::WifiConfigService;
