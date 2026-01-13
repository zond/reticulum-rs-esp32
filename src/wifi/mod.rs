//! WiFi driver and storage.
//!
//! This module provides ESP-IDF WiFi driver wrapper and NVS storage
//! for WiFi credentials.
//!
//! # Components
//!
//! - [`connection`] - ESP-IDF WiFi driver wrapper (ESP32 only)
//! - [`storage`] - NVS persistence for credentials (ESP32 only)
//!
//! # Configuration Types
//!
//! WiFi configuration types (SSID, password validation, etc.) have moved to
//! the [`crate::config`] module.

#[cfg(feature = "esp32")]
mod connection;
#[cfg(feature = "esp32")]
mod storage;

#[cfg(feature = "esp32")]
pub use connection::{WifiError, WifiManager};
#[cfg(feature = "esp32")]
pub use storage::{clear_wifi_config, load_wifi_config, save_wifi_config};
