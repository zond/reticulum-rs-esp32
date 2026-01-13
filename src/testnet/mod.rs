//! Reticulum testnet connectivity.
//!
//! This module provides TCP transport to Reticulum testnet entry points.
//!
//! # Platform Support
//!
//! | Platform | Status | Notes |
//! |----------|--------|-------|
//! | Host | Works | Uses std::net directly |
//! | ESP32 | Works | Requires WiFi connected first |
//! | QEMU | Fails | No network emulation |
//!
//! # Example
//!
//! ```no_run
//! use reticulum_rs_esp32::testnet::{TestnetTransport, DEFAULT_SERVER, SERVERS};
//!
//! // Connect to default server
//! let mut transport = TestnetTransport::connect(DEFAULT_SERVER)?;
//! println!("Connected to {}", transport.server_name());
//!
//! // Or try any available server
//! let mut transport = TestnetTransport::connect_any(SERVERS)?;
//! # Ok::<(), reticulum_rs_esp32::testnet::TransportError>(())
//! ```
//!
//! # ESP32 Usage
//!
//! On ESP32, ensure WiFi is connected before attempting testnet connection:
//!
//! ```ignore
//! // 1. Connect to WiFi first
//! let mut wifi = WifiManager::new(modem, sysloop)?;
//! wifi.connect(&wifi_config)?;
//!
//! // 2. Then connect to testnet
//! let transport = TestnetTransport::connect(DEFAULT_SERVER)?;
//! ```

mod config;
mod transport;

pub use config::{TestnetServer, BETWEEN_THE_BORDERS, DEFAULT_SERVER, DUBLIN, FRANKFURT, SERVERS};
pub use transport::{TestnetTransport, TransportError};
