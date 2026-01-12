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

## Optimizations (if needed)

### Flash Reduction
- LTO (Link-Time Optimization): 5-15% savings
- Remove unused tokio features: 20-30 KB
- Already using `opt-level = "z"`

### RAM Reduction
- Reduce packet cache size
- Use static buffers
- Lazy initialization
- Limit concurrent connections

## Recommendations

1. **Phase 1** (current): Comfortable fit
2. **Profile on-device** after adding LoRa interface
3. **Use PSRAM** for large buffers if SRAM pressure appears
4. **Enable LTO** if binary exceeds 2 MB

## Comparison

| Project | Flash | SRAM |
|---------|-------|------|
| Our reticulum-rs | 928 KB+ | ~170 KB+ |
| microReticulum (C++) | 200-400 KB | 50-150 KB |
| RNode Firmware (C) | 500-800 KB | 100-200 KB |

Rust is larger due to async runtime but provides memory safety.
