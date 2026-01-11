# Persistence Module Future Improvements

This document tracks improvements identified by code review for the identity persistence module. These are not blocking issues but should be addressed for production hardening.

## Security

### NVS Encryption (Priority: Critical)
**Location:** `src/persistence.rs`

Private identity keys (Ed25519 and X25519) are stored as plaintext hex strings in NVS. ESP32 NVS is stored in flash memory without encryption by default. Anyone with physical access can extract private keys using flash dump tools.

**Impact:** Compromised private keys allow:
- Impersonation of the node on the Reticulum network
- Decryption of past communications
- Persistent identity theft across the mesh network

**Solution:** Enable ESP-IDF's NVS encryption feature:
- Use `nvs_flash_secure_init()` for encrypted NVS partition
- Requires flash encryption to be enabled in menuconfig
- See: [NVS Encryption Guide](https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/storage/nvs_encryption.html)

### Module Documentation Security Warning (Priority: Medium)
**Location:** `src/persistence.rs:1-15`

The module documentation should mention that private keys are stored in NVS and reference security requirements for production deployments.

## Correctness

### Buffer Size Verification (Priority: High)
**Location:** `src/persistence.rs:31`

The `IDENTITY_HEX_LEN` constant (128) assumes two 32-byte keys = 64 bytes = 128 hex characters. This assumption should be verified against actual `PrivateIdentity::to_hex_string()` output.

**Solution:** Add a test that verifies the constant matches actual serialization length:
```rust
#[test]
fn test_identity_hex_length() {
    let identity = PrivateIdentity::new_from_rand(OsRng);
    assert_eq!(identity.to_hex_string().len(), IDENTITY_HEX_LEN);
}
```

### Verification After Save (Priority: High)
**Location:** `src/persistence.rs:74-78`

After generating a new identity and saving it, there's no verification that the save succeeded and the data is retrievable. On embedded systems, flash writes can fail without returning an error.

**Impact:** If save appears to succeed but data is corrupted, the device will generate a different identity on every boot, breaking Reticulum routing.

**Solution:** Read back and verify identity after save:
```rust
save_identity(nvs, &identity)?;
match load_identity(nvs) {
    Some(loaded) if loaded.to_hex_string() == identity.to_hex_string() => {
        info!("Identity saved and verified successfully");
        Ok(identity)
    }
    _ => Err(EspError::from_infallible::<{ esp_idf_sys::ESP_FAIL }>())
}
```

### RNG Entropy Verification (Priority: Medium)
**Location:** `src/persistence.rs:75`

The code relies on `OsRng` for key generation without verifying that entropy is sufficient or that the RNG is properly initialized.

**Solution:** Add entropy sanity check before generating keys:
```rust
let mut test_bytes = [0u8; 32];
OsRng.fill_bytes(&mut test_bytes);
if test_bytes.iter().all(|&b| b == 0) {
    log::error!("RNG appears non-functional");
    return Err(/* ... */);
}
```

## Architecture

### Error Context Loss (Priority: Medium)
**Location:** `src/persistence.rs:36-41`

The `load_identity` function silently converts all errors into `None`. This makes debugging impossible—you can't tell if the identity is missing, corrupted, or if there's a hardware failure.

**Solution:** Return a `Result` type with proper error information:
```rust
pub enum IdentityLoadError {
    NotFound,
    Corrupted(String),
    NvsError(EspError),
}
```

### NVS Re-initialization Guard (Priority: Medium)
**Location:** `src/persistence.rs:83-86`

`EspNvsPartition::take()` can only succeed once. Add guard against multiple initialization calls using `OnceLock` or similar (same as WiFi module issue).

## Suggestions

### Add Unit Tests (Priority: Low)
**Location:** Entire module

Add tests for:
- Identity save/load round-trip
- Clear identity functionality
- Error handling paths

Note: Tests require ESP32 hardware or mocking infrastructure.

### Add Logging for Critical Operations (Priority: Low)
**Location:** `src/persistence.rs:46-53, 55-61`

Save and clear operations have no logging. In production embedded systems, you want to know when these rare but critical operations occur.

```rust
pub fn save_identity(...) -> Result<(), EspError> {
    // ...
    nvs.set_raw(IDENTITY_KEY, hex_string.as_bytes())?;
    log::info!("Identity saved to NVS");
    Ok(())
}
```

---

*Generated from rust-code-guardian review on 2026-01-11*
