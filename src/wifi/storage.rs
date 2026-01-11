//! NVS persistence for WiFi credentials.
//!
//! This module stores WiFi credentials in ESP32's Non-Volatile Storage (NVS)
//! so they persist across reboots.

#![allow(dead_code)] // Functions will be used when main.rs is integrated

use super::config::WifiConfig;
use esp_idf_svc::nvs::{EspNvs, NvsDefault};
use esp_idf_sys::EspError;

/// NVS namespace for WiFi configuration.
const NVS_NAMESPACE: &str = "wifi_config";

/// NVS key for stored credentials.
const NVS_KEY: &str = "credentials";

/// Load WiFi configuration from NVS.
///
/// Returns `None` if no configuration is stored or if it's corrupted.
pub fn load_wifi_config(nvs: &EspNvs<NvsDefault>) -> Option<WifiConfig> {
    let mut buf = [0u8; 128]; // Max SSID (32) + Max Password (64) + overhead
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
