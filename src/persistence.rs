//! Identity persistence for Reticulum node.
//!
//! This module stores the node's private identity in ESP32's Non-Volatile Storage (NVS)
//! so it persists across reboots. A stable identity is essential for Reticulum routing
//! and destination addressing.
//!
//! # Security
//!
//! For production devices, use `cargo production-flash` which enables NVS encryption.
//! Development builds (default) do not encrypt NVS data.
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

use esp_idf_svc::nvs::{EspNvs, NvsDefault};
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
/// Errors are logged for debugging purposes.
pub fn load_identity(nvs: &EspNvs<NvsDefault>) -> Option<PrivateIdentity> {
    let mut buf = [0u8; IDENTITY_HEX_LEN + 1];

    let bytes = match nvs.get_raw(IDENTITY_KEY, &mut buf) {
        Ok(Some(bytes)) => bytes,
        Ok(None) => {
            log::debug!("No identity found in NVS");
            return None;
        }
        Err(e) => {
            log::warn!("Failed to read identity from NVS: {:?}", e);
            return None;
        }
    };

    let hex_str = match core::str::from_utf8(bytes) {
        Ok(s) => s,
        Err(e) => {
            log::error!("Stored identity is not valid UTF-8: {:?}", e);
            return None;
        }
    };

    match PrivateIdentity::new_from_hex_string(hex_str) {
        Ok(identity) => Some(identity),
        Err(e) => {
            log::error!("Failed to parse stored identity: {:?}", e);
            None
        }
    }
}

/// Save identity to NVS with read-back verification.
///
/// Serializes the identity as a hex string for storage, then reads it back
/// to verify the write succeeded. This catches flash write failures that
/// may not return an error code.
pub fn save_identity(
    nvs: &mut EspNvs<NvsDefault>,
    identity: &PrivateIdentity,
) -> Result<(), EspError> {
    let hex_string = identity.to_hex_string();

    // Verify our buffer size constant is correct
    debug_assert_eq!(
        hex_string.len(),
        IDENTITY_HEX_LEN,
        "IDENTITY_HEX_LEN ({}) doesn't match actual hex string length ({})",
        IDENTITY_HEX_LEN,
        hex_string.len()
    );

    nvs.set_raw(IDENTITY_KEY, hex_string.as_bytes())?;

    // Read back and verify to catch silent flash write failures
    let mut verify_buf = [0u8; IDENTITY_HEX_LEN + 1];
    let read_bytes = nvs
        .get_raw(IDENTITY_KEY, &mut verify_buf)
        .map_err(|e| {
            log::error!("Failed to read back identity after save: {:?}", e);
            e
        })?
        .ok_or_else(|| {
            log::error!("Identity not found after save - possible flash failure");
            EspError::from_infallible::<{ esp_idf_sys::ESP_ERR_NVS_NOT_FOUND }>()
        })?;

    if read_bytes != hex_string.as_bytes() {
        log::error!("Identity verification failed - data mismatch after save");
        return Err(EspError::from_infallible::<
            { esp_idf_sys::ESP_ERR_INVALID_CRC },
        >());
    }

    info!("Identity saved and verified in NVS");
    Ok(())
}

/// Clear stored identity from NVS.
///
/// After calling this, the next boot will generate a new identity.
pub fn clear_identity(nvs: &mut EspNvs<NvsDefault>) -> Result<(), EspError> {
    nvs.remove(IDENTITY_KEY)?;
    log::warn!("Identity cleared from NVS - new identity will be generated on next boot");
    Ok(())
}

/// Load existing identity or create and persist a new one.
///
/// This is the main entry point for identity management. On first boot,
/// creates a new random identity and saves it. On subsequent boots,
/// loads the existing identity.
///
/// # Entropy Source
///
/// New identities are generated using `OsRng`, which on ESP32 uses the
/// hardware random number generator (RNG). The ESP32 RNG derives entropy
/// from hardware thermal noise and is initialized by ESP-IDF during boot.
/// See: <https://docs.espressif.com/projects/esp-idf/en/latest/esp32/api-reference/system/random.html>
pub fn load_or_create_identity(nvs: &mut EspNvs<NvsDefault>) -> Result<PrivateIdentity, EspError> {
    if let Some(identity) = load_identity(nvs) {
        info!("Loaded existing identity");
        return Ok(identity);
    }

    info!("Creating new identity using hardware RNG");
    let identity = PrivateIdentity::new_from_rand(OsRng);
    save_identity(nvs, &identity)?;
    Ok(identity)
}

/// Initialize NVS for Reticulum identity storage.
///
/// Uses a shared partition handle to ensure `EspNvsPartition::take()` is only
/// called once across the entire application. Safe to call multiple times.
///
/// Must be called before any other persistence functions.
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
    fn test_save_load_identity_roundtrip() {
        crate::ensure_esp_initialized();
        let mut nvs = init_nvs().expect("Failed to init NVS");

        // Use Box to reduce stack usage - PrivateIdentity crypto ops need stack space
        let identity = Box::new(PrivateIdentity::new_from_rand(OsRng));
        let original_hex = identity.to_hex_string();

        save_identity(&mut nvs, &identity).expect("Failed to save identity");
        drop(identity); // Free stack space before loading

        let loaded = load_identity(&nvs);
        assert!(loaded.is_some(), "Failed to load identity");
        assert_eq!(original_hex, loaded.unwrap().to_hex_string());
    }

    #[esp32_test]
    fn test_clear_identity() {
        crate::ensure_esp_initialized();
        let mut nvs = init_nvs().expect("Failed to init NVS");

        // Create, save, then drop to minimize stack usage
        {
            let identity = PrivateIdentity::new_from_rand(OsRng);
            save_identity(&mut nvs, &identity).expect("Failed to save identity");
        }

        assert!(load_identity(&nvs).is_some());
        clear_identity(&mut nvs).expect("Failed to clear identity");
        assert!(load_identity(&nvs).is_none());
    }

    #[esp32_test]
    fn test_load_nonexistent_identity() {
        crate::ensure_esp_initialized();
        let mut nvs = init_nvs().expect("Failed to init NVS");

        let _ = clear_identity(&mut nvs);
        assert!(load_identity(&nvs).is_none());
    }

    #[esp32_test]
    fn test_load_or_create_identity() {
        crate::ensure_esp_initialized();
        let mut nvs = init_nvs().expect("Failed to init NVS");

        // Clear and create new
        let _ = clear_identity(&mut nvs);
        let hex1 = load_or_create_identity(&mut nvs)
            .expect("Failed to create")
            .to_hex_string();

        // Should load existing (same identity)
        let hex2 = load_or_create_identity(&mut nvs)
            .expect("Failed to load")
            .to_hex_string();

        assert_eq!(hex1, hex2, "Should load same identity");
    }

    #[esp32_test]
    fn test_identity_hex_length_constant() {
        crate::ensure_esp_initialized();

        let identity = PrivateIdentity::new_from_rand(OsRng);
        assert_eq!(
            identity.to_hex_string().len(),
            IDENTITY_HEX_LEN,
            "IDENTITY_HEX_LEN constant is incorrect"
        );
    }
}
