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

---

*Updated 2026-01-12*
