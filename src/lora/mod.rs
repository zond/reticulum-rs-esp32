//! LoRa radio support.
//!
//! This module contains:
//! - [`config`]: Region configuration and standard LoRa parameters
//! - [`duty_cycle`]: Duty cycle limiter for regulatory compliance
//! - [`airtime`]: Time-on-air calculation for LoRa packets
//! - [`csma`]: CSMA/CA collision avoidance for shared frequencies
//! - [`radio`]: SX1262 radio driver (ESP32 only)
//! - [`iface`]: Reticulum-rs transport interface adapter (ESP32 only)

mod airtime;
mod config;
mod csma;
mod duty_cycle;

#[cfg(feature = "esp32")]
mod iface;
#[cfg(feature = "esp32")]
mod radio;

pub use airtime::{calculate_airtime_ms, calculate_airtime_us, LoRaParams};
pub use config::{
    Region, BANDWIDTH_HZ, CODING_RATE, LORA_MTU, LOW_DATA_RATE_OPTIMIZE, PREAMBLE_LENGTH,
    SPREADING_FACTOR, SYNC_WORD, TX_POWER,
};
pub use csma::{Csma, CsmaConfig, CsmaError, CsmaResult};
pub use duty_cycle::DutyCycleLimiter;

#[cfg(feature = "esp32")]
pub use iface::LoRaInterface;
#[cfg(feature = "esp32")]
pub use radio::{LoRaRadio, RadioError, ReceivedPacket};
