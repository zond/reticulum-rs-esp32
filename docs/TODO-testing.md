# TAP Testing System TODOs

Code review findings from rust-code-guardian for future improvement.

## Completed

- [x] Warning for zero tests (`src/testing.rs`)
- [x] Better error handling in proc-macro (`macros/src/lib.rs`)
- [x] Fragment lookup uses source address for disambiguation (`src/ble/fragmentation.rs`)
- [x] Add `#[inline]` to hot path methods (`src/ble/fragmentation.rs`)
- [x] Bounds checking in airtime calculation (`src/lora/airtime.rs`)

## Future Improvements

### 1. Add timeout support for hung tests
On embedded hardware, hung tests block forever. Consider adding `run_with_timeout()` method using threads and channels.

### 2. Consider `thiserror` for error handling
`ConfigError` in wifi/config.rs has manual `Display` impl. `thiserror` crate would reduce boilerplate.

---

*Created 2026-01-12 from rust-code-guardian review*
*Updated 2026-01-12 with fixes*
