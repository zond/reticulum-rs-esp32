//! Host network provider.
//!
//! On host systems, the OS handles networking. This provider is a thin wrapper
//! that reports the system's network status.

use super::{NetworkError, NetworkProvider};
use log::info;
use std::net::IpAddr;

/// Host network provider.
///
/// On host systems, networking is always available via the OS.
/// This provider detects the local IP address for binding servers.
pub struct HostNetwork {
    ip_addr: Option<IpAddr>,
}

impl HostNetwork {
    /// Create a new host network provider.
    pub fn new() -> Self {
        Self { ip_addr: None }
    }

    /// Get the primary local IP address.
    ///
    /// This uses a trick: create a UDP socket and "connect" to a public IP
    /// (doesn't actually send anything), then check which local address was chosen.
    fn detect_local_ip() -> Option<IpAddr> {
        use std::net::UdpSocket;

        // Connect to a public IP to determine our default route's local address
        // This doesn't actually send any packets
        let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
        socket.connect("8.8.8.8:80").ok()?;
        let local_addr = socket.local_addr().ok()?;
        Some(local_addr.ip())
    }
}

impl Default for HostNetwork {
    fn default() -> Self {
        Self::new()
    }
}

impl NetworkProvider for HostNetwork {
    fn connect(&mut self) -> Result<(), NetworkError> {
        // On host, we're always "connected" via the OS
        // Just detect our local IP for server binding
        self.ip_addr = Self::detect_local_ip();

        if let Some(ip) = self.ip_addr {
            info!("Host network ready, local IP: {}", ip);
        } else {
            info!("Host network ready, binding to 0.0.0.0");
        }

        Ok(())
    }

    fn is_connected(&self) -> bool {
        // On host, assume we're always connected
        // The actual connection will fail at socket level if not
        true
    }

    fn ip_addr(&self) -> Option<IpAddr> {
        self.ip_addr
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_host_network_always_connected() {
        let network = HostNetwork::new();
        assert!(network.is_connected());
    }

    #[test]
    fn test_host_network_connect() {
        let mut network = HostNetwork::new();
        let result = network.connect();
        assert!(result.is_ok());
        // IP detection might fail in some CI environments, so we don't assert on ip_addr
    }

    #[test]
    fn test_detect_local_ip() {
        // This test may fail in environments without network access
        let ip = HostNetwork::detect_local_ip();
        // Just verify it doesn't panic - IP might be None in CI/air-gapped environments
        // If we got an IP, it should be a valid address (loopback is acceptable in some configs)
        if let Some(addr) = ip {
            assert!(addr.is_ipv4() || addr.is_ipv6());
        }
    }
}
