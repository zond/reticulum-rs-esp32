//! Reticulum-rs ESP32 firmware library.
//!
//! This library contains platform-independent components that can be tested
//! on the host machine without ESP32 hardware.

// Allow the crate to reference itself by name (needed for proc-macro generated code)
extern crate self as reticulum_rs_esp32;

pub mod ble;
pub mod lora;
#[cfg(feature = "esp32")]
pub mod persistence;
#[cfg(feature = "tap-tests")]
pub mod testing;
pub mod wifi;

// Re-export commonly used items
pub use ble::{Fragment, FragmentError, Fragmenter, Reassembler};
pub use lora::{calculate_airtime_ms, calculate_airtime_us, DutyCycleLimiter, LoRaParams};
pub use wifi::{ConfigCommand, ConfigError, WifiConfig, WifiStatus};

// Re-export testing items (only with tap-tests feature)
#[cfg(feature = "tap-tests")]
pub use testing::TestRunner;
