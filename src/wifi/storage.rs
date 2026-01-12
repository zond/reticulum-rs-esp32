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
/// Adding small margin for safety.
const MAX_CONFIG_BUFFER_SIZE: usize = 1 + MAX_SSID_LEN + 1 + MAX_PASSWORD_LEN + 4;

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

/// Initialize NVS partition for WiFi configuration.
pub fn init_nvs() -> Result<EspNvs<NvsDefault>, EspError> {
    use esp_idf_svc::nvs::EspNvsPartition;
    let partition = EspNvsPartition::<NvsDefault>::take()?;
    EspNvs::new(partition, NVS_NAMESPACE, true)
}
