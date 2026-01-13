# Embedded Testing Strategy

Testing system for ESP32 firmware that runs the same tests on host and QEMU.

## Overview

This project uses a custom `#[esp32_test]` macro that wraps standard Rust `#[test]`. This enables:

- **Same tests everywhere**: Identical test code runs on host and QEMU
- **Simple commands**: `cargo test` for host, `cargo test-qemu` for QEMU
- **Standard Rust tooling**: Works with cargo test, IDE integrations, etc.
- **ESP-IDF initialization**: Automatically initializes ESP-IDF on ESP32 targets

## Quick Start

```bash
# Run tests on host (default, fastest)
cargo test

# Run tests in QEMU (ESP32 emulation)
cargo test-qemu
```

## How It Works

### The `#[esp32_test]` Macro

The macro does two things:
1. Adds `#[test]` so the Rust compiler collects it
2. Injects ESP-IDF initialization on ESP32 targets

```rust
#[esp32_test]
fn my_test() {
    assert_eq!(2 + 2, 4);
}

// Expands to:
#[test]
fn my_test() {
    #[cfg(feature = "esp32")]
    {
        crate::ensure_esp_initialized(); // Once-initialized ESP-IDF
    }
    assert_eq!(2 + 2, 4);
}
```

### QEMU Test Runner

The `cargo test-qemu` command runs a host binary (`qemu-test-runner`) that:
1. Builds tests for ESP32: `cargo test --no-run --target xtensa-esp32-espidf --features esp32 --release`
2. Creates flash image: `espflash save-image --merge`
3. Runs QEMU with the image
4. Parses output for test results
5. Exits with appropriate code (0 = success, 1 = failure)

## Writing Tests

Tests use the `#[esp32_test]` attribute:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use reticulum_rs_esp32_macros::esp32_test;

    #[esp32_test]
    fn test_basic_functionality() {
        assert_eq!(2 + 2, 4);
    }

    #[esp32_test]
    fn test_with_result() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let value: i32 = "42".parse()?;
        assert_eq!(value, 42);
        Ok(())
    }
}
```

### Key Points

1. **Import the macro**: `use reticulum_rs_esp32_macros::esp32_test;`
2. **Return types**: Tests can return `()` or `Result<(), E>`
3. **No `#[should_panic]`**: Use `try_new()` pattern instead (panic=abort on ESP32)
4. **Timing tolerance**: Use `saturating_duration_since()` for clock operations

### Avoid `#[should_panic]`

ESP32 builds use `panic=abort`, so `#[should_panic]` tests don't work. Instead, use fallible constructors:

```rust
// Instead of:
#[should_panic]
fn test_invalid_input() {
    MyType::new(invalid_value);
}

// Use:
#[esp32_test]
fn test_invalid_input() {
    assert!(matches!(
        MyType::try_new(invalid_value),
        Err(MyError::InvalidInput)
    ));
}
```

### Timing-Dependent Tests

QEMU has clock jitter. Use tolerances instead of exact values:

```rust
// Instead of:
assert_eq!(limiter.remaining(), expected);

// Use:
assert!(limiter.remaining() < expected + tolerance);
```

## File Structure

```
macros/
└── src/lib.rs              # #[esp32_test] proc-macro

src/
├── lib.rs                  # ensure_esp_initialized()
├── bin/
│   └── qemu-test-runner.rs # QEMU test orchestrator
└── */mod.rs                # Modules with #[cfg(test)] tests
```

## Test Coverage

| Module | Tests | Description |
|--------|-------|-------------|
| `announce/cache.rs` | 16 | LRU announce cache for deduplication |
| `ble/fragmentation.rs` | 27 | BLE packet fragmentation/reassembly |
| `config/wifi.rs` | 26 | WiFi credential validation |
| `lora/airtime.rs` | 14 | LoRa time-on-air calculations |
| `lora/config.rs` | 4 | Region configuration |
| `lora/csma.rs` | 23 | CSMA/CA collision avoidance |
| `lora/duty_cycle.rs` | 8 | Token bucket duty cycle limiter |
| `persistence.rs` | 6 | Identity NVS storage (ESP32 only) |
| `routing/path_table.rs` | 17 | Routing table for destination paths |
| `testnet/config.rs` | 4 | Testnet server configuration |
| `testnet/transport.rs` | 1 | TCP transport (+ 2 ignored network tests) |
| `wifi/storage.rs` | 6 | WiFi config NVS storage (ESP32 only) |
| **Total** | **152** | 140 host + 12 ESP32-only |

## Testing Environments

### Host Testing

Fastest iteration. Tests run natively without ESP32 dependencies.

**What can be tested:**
- Pure logic (validation, calculations, parsing)
- Data structures (serialization, fragmentation)
- Algorithms (airtime calculation, duty cycle)

**What cannot be tested:**
- ESP-IDF APIs (WiFi, BLE, NVS)
- Hardware peripherals

### QEMU Testing

Tests ESP32 code without real hardware. Uses plain ESP32 (not S3) due to QEMU stdout bug.

**What can be tested:**
- ESP-IDF initialization
- Memory allocation patterns
- Code that compiles for ESP32

**What cannot be tested:**
- WiFi/BLE (no radio emulation)
- LoRa (no SX1262 emulation)

## Best Practices

### 1. Testing Priority: Host First, Then QEMU

**Priority order:**
1. **Host tests** (`cargo test`) - Fastest iteration, pure logic
2. **QEMU tests** (`cargo test-qemu`) - ESP32-specific code that can't run on host
3. **Device tests** - Only for hardware that QEMU can't emulate (radio, peripherals)

Separate logic from hardware to maximize host-testable code:

```rust
// Good: Pure logic, host-testable
pub fn validate_ssid(ssid: &str) -> Result<(), ConfigError> {
    if ssid.is_empty() { return Err(ConfigError::Empty); }
    if ssid.len() > 32 { return Err(ConfigError::TooLong); }
    Ok(())
}

// ESP32-specific wrapper, tested in QEMU
#[cfg(feature = "esp32")]
pub fn save_to_nvs(nvs: &mut EspNvs, ssid: &str) -> Result<(), EspError> {
    validate_ssid(ssid)?;
    nvs.set_raw("ssid", ssid.as_bytes())
}
```

For ESP32-specific code that can't run on host (NVS, ESP-IDF APIs), write tests that run in QEMU. See `persistence::tests` and `wifi::storage::tests` for examples.

### 2. Use Feature Flags Consistently

```rust
// Platform-independent (always compiled)
pub mod config;

// Platform-dependent (ESP32 only)
#[cfg(feature = "esp32")]
pub mod storage;
```

## Known Limitations

### Why Custom Macro Instead of Standard `#[test]`?

Standard `#[test]` works, but ESP-IDF needs initialization before tests run. The `#[esp32_test]` macro ensures `link_patches()` and logger init happen once.

### QEMU Limitations

- ESP32-S3 has stdout bug (use plain ESP32 target)
- No WiFi/BLE/LoRa radio emulation
- Clock jitter can affect timing-dependent tests
- Some peripherals not emulated

## References

- [ESP-IDF QEMU Guide](https://docs.espressif.com/projects/esp-idf/en/stable/esp32s3/api-guides/tools/qemu.html)
- [esp-rs Book](https://esp-rs.github.io/book/)
- [Espressif QEMU Releases](https://github.com/espressif/qemu/releases)

---

*Updated 2026-01-13: Clarified testing priority (host first, then QEMU)*
