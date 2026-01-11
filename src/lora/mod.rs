//! LoRa radio support.
//!
//! This module contains:
//! - [`duty_cycle`]: Duty cycle limiter for regulatory compliance
//! - [`airtime`]: Time-on-air calculation for LoRa packets

mod airtime;
mod duty_cycle;

pub use airtime::{calculate_airtime_ms, calculate_airtime_us, LoRaParams};
pub use duty_cycle::DutyCycleLimiter;
