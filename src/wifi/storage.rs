//! NVS persistence for WiFi credentials.
//!
//! This module stores WiFi credentials in ESP32's Non-Volatile Storage (NVS)
//! so they persist across reboots.

#![allow(dead_code)] // Functions will be used when main.rs is integrated

use super::config::{WifiConfig, MAX_PASSWORD_LEN, MAX_SSID_LEN};
use esp_idf_svc::nvs::{EspNvs, NvsDefault};
use esp_idf_sys::EspError;

/// NVS namespace for WiFi configuration.
const NVS_NAMESPACE: &str = "wifi_config";

/// NVS key for stored credentials.
const NVS_KEY: &str = "credentials";

/// Maximum buffer size for WiFi config serialization.
/// Format: [ssid_len:1][ssid:32][password_len:1][password:64] = 98 bytes.
const MAX_CONFIG_BUFFER_SIZE: usize = 1 + MAX_SSID_LEN + 1 + MAX_PASSWORD_LEN;

/// Load WiFi configuration from NVS.
///
/// Returns `None` if no configuration is stored or if it's corrupted.
pub fn load_wifi_config(nvs: &EspNvs<NvsDefault>) -> Option<WifiConfig> {
    let mut buf = [0u8; MAX_CONFIG_BUFFER_SIZE];
    let bytes = nvs.get_raw(NVS_KEY, &mut buf).ok()??;
    WifiConfig::from_bytes(bytes).ok()
}

/// Save WiFi configuration to NVS.
pub fn save_wifi_config(nvs: &mut EspNvs<NvsDefault>, config: &WifiConfig) -> Result<(), EspError> {
    let bytes = config.to_bytes();
    nvs.set_raw(NVS_KEY, &bytes)?;
    Ok(())
}

/// Clear stored WiFi configuration from NVS.
pub fn clear_wifi_config(nvs: &mut EspNvs<NvsDefault>) -> Result<(), EspError> {
    nvs.remove(NVS_KEY)?;
    Ok(())
}

/// Initialize NVS for WiFi configuration.
///
/// Uses a shared partition handle to ensure `EspNvsPartition::take()` is only
/// called once across the entire application. Safe to call multiple times.
pub fn init_nvs() -> Result<EspNvs<NvsDefault>, EspError> {
    let partition = crate::get_nvs_default_partition()?;
    EspNvs::new(partition, NVS_NAMESPACE, true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use reticulum_rs_esp32_macros::esp32_test;

    #[esp32_test]
    fn test_init_nvs() {
        crate::ensure_esp_initialized();
        let nvs = init_nvs();
        assert!(nvs.is_ok(), "Failed to initialize NVS: {:?}", nvs.err());
    }

    #[esp32_test]
    fn test_save_load_roundtrip() {
        crate::ensure_esp_initialized();
        let mut nvs = init_nvs().expect("Failed to init NVS");

        let config =
            WifiConfig::new("TestNetwork", "password123").expect("Failed to create config");
        save_wifi_config(&mut nvs, &config).expect("Failed to save config");

        let loaded = load_wifi_config(&nvs);
        assert!(loaded.is_some(), "Failed to load config");
        assert_eq!(loaded.unwrap(), config);
    }

    #[esp32_test]
    fn test_save_load_open_network() {
        crate::ensure_esp_initialized();
        let mut nvs = init_nvs().expect("Failed to init NVS");

        let config = WifiConfig::open("OpenNetwork").expect("Failed to create config");
        save_wifi_config(&mut nvs, &config).expect("Failed to save config");

        let loaded = load_wifi_config(&nvs).expect("Failed to load config");
        assert_eq!(loaded, config);
        assert!(loaded.is_open());
    }

    #[esp32_test]
    fn test_clear_config() {
        crate::ensure_esp_initialized();
        let mut nvs = init_nvs().expect("Failed to init NVS");

        // Save a config first
        let config =
            WifiConfig::new("TestNetwork", "password123").expect("Failed to create config");
        save_wifi_config(&mut nvs, &config).expect("Failed to save config");

        // Verify it's saved
        assert!(load_wifi_config(&nvs).is_some());

        // Clear it
        clear_wifi_config(&mut nvs).expect("Failed to clear config");

        // Verify it's gone
        assert!(load_wifi_config(&nvs).is_none());
    }

    #[esp32_test]
    fn test_load_nonexistent() {
        crate::ensure_esp_initialized();
        let mut nvs = init_nvs().expect("Failed to init NVS");

        // Clear any existing config first
        let _ = clear_wifi_config(&mut nvs);

        // Loading should return None
        let loaded = load_wifi_config(&nvs);
        assert!(loaded.is_none());
    }

    #[esp32_test]
    fn test_overwrite_config() {
        crate::ensure_esp_initialized();
        let mut nvs = init_nvs().expect("Failed to init NVS");

        // Save first config
        let config1 = WifiConfig::new("Network1", "password1").expect("Failed to create config1");
        save_wifi_config(&mut nvs, &config1).expect("Failed to save config1");

        // Save second config (overwrite)
        let config2 = WifiConfig::new("Network2", "password2").expect("Failed to create config2");
        save_wifi_config(&mut nvs, &config2).expect("Failed to save config2");

        // Should load the second config
        let loaded = load_wifi_config(&nvs).expect("Failed to load config");
        assert_eq!(loaded, config2);
        assert_ne!(loaded, config1);
    }
}
