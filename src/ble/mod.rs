//! BLE interface components.
//!
//! This module contains components for the BLE mesh interface, including
//! packet fragmentation for handling BLE's small MTU.

mod fragmentation;

pub use fragmentation::{Fragment, FragmentError, Fragmenter, Reassembler};
