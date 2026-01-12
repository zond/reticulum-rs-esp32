# Embedded Testing Strategy

Unified TAP testing system for ESP32 firmware that runs the same tests on host, QEMU, and real hardware.

## Overview

This project uses a custom TAP (Test Anything Protocol) testing system instead of the standard `cargo test` harness. This enables:

- **Same tests everywhere**: Identical test code runs on host, QEMU, and device
- **TAP output**: Standard protocol parseable by CI systems
- **No harness conflicts**: Works around ESP-IDF/libc incompatibilities with `cargo test`
- **Feature-gated**: Test code excluded from production builds

## Quick Start

```bash
# Run tests on host
cargo run --bin device-tests --no-default-features --features tap-tests \
    --target x86_64-apple-darwin

# Run tests in QEMU (plain ESP32)
source ~/export-esp.sh
cargo run --bin device-tests --features tap-tests \
    --target xtensa-esp32-espidf --release

# Run tests on hardware
cargo espflash flash --bin device-tests --features tap-tests --release --monitor
```

## Writing Tests

Tests use the `#[tap_test]` attribute, similar to `#[test]`:

```rust
#[cfg(feature = "tap-tests")]
mod tap_tests {
    use super::*;
    use reticulum_rs_esp32_macros::tap_test;

    #[tap_test]
    fn test_basic_functionality() {
        assert_eq!(2 + 2, 4);
    }

    #[tap_test]
    fn test_with_result() -> Result<(), String> {
        if some_condition() {
            Ok(())
        } else {
            Err("condition failed".to_string())
        }
    }

    #[tap_test(should_panic = "expected message")]
    fn test_panics_correctly() {
        panic!("expected message");
    }
}
```

### Key Points

1. **Always guard with feature flag**: Use `#[cfg(feature = "tap-tests")]` on the test module
2. **Use `tap_tests` module name**: Conventional but not required
3. **Import the macro**: `use reticulum_rs_esp32_macros::tap_test;`
4. **Return types**: Tests can return `()` or `Result<(), E>` where `E: Debug`
5. **Panic tests**: Use `#[tap_test(should_panic = "substring")]` for expected panics

## Architecture

### Components

```
macros/                      # Proc-macro crate
├── Cargo.toml              # [lib] proc-macro = true
└── src/lib.rs              # #[tap_test] attribute implementation

src/
├── testing.rs              # TAP test runner (feature-gated)
├── bin/device-tests.rs     # Test binary entry point
└── */mod.rs                # Modules with tap_tests submodules
```

### How It Works

1. **`#[tap_test]` macro** transforms test functions and registers them using `inventory::submit!`
2. **`inventory` crate** collects all registered tests at link time
3. **`device-tests` binary** calls `run_all_tests()` which iterates and executes all tests
4. **Output** is TAP format: `ok 1 - test_name` or `not ok 1 - test_name`

### Feature Flags

```toml
[features]
tap-tests = [
    "dep:reticulum-rs-esp32-macros",
    "dep:inventory",
]
```

When `tap-tests` is disabled:
- No test code is compiled
- `device-tests` binary cannot be built (requires feature)
- Production builds are smaller

## Test Coverage

| Module | Tests | Description |
|--------|-------|-------------|
| `ble/fragmentation.rs` | 22 | BLE packet fragmentation/reassembly |
| `lora/airtime.rs` | 15 | LoRa time-on-air calculations |
| `lora/duty_cycle.rs` | 10 | Token bucket duty cycle limiter |
| `wifi/config.rs` | 19 | WiFi credential validation |
| `testing.rs` | 12 | Test harness self-tests |
| **Total** | **78** | |

## Testing Environments

### Host Testing

Fastest iteration cycle. Tests run natively without ESP32 dependencies.

```bash
cargo run --bin device-tests --no-default-features --features tap-tests \
    --target x86_64-apple-darwin
```

**What can be tested:**
- Pure logic (validation, calculations, parsing)
- Data structures (serialization, fragmentation)
- Algorithms (airtime calculation, duty cycle)

**What cannot be tested:**
- ESP-IDF APIs (WiFi, BLE, NVS)
- Hardware peripherals

### QEMU Testing

Tests ESP32 code without real hardware. Uses plain ESP32 (not S3) due to QEMU stdout bug.

```bash
source ~/export-esp.sh
cargo run --bin device-tests --features tap-tests \
    --target xtensa-esp32-espidf --release
```

**What can be tested:**
- ESP-IDF initialization
- Memory allocation patterns
- Code that compiles for ESP32 but doesn't need real hardware

**What cannot be tested:**
- WiFi/BLE (no radio emulation)
- LoRa (no SX1262 emulation)
- Real-time behavior

### Hardware Testing

Full integration testing on actual ESP32-S3 device.

```bash
cargo espflash flash --bin device-tests --features tap-tests --release --monitor
```

**What can be tested:**
- Everything including WiFi, BLE, NVS
- Real-time behavior
- Actual LoRa transmission (with proper hardware)

## TAP Output Format

```
TAP version 14
1..78
ok 1 - test_basic_functionality
ok 2 - test_with_result
ok 3 - test_panics_correctly
not ok 4 - test_failure
  ---
  message: assertion failed: expected 5, got 4
  ---
# Passed: 77/78
```

TAP output can be parsed by:
- CI systems (Jenkins, GitLab CI)
- TAP consumers (`tap-parser`, `prove`)
- Log analysis tools

## Best Practices

### 1. Maximize Primarily Host-Testable Code, and Secondarily QEMU-Testable Code 

Separate logic from hardware:

```rust
// Good: Pure logic, host-testable
pub fn validate_ssid(ssid: &str) -> Result<(), ConfigError> {
    if ssid.is_empty() { return Err(ConfigError::Empty); }
    if ssid.len() > 32 { return Err(ConfigError::TooLong); }
    Ok(())
}

// Hardware wrapper, tested on device only
#[cfg(feature = "esp32")]
pub fn save_to_nvs(nvs: &mut EspNvs, ssid: &str) -> Result<(), EspError> {
    validate_ssid(ssid)?;
    nvs.set_str("ssid", ssid)
}
```

### 2. Use Feature Flags Consistently

```rust
// Platform-independent (always compiled)
pub mod config;

// Platform-dependent (ESP32 only)
#[cfg(feature = "esp32")]
pub mod storage;
```

### 3. Test Module Structure

```rust
// At the bottom of each source file
#[cfg(feature = "tap-tests")]
mod tap_tests {
    use super::*;
    use reticulum_rs_esp32_macros::tap_test;

    #[tap_test]
    fn test_something() {
        // ...
    }
}
```

## Known Limitations

### Why Not Standard `cargo test`?

Standard `cargo test` doesn't work with ESP-IDF targets due to:
- Test harness assumes host execution model
- libc incompatibilities ([rust-lang/rust#125714](https://github.com/rust-lang/rust/issues/125714))
- No way to run tests on device with result collection

### QEMU Limitations

- ESP32-S3 machine has stdout bug (use plain ESP32)
- No WiFi/BLE/LoRa radio emulation
- Some peripherals not emulated
- Slower than real hardware (~4-5x)

## References

- [TAP Specification](https://testanything.org/tap-version-14-specification.html)
- [inventory crate](https://crates.io/crates/inventory)
- [ESP-IDF Programming Guide](https://docs.espressif.com/projects/esp-idf/en/latest/esp32s3/)

---

*Updated 2026-01-12: Migrated to unified TAP testing system*
