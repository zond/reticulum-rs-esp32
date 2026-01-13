//! TCP transport for testnet connections.
//!
//! This module provides TCP connectivity to Reticulum testnet entry points.
//! Works on both host (via standard sockets) and ESP32 (via ESP-IDF sockets).
//!
//! # Platform Notes
//!
//! - **Host**: Works directly with std::net
//! - **ESP32**: Requires WiFi to be connected first (caller's responsibility)
//! - **QEMU**: Will fail at runtime (no network emulation)

use super::config::TestnetServer;
use log::{debug, error, info, warn};
use std::io::{self, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

/// Default connection timeout in seconds.
const CONNECT_TIMEOUT_SECS: u64 = 10;

/// Default read timeout in seconds.
const READ_TIMEOUT_SECS: u64 = 30;

/// Testnet transport connection.
pub struct TestnetTransport {
    stream: TcpStream,
    server_name: String,
}

impl TestnetTransport {
    /// Connect to a testnet server.
    ///
    /// # Arguments
    ///
    /// * `server` - The testnet server to connect to
    ///
    /// # Errors
    ///
    /// Returns an error if DNS resolution or TCP connection fails.
    /// On ESP32, ensure WiFi is connected before calling this.
    pub fn connect(server: &TestnetServer) -> Result<Self, TransportError> {
        info!(
            "Connecting to testnet: {} ({})",
            server.name,
            server.address()
        );

        // Resolve hostname
        let addr = server
            .address()
            .to_socket_addrs()
            .map_err(|e| {
                error!("DNS resolution failed for {}: {}", server.host, e);
                TransportError::DnsResolution(e)
            })?
            .next()
            .ok_or_else(|| {
                error!("No addresses found for {}", server.host);
                TransportError::NoAddresses
            })?;

        debug!("Resolved {} to {}", server.host, addr);

        // Connect with timeout
        let stream = TcpStream::connect_timeout(&addr, Duration::from_secs(CONNECT_TIMEOUT_SECS))
            .map_err(|e| {
            error!("TCP connection failed to {}: {}", addr, e);
            TransportError::Connection(e)
        })?;

        // Configure timeouts (log failures but continue - non-critical)
        if let Err(e) = stream.set_read_timeout(Some(Duration::from_secs(READ_TIMEOUT_SECS))) {
            warn!("Failed to set read timeout: {}", e);
        }
        if let Err(e) = stream.set_write_timeout(Some(Duration::from_secs(CONNECT_TIMEOUT_SECS))) {
            warn!("Failed to set write timeout: {}", e);
        }

        // Disable Nagle's algorithm for lower latency
        if let Err(e) = stream.set_nodelay(true) {
            warn!("Failed to disable Nagle's algorithm: {}", e);
        }

        info!(
            "Connected to testnet {} at {}",
            server.name,
            stream
                .peer_addr()
                .map(|a| a.to_string())
                .unwrap_or_default()
        );

        Ok(Self {
            stream,
            server_name: server.name.to_string(),
        })
    }

    /// Try connecting to any available testnet server.
    ///
    /// Attempts each server in order until one succeeds.
    pub fn connect_any(servers: &[TestnetServer]) -> Result<Self, TransportError> {
        let mut last_error = None;

        for server in servers {
            match Self::connect(server) {
                Ok(transport) => return Ok(transport),
                Err(e) => {
                    warn!("Failed to connect to {}: {}", server.name, e);
                    last_error = Some(e);
                }
            }
        }

        Err(last_error.unwrap_or(TransportError::NoServers))
    }

    /// Get the name of the connected server.
    pub fn server_name(&self) -> &str {
        &self.server_name
    }

    /// Best-effort check if the connection may still be alive.
    ///
    /// NOTE: This is not reliable. False positives are possible - the remote
    /// peer may have closed the connection but we won't know until we try I/O.
    /// The only reliable way to detect disconnection is to attempt read/write.
    pub fn may_be_connected(&self) -> bool {
        self.stream.peer_addr().is_ok()
    }

    /// Send raw bytes to the testnet.
    pub fn send(&mut self, data: &[u8]) -> Result<usize, TransportError> {
        self.stream.write(data).map_err(TransportError::Io)
    }

    /// Receive bytes from the testnet.
    ///
    /// Returns the number of bytes read into the buffer.
    /// Returns 0 if the connection was closed.
    pub fn receive(&mut self, buffer: &mut [u8]) -> Result<usize, TransportError> {
        self.stream.read(buffer).map_err(TransportError::Io)
    }

    /// Get the underlying TCP stream for advanced use.
    pub fn stream(&self) -> &TcpStream {
        &self.stream
    }

    /// Get mutable access to the underlying TCP stream.
    pub fn stream_mut(&mut self) -> &mut TcpStream {
        &mut self.stream
    }
}

impl std::fmt::Debug for TestnetTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TestnetTransport")
            .field("server", &self.server_name)
            .field(
                "peer",
                &self
                    .stream
                    .peer_addr()
                    .map(|a| a.to_string())
                    .unwrap_or_else(|_| "unknown".to_string()),
            )
            .finish()
    }
}

/// Transport errors.
#[derive(Debug)]
pub enum TransportError {
    /// DNS resolution failed.
    DnsResolution(io::Error),
    /// No addresses found for hostname.
    NoAddresses,
    /// TCP connection failed.
    Connection(io::Error),
    /// I/O error during read/write.
    Io(io::Error),
    /// No servers provided.
    NoServers,
}

impl std::fmt::Display for TransportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DnsResolution(e) => write!(f, "DNS resolution failed: {}", e),
            Self::NoAddresses => write!(f, "no addresses found for hostname"),
            Self::Connection(e) => write!(f, "connection failed: {}", e),
            Self::Io(e) => write!(f, "I/O error: {}", e),
            Self::NoServers => write!(f, "no servers provided"),
        }
    }
}

impl std::error::Error for TransportError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::DnsResolution(e) | Self::Connection(e) | Self::Io(e) => Some(e),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testnet::config::{DEFAULT_SERVER, SERVERS};
    use reticulum_rs_esp32_macros::esp32_test;

    // Note: These tests require network access and will fail in QEMU.
    // They are marked with #[ignore] for CI but can be run manually.

    #[esp32_test]
    #[ignore] // Requires network
    fn test_connect_to_default_server() {
        let transport = TestnetTransport::connect(DEFAULT_SERVER);
        assert!(
            transport.is_ok(),
            "Failed to connect: {:?}",
            transport.err()
        );

        let transport = transport.unwrap();
        assert!(transport.may_be_connected());
        assert_eq!(transport.server_name(), DEFAULT_SERVER.name);
    }

    #[esp32_test]
    #[ignore] // Requires network
    fn test_connect_any() {
        let transport = TestnetTransport::connect_any(SERVERS);
        assert!(
            transport.is_ok(),
            "Failed to connect to any server: {:?}",
            transport.err()
        );
    }

    #[esp32_test]
    fn test_no_servers_error() {
        let result = TestnetTransport::connect_any(&[]);
        assert!(matches!(result, Err(TransportError::NoServers)));
    }
}
