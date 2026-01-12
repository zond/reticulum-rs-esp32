# TAP Testing System TODOs

Code review findings from rust-code-guardian for future improvement.

## Important Improvements

### 1. Warning for zero tests
**Location:** `src/testing.rs:89`

Add a warning if `test_count()` returns 0:
```rust
if count == 0 {
    eprintln!("WARNING: No tests registered. Did you forget #[tap_test]?");
}
```

### 2. Better error handling in proc-macro
**Location:** `macros/src/lib.rs:136-148`

Invalid `should_panic` syntax (e.g., `should_panic = 42`) silently falls back to "any panic is ok". Should emit a compile error instead.

### 3. Fragment lookup limitation
**Location:** `src/ble/fragmentation.rs:461-471`

`find_key_for_fragment()` does linear search and may match wrong reassembly when multiple sources send concurrently. Document limitation and add source address tracking when BLE layer provides it.

## Suggestions for Excellence

### 4. Add timeout support for hung tests
On embedded hardware, hung tests block forever. Consider adding `run_with_timeout()` method using threads and channels.

### 5. Add `#[inline]` to hot path methods
Fragment flag checking methods (`is_first()`, `has_more()`, `has_valid_flags()`) are called frequently - could benefit from inline hints.

### 6. Consider `thiserror` for error handling
`ConfigError` in wifi/config.rs has manual `Display` impl. `thiserror` crate would reduce boilerplate.

### 7. Bounds checking in airtime calculation
**Location:** `src/lora/airtime.rs:125`

Extreme inputs could produce `NaN`/`Infinity` in float calculations. Add saturation before casting to `u64`.

---

*Created 2026-01-12 from rust-code-guardian review*
