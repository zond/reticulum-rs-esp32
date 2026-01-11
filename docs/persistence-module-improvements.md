# Persistence Module Future Improvements

This document tracks improvements identified by code review for the identity persistence module.

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

## Correctness

### Buffer Size Verification (Priority: High)
**Location:** `src/persistence.rs:42`

The `IDENTITY_HEX_LEN` constant (128) assumes two 32-byte keys = 64 bytes = 128 hex characters. This assumption should be verified against actual `PrivateIdentity::to_hex_string()` output. Requires ESP32 target or QEMU to test.

### Verification After Save (Priority: High)
**Location:** `src/persistence.rs:88-89`

After generating a new identity and saving it, there's no verification that the save succeeded and the data is retrievable. On embedded systems, flash writes can fail without returning an error.

**Impact:** If save appears to succeed but data is corrupted, the device will generate a different identity on every boot, breaking Reticulum routing.

### RNG Entropy Verification (Priority: Medium)
**Location:** `src/persistence.rs:86`

The code relies on `OsRng` for key generation without verifying that entropy is sufficient or that the RNG is properly initialized.

## Architecture

### Error Context Loss (Priority: Medium)
**Location:** `src/persistence.rs:47-52`

The `load_identity` function silently converts all errors into `None`. This makes debugging impossible—you can't tell if the identity is missing, corrupted, or if there's a hardware failure.

### NVS Re-initialization Guard (Priority: Medium)
**Location:** `src/persistence.rs:94-97`

`EspNvsPartition::take()` can only succeed once. Add guard against multiple initialization calls using `OnceLock` or similar.

## Testing

### Add Unit Tests (Priority: Low)

Add tests for:
- Identity save/load round-trip
- Clear identity functionality
- Error handling paths

Note: Tests require ESP32 hardware or QEMU emulation.

---

*Updated 2026-01-11*
