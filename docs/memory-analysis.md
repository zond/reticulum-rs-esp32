# Memory and Binary Size Analysis

Analysis of reticulum-rs-esp32 firmware fit on ESP32-S3 (LILYGO T3-S3).

## Summary

**The firmware WILL FIT** on the LILYGO T3-S3 with comfortable margins.

| Resource | Current | Projected Full | Available | Margin |
|----------|---------|----------------|-----------|--------|
| Flash | 928 KB | ~1.6 MB | 3.3 MB | 52% free |
| SRAM | ~170 KB | ~452 KB | 512 KB | 12% free |
| PSRAM | 0 KB | ~256 KB | 2 MB | 87% free |

## Hardware Specifications (LILYGO T3-S3)

| Component | Size | Notes |
|-----------|------|-------|
| Flash (SPI) | 4 MB | ~3.3 MB usable after bootloader |
| SRAM (Internal) | 512 KB | Fast, on-chip |
| PSRAM (QSPI) | 2 MB | Slower, for large buffers |
| MCU | ESP32-S3 | Dual-core Xtensa, 240 MHz |

## Current Binary Breakdown

The minimal firmware (928 KB) contains:

| Component | Estimated Size |
|-----------|----------------|
| Reticulum-rs Core | 250-300 KB |
| Tokio Runtime | 150-200 KB |
| Cryptography (X25519, Ed25519, AES) | 100-120 KB |
| ESP-IDF Rust Bindings | 150-180 KB |
| esp-idf-hal/-svc | 80-100 KB |
| Our Code (LoRa + Duty Cycle) | ~5 KB |
| Standard Library | 80-100 KB |

## Projected Additions

### LoRa Interface (~100 KB flash, ~12 KB RAM)

| Component | Flash | RAM |
|-----------|-------|-----|
| sx126x crate | 40-50 KB | 4-8 KB |
| Our LoRa implementation | 30-50 KB | 1 KB |
| Duty cycle limiter | 5 KB | 32 bytes |

### BLE Interface (~150 KB flash, ~25 KB RAM)

| Component | Flash | RAM |
|-----------|-------|-----|
| esp32-nimble | 60-80 KB | 15-20 KB |
| GATT services (mesh + WiFi config) | 25-35 KB | 2-3 KB |
| Fragmentation layer | 5-10 KB | 1 KB |

### WiFi + HTTP Stats (~30 KB flash, ~8 KB RAM)

| Component | Flash | RAM |
|-----------|-------|-----|
| WiFi connection manager | 10-15 KB | 2 KB |
| HTTP server (esp-idf-svc) | 15-20 KB | 4-6 KB |
| JSON stats serialization | 5 KB | 1 KB |

## Runtime Memory (SRAM)

Worst-case peak usage with all features active:

| Component | Memory |
|-----------|--------|
| ESP-IDF + WiFi (system) | ~200 KB |
| Reticulum-rs (active) | ~160 KB |
| Tokio Runtime + Tasks | ~35 KB |
| LoRa Interface | ~12 KB |
| BLE Interface (mesh + WiFi config) | ~27 KB |
| HTTP Stats Server | ~6 KB |
| Crypto Operations | ~4 KB |
| Packet buffers | ~8 KB |
| **Total Peak** | **~452 KB** |

**Remaining:** ~60 KB (12% margin)

## PSRAM Strategy

If SRAM becomes constrained, move these to PSRAM:

| Buffer | Size |
|--------|------|
| Packet RX ring | 256 KB |
| Packet TX queue | 256 KB |
| Announce cache | 128 KB |
| Path table cache | 128 KB |

## Duty Cycle Budget

At 1% EU duty cycle (36 seconds per hour) with default LoRa settings (SF7, 125 kHz):

| Packet Size | Airtime | Packets/Hour |
|-------------|---------|--------------|
| 50 bytes | 78 ms | ~460 |
| 100 bytes | 128 ms | ~280 |
| 500 bytes | 614 ms | ~59 |

Ample budget for a transport node.

## Future Routing Memory Budget

When implementing [scalable-routing-proposal.md](scalable-routing-proposal.md), additional memory will be needed:

| Feature | Flash | RAM | Notes |
|---------|-------|-----|-------|
| Gateway announce filtering | ~5 KB | ~1 KB | Simple hash set |
| DHT routing (Kademlia) | ~30-50 KB | ~20-40 KB | k-buckets, RPCs |
| Expanded announce cache | ~5 KB | ~10-50 KB | Depends on network size |
| Path table scaling | ~5 KB | ~10-30 KB | More destinations |

This is why optimization headroom matters.

## Optimizations (if needed)

### Rust Compiler Settings (Cargo.toml)

Current `profile.release`:
```toml
opt-level = "s"  # Size-optimized
```

Full size optimization (apply when needed):
```toml
[profile.release]
opt-level = "z"     # Maximum size reduction (currently "s")
lto = true          # Link-Time Optimization: 5-15% savings
codegen-units = 1   # Better optimization, slower builds
strip = true        # Remove symbols (may already be handled by ESP-IDF)
```

We already use `build-std = ["std", "panic_abort"]` in `.cargo/config.toml` which eliminates unwinding code.

Additional nightly options (if needed):
```bash
# Remove location details from panics
RUSTFLAGS="-Zlocation-detail=none"

# Build std with size optimization
-Z build-std-features="optimize_for_size"
```

### ESP-IDF sdkconfig Optimizations

Add to `config/sdkconfig.defaults` when flash gets tight:

**High impact (20-50 KB each):**
```
# Use Newlib Nano printf (25-50 KB savings)
CONFIG_NEWLIB_NANO_FORMAT=y

# Disable IPv6 if not needed (~30 KB)
CONFIG_LWIP_IPV6=n

# Reduce log level (removes strings)
CONFIG_LOG_DEFAULT_LEVEL_WARN=y
```

**Medium impact (5-20 KB each):**
```
# Disable WPA3 if WPA2 sufficient
CONFIG_ESP_WIFI_ENABLE_WPA3_SAE=n

# Disable soft-AP if not needed
CONFIG_ESP_WIFI_SOFTAP_SUPPORT=n

# Silent assertions (removes strings)
CONFIG_COMPILER_OPTIMIZATION_ASSERTION_LEVEL=0

# Disable error name lookup table
CONFIG_ESP_ERR_TO_NAME_LOOKUP=n
```

**BLE optimizations:**
```
# Single BLE connection (we control the protocol)
CONFIG_BT_NIMBLE_MAX_CONNECTIONS=1
CONFIG_BTDM_CTRL_BLE_MAX_CONN=1

# Disable unused roles
CONFIG_BT_NIMBLE_ROLE_CENTRAL=n
CONFIG_BT_NIMBLE_ROLE_OBSERVER=n
```

### Reticulum-rs Fork Optimizations

Our fork (`esp32-compat` branch) already disables gRPC. Additional options:

| Change | Savings | Status |
|--------|---------|--------|
| Disable gRPC (tonic/prost) | ~50-80 KB | Done |
| Make `env_logger` optional | ~15 KB | TODO in fork |
| Make `serde` optional | ~50 KB | Requires refactoring |
| Reduce tokio features | ~20-30 KB | Investigate |

### RAM Reduction

- Move large buffers to PSRAM (see PSRAM Strategy above)
- Reduce packet cache size
- Use static buffers where possible
- Lazy initialization of subsystems
- Limit concurrent connections

### Analysis Tools

```bash
# Measure binary size breakdown
idf.py size
idf.py size-components
idf.py size-files

# Rust-specific analysis (run on host build)
cargo bloat --release        # Find large functions
cargo llvm-lines             # Generic instantiation bloat
cargo unused-features        # Find unused feature flags
```

## Recommendations

1. **Phase 1** (current): Comfortable fit with 52% flash margin
2. **Profile on-device** after adding LoRa interface
3. **Use PSRAM** for large buffers if SRAM pressure appears
4. **Apply Cargo.toml optimizations** (LTO, codegen-units=1) when approaching 2 MB
5. **Apply sdkconfig optimizations** based on actual feature needs
6. **Reserve headroom** for DHT routing (~50 KB flash, ~40 KB RAM)

## Comparison

| Project | Flash | SRAM |
|---------|-------|------|
| Our reticulum-rs | 928 KB+ | ~170 KB+ |
| microReticulum (C++) | 200-400 KB | 50-150 KB |
| RNode Firmware (C) | 500-800 KB | 100-200 KB |

Rust is larger due to async runtime but provides memory safety.
