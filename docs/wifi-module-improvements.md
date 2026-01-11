# WiFi Module Future Improvements

This document tracks improvements identified by code review for the WiFi configuration module. These are not blocking issues but should be addressed for production hardening.

## Security

### Password Memory Zeroing (Priority: High)
**Location:** `src/wifi/config.rs`, `src/wifi/ble_service.rs`

WiFi passwords are stored as `String` which doesn't zero memory on drop. For production use, implement credential zeroing:

```rust
// Add to Cargo.toml
zeroize = "1.7"

// In config.rs
use zeroize::{Zeroize, ZeroizeOnDrop};

#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct SecureString(String);
```

### BLE Security Documentation (Priority: Medium)
**Location:** `src/wifi/ble_service.rs`

Add security considerations to module docs:
- BLE communication is encrypted at link layer but credentials are plaintext at application level
- Pairing should be performed in a secure environment
- Consider implementing PIN or out-of-band authentication for production

## Correctness

### Missing Bounds Check in from_bytes (Priority: High)
**Location:** `src/wifi/config.rs:125`

The `from_bytes()` function reads password length without verifying the byte exists:
```rust
let password_len = bytes[1 + ssid_len] as usize;  // Could panic
```

Fix: Add bounds check before reading:
```rust
if bytes.len() < 2 + ssid_len {
    return Err(ConfigError::InvalidFormat("missing password length".into()));
}
```

### Validate Lengths Before Allocation (Priority: Medium)
**Location:** `src/wifi/config.rs:117-133`

Validate SSID/password lengths against MAX constants before allocating strings to prevent memory exhaustion on ESP32.

### Connection Timeout Implementation (Priority: Medium)
**Location:** `src/wifi/connection.rs`

The `CONNECTION_TIMEOUT_SECS` constant (30s) is defined in config.rs but never used. Verify if `BlockingWifi::connect()` has built-in timeout or implement explicit timeout handling.

## Performance

### Status String Allocations (Priority: Low)
**Location:** `src/wifi/config.rs:154-161`

`WifiStatus::to_ble_string()` allocates new strings for static values. Consider using `Cow<'static, str>`:
```rust
pub fn to_ble_string(&self) -> Cow<'static, str> {
    match self {
        Self::Unconfigured => "unconfigured".into(),
        Self::Connecting => "connecting".into(),
        Self::Connected { ip } => format!("connected:{}", ip).into(),
        Self::Failed { reason } => format!("failed:{}", reason).into(),
    }
}
```

### NVS Buffer Size (Priority: Low)
**Location:** `src/wifi/storage.rs:22`

Replace magic number with named constant:
```rust
const MAX_CONFIG_BUFFER_SIZE: usize = 1 + MAX_SSID_LEN + 1 + MAX_PASSWORD_LEN;  // 98 bytes
```

## Architecture

### NVS Re-initialization Guard (Priority: Medium)
**Location:** `src/wifi/storage.rs:39-45`

`EspNvsPartition::take()` can only succeed once. Add guard against multiple initialization calls using `OnceLock` or similar.

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

### Additional Tests
- Test exact buffer boundaries in `from_bytes()` (panic scenario)
- Test malformed UTF-8 in serialized data
- Test NVS corruption scenarios

---

*Generated from rust-code-guardian review on 2026-01-11*
