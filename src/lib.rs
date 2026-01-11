//! Reticulum-rs ESP32 firmware library.
//!
//! This library contains platform-independent components that can be tested
//! on the host machine without ESP32 hardware.

pub mod ble;
pub mod lora;

// Re-export commonly used items
pub use ble::{Fragment, FragmentError, Fragmenter, Reassembler};
pub use lora::{calculate_airtime_ms, calculate_airtime_us, DutyCycleLimiter, LoRaParams};
