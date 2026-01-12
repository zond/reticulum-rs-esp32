# Testing System TODOs

Code review findings and future improvements for the testing system.

## Completed

- [x] Unified `#[esp32_test]` macro for host and ESP32 tests
- [x] `cargo test-qemu` command for automated QEMU testing
- [x] Fragment lookup uses source address for disambiguation (`src/ble/fragmentation.rs`)
- [x] Add `#[inline]` to hot path methods (`src/ble/fragmentation.rs`)
- [x] Bounds checking in airtime calculation (`src/lora/airtime.rs`)
- [x] `Fragmenter::try_new()` fallible constructor (avoids `#[should_panic]`)
- [x] `saturating_duration_since()` for QEMU clock jitter tolerance

## Future Improvements

### 1. Test timeout in QEMU runner
The QEMU test runner has a global timeout, but individual test timeouts would be useful for identifying hung tests.

### 2. Consider `thiserror` for error handling
`ConfigError` in wifi/config.rs has manual `Display` impl. `thiserror` crate would reduce boilerplate.

### 3. Test filtering support
Pass test name filters through `cargo test-qemu` to the test harness.

---

*Created 2026-01-12 from rust-code-guardian review*
*Updated 2026-01-12: Simplified to `#[esp32_test]` macro system*
