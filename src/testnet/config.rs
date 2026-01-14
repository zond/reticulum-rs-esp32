//! Testnet server configuration.
//!
//! Platform-independent configuration for Reticulum testnet entry points.

/// A testnet server entry point.
#[derive(Debug, Clone)]
pub struct TestnetServer {
    /// Human-readable name.
    pub name: &'static str,
    /// Hostname or IP address.
    pub host: &'static str,
    /// TCP port.
    pub port: u16,
}

impl TestnetServer {
    /// Create a new testnet server configuration.
    pub const fn new(name: &'static str, host: &'static str, port: u16) -> Self {
        Self { name, host, port }
    }

    /// Get the address string for connection (host:port).
    pub fn address(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}

/// Dublin testnet hub.
pub const DUBLIN: TestnetServer =
    TestnetServer::new("Dublin", "dublin.connect.reticulum.network", 4965);

/// Frankfurt testnet hub.
pub const FRANKFURT: TestnetServer =
    TestnetServer::new("Frankfurt", "frankfurt.connect.reticulum.network", 5377);

/// BetweenTheBorders community hub.
pub const BETWEEN_THE_BORDERS: TestnetServer =
    TestnetServer::new("BetweenTheBorders", "reticulum.betweentheborders.com", 4242);

/// All available testnet servers.
pub const SERVERS: &[TestnetServer] = &[DUBLIN, FRANKFURT, BETWEEN_THE_BORDERS];

/// Default server to use.
pub const DEFAULT_SERVER: &TestnetServer = &DUBLIN;

#[cfg(test)]
mod tests {
    use super::*;
    use reticulum_rs_esp32_macros::esp32_test;

    #[esp32_test]
    fn test_server_address() {
        assert_eq!(DUBLIN.address(), "dublin.connect.reticulum.network:4965");
    }

    #[esp32_test]
    fn test_frankfurt_config() {
        assert_eq!(FRANKFURT.host, "frankfurt.connect.reticulum.network");
        assert_eq!(FRANKFURT.port, 5377);
    }

    #[esp32_test]
    #[allow(clippy::const_is_empty)] // Intentional sanity check that SERVERS wasn't emptied
    fn test_servers_list_not_empty() {
        assert!(!SERVERS.is_empty());
    }

    #[esp32_test]
    fn test_default_server_in_list() {
        assert!(SERVERS.iter().any(|s| s.host == DEFAULT_SERVER.host));
    }
}
