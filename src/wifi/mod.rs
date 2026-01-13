//! WiFi configuration and connection management.
//!
//! This module provides BLE-based WiFi configuration for the headless ESP32 device.
//!
//! # Components
//!
//! - [`config`] - Platform-independent configuration types (host-testable)
//! - [`storage`] - NVS persistence for credentials (ESP32 only)
//! - [`ble_service`] - BLE GATT service for configuration (ESP32 only)
//! - [`connection`] - WiFi driver wrapper (ESP32 only)

mod config;

#[cfg(feature = "esp32")]
mod ble_service;
#[cfg(feature = "esp32")]
mod connection;
#[cfg(feature = "esp32")]
mod storage;

// Re-export platform-independent types
pub use config::{
    ConfigCommand, ConfigError, WifiConfig, WifiStatus, MAX_PASSWORD_LEN, MAX_SSID_LEN,
    MIN_PASSWORD_LEN,
};

// Re-export ESP32-specific types
#[cfg(feature = "esp32")]
pub use ble_service::WifiConfigService;
#[cfg(feature = "esp32")]
pub use connection::WifiManager;
#[cfg(feature = "esp32")]
pub use storage::{clear_wifi_config, load_wifi_config, save_wifi_config};
