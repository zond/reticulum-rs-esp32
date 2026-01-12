//! Reticulum-rs ESP32 firmware library.
//!
//! This library contains platform-independent components that can be tested
//! on the host machine without ESP32 hardware.

pub mod ble;
pub mod lora;
#[cfg(feature = "esp32")]
pub mod persistence;
pub mod wifi;

// Re-export commonly used items
pub use ble::{Fragment, FragmentError, Fragmenter, Reassembler};
pub use lora::{calculate_airtime_ms, calculate_airtime_us, DutyCycleLimiter, LoRaParams};
pub use wifi::{ConfigCommand, ConfigError, WifiConfig, WifiStatus};

/// Initialize ESP-IDF for tests. Uses Once to ensure it only runs once.
/// This is a no-op on non-ESP32 targets.
#[cfg(feature = "esp32")]
pub fn ensure_esp_initialized() {
    use std::sync::Once;
    static INIT: Once = Once::new();

    INIT.call_once(|| {
        esp_idf_svc::sys::link_patches();
        esp_idf_svc::log::EspLogger::initialize_default();
    });
}

#[cfg(not(feature = "esp32"))]
pub fn ensure_esp_initialized() {
    // No-op on host
}
