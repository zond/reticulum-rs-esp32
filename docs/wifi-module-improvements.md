# WiFi Module Future Improvements

This document tracks improvements identified by code review for the WiFi configuration module.

## Correctness

### Connection Timeout Implementation (Priority: Medium)
**Location:** `src/wifi/connection.rs`

The `CONNECTION_TIMEOUT_SECS` constant (30s) is defined in config.rs but never used. Verify if `BlockingWifi::connect()` has built-in timeout or implement explicit timeout handling.

## Performance

### ~~NVS Buffer Size~~ ✓ RESOLVED
**Location:** `src/wifi/storage.rs:20`

~~Replace magic number with named constant.~~

**Resolution:** Updated to use named constants:
```rust
const MAX_CONFIG_BUFFER_SIZE: usize = 1 + MAX_SSID_LEN + 1 + MAX_PASSWORD_LEN;
```

## Architecture

### ~~NVS Re-initialization Guard~~ ✓ RESOLVED
**Location:** `src/wifi/storage.rs` / `src/lib.rs`

~~`EspNvsPartition::take()` can only succeed once. Add guard against multiple initialization calls using `OnceLock` or similar.~~

**Resolution:** Added shared `get_nvs_default_partition()` function in `lib.rs` using `OnceLock` to ensure `EspNvsPartition::take()` is called at most once. Both `persistence.rs` and `wifi/storage.rs` now use this shared partition accessor.

### Error Information Preservation (Priority: Low)
**Location:** `src/wifi/connection.rs:65, 70`

Converting `EspError` to string via `format!("{:?}", e)` loses error codes. Consider storing original error:
```rust
pub enum WifiError {
    ConnectionFailed(EspError),  // Store error directly
    DhcpFailed(EspError),
}
```

## Future Enhancements

### Serialization Versioning
Add version byte to `to_bytes()`/`from_bytes()` for forward compatibility if format changes (e.g., WPA3 support).

---

*Updated 2026-01-12*
