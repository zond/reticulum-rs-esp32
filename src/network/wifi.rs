//! ESP32 WiFi network provider.
//!
//! This module provides network connectivity via WiFi on ESP32 devices.
//! It loads credentials from NVS (configured via BLE) and connects automatically.

use super::{NetworkError, NetworkProvider};
use crate::wifi::{load_wifi_config, WifiManager};
use esp_idf_hal::modem::Modem;
use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::nvs::{EspNvs, NvsDefault};
use log::{info, warn};
use std::net::IpAddr;

/// NVS namespace for WiFi configuration.
const NVS_NAMESPACE: &str = "wifi_config";

/// WiFi-based network provider for ESP32.
///
/// Loads WiFi credentials from NVS and manages the connection.
pub struct WifiNetwork<'a> {
    wifi: WifiManager<'a>,
    nvs: EspNvs<NvsDefault>,
    ip_addr: Option<IpAddr>,
}

impl<'a> WifiNetwork<'a> {
    /// Create a new WiFi network provider.
    ///
    /// # Arguments
    ///
    /// * `modem` - The WiFi/BT modem peripheral
    /// * `sysloop` - The ESP-IDF system event loop
    ///
    /// # Errors
    ///
    /// Returns an error if WiFi initialization fails.
    pub fn new(modem: Modem, sysloop: EspSystemEventLoop) -> Result<Self, NetworkError> {
        let wifi = WifiManager::new(modem, sysloop)?;

        // Initialize NVS for WiFi config
        let partition = crate::get_nvs_default_partition()?;
        let nvs = EspNvs::new(partition, NVS_NAMESPACE, true)?;

        Ok(Self {
            wifi,
            nvs,
            ip_addr: None,
        })
    }

    /// Check if WiFi credentials are configured.
    pub fn is_configured(&self) -> bool {
        load_wifi_config(&self.nvs).is_some()
    }
}

impl<'a> NetworkProvider for WifiNetwork<'a> {
    fn connect(&mut self) -> Result<(), NetworkError> {
        // Load credentials from NVS
        let config = load_wifi_config(&self.nvs).ok_or(NetworkError::NotConfigured)?;

        info!("Connecting to WiFi: {}", config.ssid);

        // Connect to WiFi
        let ip_string = self.wifi.connect(&config)?;

        // Parse IP address
        match ip_string.parse() {
            Ok(ip) => {
                self.ip_addr = Some(ip);
                info!("WiFi connected, IP: {}", ip_string);
            }
            Err(e) => {
                // Log warning but don't fail - the connection is still valid
                warn!(
                    "WiFi connected but failed to parse IP '{}': {}",
                    ip_string, e
                );
                self.ip_addr = None;
            }
        }

        Ok(())
    }

    fn is_connected(&self) -> bool {
        self.wifi.is_connected()
    }

    fn ip_addr(&self) -> Option<IpAddr> {
        if self.is_connected() {
            self.ip_addr
        } else {
            None
        }
    }
}
