//! Network abstraction layer.
//!
//! This module provides a platform-independent network interface:
//! - **ESP32** (`esp32` feature): WiFi-based connectivity
//! - **Host** (default): Native OS networking
//!
//! # Example
//!
//! ```ignore
//! use reticulum_rs_esp32::network::NetworkProvider;
//!
//! // Platform-specific initialization
//! #[cfg(feature = "esp32")]
//! let mut network = network::WifiNetwork::new(modem, sysloop, nvs)?;
//!
//! #[cfg(not(feature = "esp32"))]
//! let mut network = network::HostNetwork::new();
//!
//! // Same code for both platforms
//! network.connect()?;
//! println!("Connected, IP: {:?}", network.ip_addr());
//! ```

use std::net::IpAddr;

#[cfg(feature = "esp32")]
mod wifi;

#[cfg(not(feature = "esp32"))]
mod host;

mod stats_server;

// Re-exports
#[cfg(feature = "esp32")]
pub use wifi::WifiNetwork;

#[cfg(not(feature = "esp32"))]
pub use host::HostNetwork;

pub use stats_server::{NodeStats, StatsServer, DEFAULT_STATS_PORT};

/// Network provider abstraction.
///
/// This trait abstracts over platform-specific network initialization,
/// allowing the same application code to run on ESP32 (WiFi) and host (native).
pub trait NetworkProvider: Send {
    /// Connect to the network.
    ///
    /// - On ESP32: Loads WiFi credentials from NVS and connects
    /// - On Host: No-op (always connected)
    fn connect(&mut self) -> Result<(), NetworkError>;

    /// Check if the network is connected.
    fn is_connected(&self) -> bool;

    /// Get the local IP address.
    ///
    /// Returns `None` if not connected.
    fn ip_addr(&self) -> Option<IpAddr>;
}

/// Network errors.
#[derive(Debug)]
pub enum NetworkError {
    /// No WiFi credentials configured (ESP32).
    NotConfigured,
    /// WiFi connection failed (ESP32).
    #[cfg(feature = "esp32")]
    WifiError(crate::wifi::WifiError),
    /// Generic I/O error.
    Io(std::io::Error),
}

impl std::fmt::Display for NetworkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotConfigured => write!(f, "network not configured"),
            #[cfg(feature = "esp32")]
            Self::WifiError(e) => write!(f, "WiFi error: {}", e),
            Self::Io(e) => write!(f, "I/O error: {}", e),
        }
    }
}

impl std::error::Error for NetworkError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::NotConfigured => None,
            #[cfg(feature = "esp32")]
            Self::WifiError(e) => Some(e),
            Self::Io(e) => Some(e),
        }
    }
}

impl From<std::io::Error> for NetworkError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

#[cfg(feature = "esp32")]
impl From<crate::wifi::WifiError> for NetworkError {
    fn from(e: crate::wifi::WifiError) -> Self {
        Self::WifiError(e)
    }
}

#[cfg(feature = "esp32")]
impl From<esp_idf_sys::EspError> for NetworkError {
    fn from(e: esp_idf_sys::EspError) -> Self {
        Self::Io(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("ESP error: {:?}", e),
        ))
    }
}
