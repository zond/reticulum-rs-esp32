//! WiFi configuration data structures.
//!
//! This module contains platform-independent types for WiFi configuration
//! that can be tested on the host machine.
//!
//! # Example
//!
//! ```
//! use reticulum_rs_esp32::wifi::{WifiConfig, ConfigCommand};
//!
//! let config = WifiConfig::new("MyNetwork", "MyPassword").unwrap();
//! assert!(config.validate().is_ok());
//!
//! let cmd: ConfigCommand = "connect".parse().unwrap();
//! assert_eq!(cmd, ConfigCommand::Connect);
//! ```

use std::fmt;

/// Maximum SSID length per IEEE 802.11 standard.
pub const MAX_SSID_LEN: usize = 32;

/// Maximum password length for WPA2.
pub const MAX_PASSWORD_LEN: usize = 64;

/// Minimum password length for WPA2.
pub const MIN_PASSWORD_LEN: usize = 8;

/// Connection timeout in seconds.
pub const CONNECTION_TIMEOUT_SECS: u64 = 30;

/// WiFi credentials for connecting to an access point.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WifiConfig {
    /// Network SSID (1-32 bytes).
    pub ssid: String,
    /// Network password (8-64 bytes for WPA2, empty for open networks).
    pub password: String,
}

impl WifiConfig {
    /// Create a new WiFi configuration.
    ///
    /// Returns an error if SSID or password are invalid.
    pub fn new(ssid: impl Into<String>, password: impl Into<String>) -> Result<Self, ConfigError> {
        let config = Self {
            ssid: ssid.into(),
            password: password.into(),
        };
        config.validate()?;
        Ok(config)
    }

    /// Create a configuration for an open network (no password).
    pub fn open(ssid: impl Into<String>) -> Result<Self, ConfigError> {
        let config = Self {
            ssid: ssid.into(),
            password: String::new(),
        };
        config.validate()?;
        Ok(config)
    }

    /// Validate the configuration.
    pub fn validate(&self) -> Result<(), ConfigError> {
        // Validate SSID
        if self.ssid.is_empty() {
            return Err(ConfigError::SsidEmpty);
        }
        if self.ssid.len() > MAX_SSID_LEN {
            return Err(ConfigError::SsidTooLong {
                len: self.ssid.len(),
                max: MAX_SSID_LEN,
            });
        }

        // Validate password (empty is OK for open networks)
        if !self.password.is_empty() && self.password.len() < MIN_PASSWORD_LEN {
            return Err(ConfigError::PasswordTooShort {
                len: self.password.len(),
                min: MIN_PASSWORD_LEN,
            });
        }
        if self.password.len() > MAX_PASSWORD_LEN {
            return Err(ConfigError::PasswordTooLong {
                len: self.password.len(),
                max: MAX_PASSWORD_LEN,
            });
        }

        Ok(())
    }

    /// Check if this is an open network (no password).
    pub fn is_open(&self) -> bool {
        self.password.is_empty()
    }

    /// Serialize to bytes for NVS storage.
    ///
    /// Format: `[ssid_len:1][ssid:N][password_len:1][password:M]`
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(2 + self.ssid.len() + self.password.len());
        bytes.push(self.ssid.len() as u8);
        bytes.extend_from_slice(self.ssid.as_bytes());
        bytes.push(self.password.len() as u8);
        bytes.extend_from_slice(self.password.as_bytes());
        bytes
    }

    /// Deserialize from bytes.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, ConfigError> {
        if bytes.is_empty() {
            return Err(ConfigError::InvalidFormat("empty data".into()));
        }

        let ssid_len = bytes[0] as usize;
        if bytes.len() < 1 + ssid_len + 1 {
            return Err(ConfigError::InvalidFormat("truncated SSID".into()));
        }

        let ssid = String::from_utf8(bytes[1..1 + ssid_len].to_vec())
            .map_err(|_| ConfigError::InvalidFormat("invalid SSID UTF-8".into()))?;

        let password_len = bytes[1 + ssid_len] as usize;
        let password_start = 2 + ssid_len;
        if bytes.len() < password_start + password_len {
            return Err(ConfigError::InvalidFormat("truncated password".into()));
        }

        let password =
            String::from_utf8(bytes[password_start..password_start + password_len].to_vec())
                .map_err(|_| ConfigError::InvalidFormat("invalid password UTF-8".into()))?;

        Self::new(ssid, password)
    }
}

/// WiFi connection status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WifiStatus {
    /// No WiFi credentials configured.
    Unconfigured,
    /// Attempting to connect to the network.
    Connecting,
    /// Successfully connected with the given IP address.
    Connected { ip: String },
    /// Connection failed with the given reason.
    Failed { reason: String },
}

impl WifiStatus {
    /// Convert status to a string for BLE transmission.
    pub fn to_ble_string(&self) -> String {
        match self {
            Self::Unconfigured => "unconfigured".to_string(),
            Self::Connecting => "connecting".to_string(),
            Self::Connected { ip } => format!("connected:{}", ip),
            Self::Failed { reason } => format!("failed:{}", reason),
        }
    }

    /// Parse status from a BLE string.
    pub fn from_ble_string(s: &str) -> Result<Self, ConfigError> {
        if s == "unconfigured" {
            return Ok(Self::Unconfigured);
        }
        if s == "connecting" {
            return Ok(Self::Connecting);
        }
        if let Some(ip) = s.strip_prefix("connected:") {
            return Ok(Self::Connected { ip: ip.to_string() });
        }
        if let Some(reason) = s.strip_prefix("failed:") {
            return Ok(Self::Failed {
                reason: reason.to_string(),
            });
        }
        Err(ConfigError::InvalidFormat(format!("unknown status: {}", s)))
    }
}

impl fmt::Display for WifiStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_ble_string())
    }
}

/// Commands that can be sent via BLE to control WiFi.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigCommand {
    /// Connect to the configured network.
    Connect,
    /// Disconnect from the current network.
    Disconnect,
    /// Clear stored credentials.
    Clear,
}

impl ConfigCommand {
    /// Convert command to string for BLE.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Connect => "connect",
            Self::Disconnect => "disconnect",
            Self::Clear => "clear",
        }
    }
}

impl std::str::FromStr for ConfigCommand {
    type Err = ConfigError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_lowercase().as_str() {
            "connect" => Ok(Self::Connect),
            "disconnect" => Ok(Self::Disconnect),
            "clear" => Ok(Self::Clear),
            _ => Err(ConfigError::UnknownCommand(s.to_string())),
        }
    }
}

impl fmt::Display for ConfigCommand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Errors that can occur during configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigError {
    /// SSID is empty.
    SsidEmpty,
    /// SSID exceeds maximum length.
    SsidTooLong { len: usize, max: usize },
    /// Password is too short for WPA2.
    PasswordTooShort { len: usize, min: usize },
    /// Password exceeds maximum length.
    PasswordTooLong { len: usize, max: usize },
    /// Invalid data format during deserialization.
    InvalidFormat(String),
    /// Unknown command string.
    UnknownCommand(String),
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SsidEmpty => write!(f, "SSID cannot be empty"),
            Self::SsidTooLong { len, max } => {
                write!(f, "SSID too long: {} bytes (max {})", len, max)
            }
            Self::PasswordTooShort { len, min } => {
                write!(f, "password too short: {} bytes (min {})", len, min)
            }
            Self::PasswordTooLong { len, max } => {
                write!(f, "password too long: {} bytes (max {})", len, max)
            }
            Self::InvalidFormat(msg) => write!(f, "invalid format: {}", msg),
            Self::UnknownCommand(cmd) => write!(f, "unknown command: {}", cmd),
        }
    }
}

impl std::error::Error for ConfigError {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    // ==================== WifiConfig Tests ====================

    #[test]
    fn test_valid_config() {
        let config = WifiConfig::new("TestNetwork", "password123").unwrap();
        assert_eq!(config.ssid, "TestNetwork");
        assert_eq!(config.password, "password123");
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_open_network() {
        let config = WifiConfig::open("OpenNetwork").unwrap();
        assert!(config.is_open());
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_empty_ssid() {
        let result = WifiConfig::new("", "password123");
        assert_eq!(result, Err(ConfigError::SsidEmpty));
    }

    #[test]
    fn test_ssid_too_long() {
        let long_ssid = "a".repeat(33);
        let result = WifiConfig::new(long_ssid, "password123");
        assert!(matches!(result, Err(ConfigError::SsidTooLong { .. })));
    }

    #[test]
    fn test_ssid_max_length() {
        let max_ssid = "a".repeat(32);
        let config = WifiConfig::new(max_ssid, "password123").unwrap();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_password_too_short() {
        let result = WifiConfig::new("TestNetwork", "short");
        assert!(matches!(result, Err(ConfigError::PasswordTooShort { .. })));
    }

    #[test]
    fn test_password_min_length() {
        let config = WifiConfig::new("TestNetwork", "12345678").unwrap();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_password_too_long() {
        let long_password = "a".repeat(65);
        let result = WifiConfig::new("TestNetwork", long_password);
        assert!(matches!(result, Err(ConfigError::PasswordTooLong { .. })));
    }

    #[test]
    fn test_password_max_length() {
        let max_password = "a".repeat(64);
        let config = WifiConfig::new("TestNetwork", max_password).unwrap();
        assert!(config.validate().is_ok());
    }

    // ==================== Serialization Tests ====================

    #[test]
    fn test_serialize_deserialize() {
        let config = WifiConfig::new("MyNetwork", "MyPassword").unwrap();
        let bytes = config.to_bytes();
        let restored = WifiConfig::from_bytes(&bytes).unwrap();
        assert_eq!(config, restored);
    }

    #[test]
    fn test_serialize_open_network() {
        let config = WifiConfig::open("OpenNet").unwrap();
        let bytes = config.to_bytes();
        let restored = WifiConfig::from_bytes(&bytes).unwrap();
        assert_eq!(config, restored);
        assert!(restored.is_open());
    }

    #[test]
    fn test_deserialize_empty() {
        let result = WifiConfig::from_bytes(&[]);
        assert!(matches!(result, Err(ConfigError::InvalidFormat(_))));
    }

    #[test]
    fn test_deserialize_truncated() {
        let result = WifiConfig::from_bytes(&[5, b'h', b'e', b'l', b'l']); // Missing 'o' and password
        assert!(matches!(result, Err(ConfigError::InvalidFormat(_))));
    }

    // ==================== WifiStatus Tests ====================

    #[test]
    fn test_status_unconfigured() {
        let status = WifiStatus::Unconfigured;
        assert_eq!(status.to_ble_string(), "unconfigured");
        assert_eq!(WifiStatus::from_ble_string("unconfigured").unwrap(), status);
    }

    #[test]
    fn test_status_connecting() {
        let status = WifiStatus::Connecting;
        assert_eq!(status.to_ble_string(), "connecting");
        assert_eq!(WifiStatus::from_ble_string("connecting").unwrap(), status);
    }

    #[test]
    fn test_status_connected() {
        let status = WifiStatus::Connected {
            ip: "192.168.1.100".to_string(),
        };
        assert_eq!(status.to_ble_string(), "connected:192.168.1.100");
        assert_eq!(
            WifiStatus::from_ble_string("connected:192.168.1.100").unwrap(),
            status
        );
    }

    #[test]
    fn test_status_failed() {
        let status = WifiStatus::Failed {
            reason: "wrong password".to_string(),
        };
        assert_eq!(status.to_ble_string(), "failed:wrong password");
        assert_eq!(
            WifiStatus::from_ble_string("failed:wrong password").unwrap(),
            status
        );
    }

    #[test]
    fn test_status_unknown() {
        let result = WifiStatus::from_ble_string("bogus");
        assert!(matches!(result, Err(ConfigError::InvalidFormat(_))));
    }

    // ==================== ConfigCommand Tests ====================

    #[test]
    fn test_command_connect() {
        assert_eq!(
            ConfigCommand::from_str("connect").unwrap(),
            ConfigCommand::Connect
        );
        assert_eq!(
            ConfigCommand::from_str("CONNECT").unwrap(),
            ConfigCommand::Connect
        );
        assert_eq!(
            ConfigCommand::from_str("  Connect  ").unwrap(),
            ConfigCommand::Connect
        );
    }

    #[test]
    fn test_command_disconnect() {
        assert_eq!(
            ConfigCommand::from_str("disconnect").unwrap(),
            ConfigCommand::Disconnect
        );
    }

    #[test]
    fn test_command_clear() {
        assert_eq!(
            ConfigCommand::from_str("clear").unwrap(),
            ConfigCommand::Clear
        );
    }

    #[test]
    fn test_command_unknown() {
        let result = ConfigCommand::from_str("reboot");
        assert!(matches!(result, Err(ConfigError::UnknownCommand(_))));
    }

    #[test]
    fn test_command_as_str() {
        assert_eq!(ConfigCommand::Connect.as_str(), "connect");
        assert_eq!(ConfigCommand::Disconnect.as_str(), "disconnect");
        assert_eq!(ConfigCommand::Clear.as_str(), "clear");
    }
}
