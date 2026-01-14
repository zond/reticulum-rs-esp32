//! Identity persistence for host (development) builds.
//!
//! Stores the node's private identity in a file so it persists across runs.
//! Uses `~/.reticulum-rs-esp32/identity.hex` by default.
//!
//! # Usage
//!
//! ```ignore
//! use reticulum_rs_esp32::persistence_host;
//!
//! let identity = persistence_host::load_or_create_identity()?;
//! log::info!("Node identity: {}", identity.address_hash());
//! ```

use log::info;
use rand_core::OsRng;
use reticulum::identity::PrivateIdentity;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Get the default identity file path.
///
/// Returns `~/.reticulum-rs-esp32/identity.hex`
pub fn default_identity_path() -> io::Result<PathBuf> {
    let home = std::env::var("HOME")
        .map_err(|_| io::Error::new(io::ErrorKind::NotFound, "HOME not set"))?;
    Ok(PathBuf::from(home)
        .join(".reticulum-rs-esp32")
        .join("identity.hex"))
}

/// Load identity from a specific path.
///
/// Returns `None` if no identity file exists or if the data is corrupted.
pub fn load_identity_from(path: &Path) -> Option<PrivateIdentity> {
    let hex_str = match fs::read_to_string(path) {
        Ok(s) => s.trim().to_string(),
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            log::debug!("No identity file found at {:?}", path);
            return None;
        }
        Err(e) => {
            log::warn!("Failed to read identity file: {}", e);
            return None;
        }
    };

    match PrivateIdentity::new_from_hex_string(&hex_str) {
        Ok(identity) => Some(identity),
        Err(e) => {
            log::error!("Failed to parse stored identity: {:?}", e);
            None
        }
    }
}

/// Load identity from the default path.
pub fn load_identity() -> Option<PrivateIdentity> {
    let path = default_identity_path().ok()?;
    load_identity_from(&path)
}

/// Save identity to a specific path.
pub fn save_identity_to(identity: &PrivateIdentity, path: &Path) -> io::Result<()> {
    // Create parent directory if needed
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let hex_string = identity.to_hex_string();
    fs::write(path, &hex_string)?;

    // Verify write by reading back
    let read_back = fs::read_to_string(path)?;
    if read_back != hex_string {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "Identity verification failed: wrote {} bytes, read {} bytes",
                hex_string.len(),
                read_back.len()
            ),
        ));
    }

    info!("Identity saved to {:?}", path);
    Ok(())
}

/// Save identity to the default path.
pub fn save_identity(identity: &PrivateIdentity) -> io::Result<()> {
    let path = default_identity_path()?;
    save_identity_to(identity, &path)
}

/// Load existing identity from path or create and persist a new one.
pub fn load_or_create_identity_at(path: &Path) -> io::Result<PrivateIdentity> {
    if let Some(identity) = load_identity_from(path) {
        info!("Loaded existing identity from {:?}", path);
        return Ok(identity);
    }

    info!("Creating new identity");
    let identity = PrivateIdentity::new_from_rand(OsRng);
    save_identity_to(&identity, path)?;
    Ok(identity)
}

/// Load existing identity or create and persist a new one using the default path.
///
/// This is the main entry point for identity management. On first run,
/// creates a new random identity and saves it. On subsequent runs,
/// loads the existing identity.
pub fn load_or_create_identity() -> io::Result<PrivateIdentity> {
    let path = default_identity_path()?;
    load_or_create_identity_at(&path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::sync::atomic::{AtomicU32, Ordering};

    // Counter to ensure unique test files even in parallel execution
    static TEST_COUNTER: AtomicU32 = AtomicU32::new(0);

    fn unique_identity_path() -> PathBuf {
        let id = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
        let pid = std::process::id();
        env::temp_dir().join(format!("reticulum-test-{}-{}.hex", pid, id))
    }

    #[test]
    fn test_identity_roundtrip() {
        let path = unique_identity_path();

        let identity = PrivateIdentity::new_from_rand(OsRng);
        let original_hex = identity.to_hex_string();
        save_identity_to(&identity, &path).expect("Failed to save");

        let loaded = load_identity_from(&path).expect("Failed to load");
        assert_eq!(original_hex, loaded.to_hex_string());

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn test_load_or_create() {
        let path = unique_identity_path();

        // First call creates new identity
        let id1 = load_or_create_identity_at(&path).expect("Failed to create");
        let hex1 = id1.to_hex_string();

        // Second call loads existing identity
        let id2 = load_or_create_identity_at(&path).expect("Failed to load");
        let hex2 = id2.to_hex_string();

        assert_eq!(hex1, hex2, "Should load same identity");

        let _ = fs::remove_file(&path);
    }
}
