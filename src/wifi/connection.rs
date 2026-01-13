//! WiFi connection management.
//!
//! This module wraps ESP-IDF WiFi driver functionality for connecting
//! to access points.

use super::config::WifiConfig;
use esp_idf_hal::modem::Modem;
use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::wifi::{AuthMethod, BlockingWifi, ClientConfiguration, Configuration, EspWifi};
use esp_idf_sys::EspError;
use log::info;

/// WiFi connection manager.
pub struct WifiManager<'a> {
    /// ESP-IDF WiFi driver.
    wifi: BlockingWifi<EspWifi<'a>>,
}

impl<'a> WifiManager<'a> {
    /// Create a new WiFi manager.
    pub fn new(modem: Modem, sysloop: EspSystemEventLoop) -> Result<Self, EspError> {
        let esp_wifi = EspWifi::new(modem, sysloop.clone(), None)?;
        let wifi = BlockingWifi::wrap(esp_wifi, sysloop)?;

        Ok(Self { wifi })
    }

    /// Connect to a WiFi network.
    ///
    /// Returns the IP address on success.
    pub fn connect(&mut self, config: &WifiConfig) -> Result<String, WifiError> {
        info!("Connecting to WiFi: {}", config.ssid);

        // Determine auth method
        let auth_method = if config.is_open() {
            AuthMethod::None
        } else {
            AuthMethod::WPA2Personal
        };

        // Configure WiFi
        let wifi_config = Configuration::Client(ClientConfiguration {
            ssid: config
                .ssid
                .as_str()
                .try_into()
                .map_err(|_| WifiError::InvalidSsid)?,
            password: config
                .password
                .as_str()
                .try_into()
                .map_err(|_| WifiError::InvalidPassword)?,
            auth_method,
            ..Default::default()
        });

        self.wifi.set_configuration(&wifi_config)?;

        // Start WiFi
        self.wifi.start()?;

        // Connect (relies on ESP-IDF's internal timeout mechanisms)
        self.wifi.connect().map_err(WifiError::ConnectionFailed)?;

        // Wait for DHCP
        self.wifi.wait_netif_up().map_err(WifiError::DhcpFailed)?;

        // Get IP address
        let ip_info = self.wifi.wifi().sta_netif().get_ip_info()?;
        let ip = format!("{}", ip_info.ip);

        info!("Connected to WiFi, IP: {}", ip);
        Ok(ip)
    }

    /// Disconnect from the current network.
    pub fn disconnect(&mut self) -> Result<(), EspError> {
        info!("Disconnecting from WiFi");
        self.wifi.disconnect()?;
        self.wifi.stop()?;
        Ok(())
    }

    /// Check if currently connected.
    pub fn is_connected(&self) -> bool {
        self.wifi.is_connected().unwrap_or(false)
    }

    /// Get current IP address if connected.
    pub fn get_ip(&self) -> Option<String> {
        if !self.is_connected() {
            return None;
        }
        self.wifi
            .wifi()
            .sta_netif()
            .get_ip_info()
            .ok()
            .map(|info| format!("{}", info.ip))
    }
}

/// Errors that can occur during WiFi operations.
#[derive(Debug)]
pub enum WifiError {
    /// SSID is invalid (too long or contains invalid characters).
    InvalidSsid,
    /// Password is invalid.
    InvalidPassword,
    /// Failed to connect to the network.
    ConnectionFailed(EspError),
    /// Failed to obtain IP address via DHCP.
    DhcpFailed(EspError),
    /// ESP-IDF error.
    EspError(EspError),
}

impl From<EspError> for WifiError {
    fn from(e: EspError) -> Self {
        Self::EspError(e)
    }
}

impl std::fmt::Display for WifiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidSsid => write!(f, "invalid SSID"),
            Self::InvalidPassword => write!(f, "invalid password"),
            Self::ConnectionFailed(e) => write!(f, "connection failed: {:?}", e),
            Self::DhcpFailed(e) => write!(f, "DHCP failed: {:?}", e),
            Self::EspError(e) => write!(f, "ESP error: {:?}", e),
        }
    }
}

impl std::error::Error for WifiError {}
