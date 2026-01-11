//! Identity persistence for Reticulum node.
//!
//! This module stores the node's private identity in ESP32's Non-Volatile Storage (NVS)
//! so it persists across reboots. A stable identity is essential for Reticulum routing
//! and destination addressing.
//!
//! # Usage
//!
//! ```ignore
//! use reticulum_rs_esp32::persistence;
//!
//! let mut nvs = persistence::init_nvs()?;
//! let identity = persistence::load_or_create_identity(&mut nvs)?;
//! log::info!("Node identity: {}", identity.address_hash());
//! ```

use esp_idf_svc::nvs::{EspNvs, EspNvsPartition, NvsDefault};
use esp_idf_sys::EspError;
use log::info;
use rand_core::OsRng;
use reticulum::identity::PrivateIdentity;

/// NVS namespace for Reticulum identity storage.
const NVS_NAMESPACE: &str = "reticulum";

/// NVS key for the device identity.
const IDENTITY_KEY: &str = "device_id";

/// Size of identity when serialized as hex string.
/// Two 32-byte keys = 64 bytes = 128 hex characters.
const IDENTITY_HEX_LEN: usize = 128;

/// Load identity from NVS.
///
/// Returns `None` if no identity is stored or if the stored data is corrupted.
pub fn load_identity(nvs: &EspNvs<NvsDefault>) -> Option<PrivateIdentity> {
    let mut buf = [0u8; IDENTITY_HEX_LEN + 1];
    let bytes = nvs.get_raw(IDENTITY_KEY, &mut buf).ok()??;
    let hex_str = core::str::from_utf8(bytes).ok()?;
    PrivateIdentity::new_from_hex_string(hex_str).ok()
}

/// Save identity to NVS.
///
/// Serializes the identity as a hex string for storage.
pub fn save_identity(
    nvs: &mut EspNvs<NvsDefault>,
    identity: &PrivateIdentity,
) -> Result<(), EspError> {
    let hex_string = identity.to_hex_string();
    nvs.set_raw(IDENTITY_KEY, hex_string.as_bytes())?;
    Ok(())
}

/// Clear stored identity from NVS.
///
/// After calling this, the next boot will generate a new identity.
pub fn clear_identity(nvs: &mut EspNvs<NvsDefault>) -> Result<(), EspError> {
    nvs.remove(IDENTITY_KEY)?;
    Ok(())
}

/// Load existing identity or create and persist a new one.
///
/// This is the main entry point for identity management. On first boot,
/// creates a new random identity and saves it. On subsequent boots,
/// loads the existing identity.
pub fn load_or_create_identity(nvs: &mut EspNvs<NvsDefault>) -> Result<PrivateIdentity, EspError> {
    if let Some(identity) = load_identity(nvs) {
        info!("Loaded existing identity");
        return Ok(identity);
    }

    info!("Creating new identity");
    let identity = PrivateIdentity::new_from_rand(OsRng);
    save_identity(nvs, &identity)?;
    Ok(identity)
}

/// Initialize NVS partition for Reticulum identity storage.
///
/// Must be called before any other persistence functions.
pub fn init_nvs() -> Result<EspNvs<NvsDefault>, EspError> {
    let partition = EspNvsPartition::<NvsDefault>::take()?;
    EspNvs::new(partition, NVS_NAMESPACE, true)
}
