# Reticulum-rs ESP32 Firmware

Firmware for the LILYGO T3-S3 ESP32-S3 LoRa board that implements a Reticulum transport node. The device meshes with both LoRa and BLE devices while using WiFi to connect to the Reticulum internet testnet.

## Target Hardware

- **Board**: [LILYGO T3-S3](https://www.amazon.se/dp/B09FXHSS6P) (ESP32-S3 with SX1262 LoRa)
- **Capabilities**: WiFi, BLE, LoRa (868/915 MHz)

## Goals

- Mesh with other LoRa devices using the Reticulum protocol
- Mesh with BLE devices (custom implementation inspired by ble-reticulum)
- Connect to the Reticulum internet testnet via WiFi
- Provide a simple HTTP/JSON stats endpoint for monitoring

## Dependencies

Rust crate dependencies are managed by Cargo and downloaded automatically on first build:
- [Reticulum-rs](https://github.com/BeechatNetworkSystemsLtd/Reticulum-rs) - Rust Reticulum implementation
  - Using [fork](https://github.com/zond/Reticulum-rs) with ESP32 patches ([PR #55](https://github.com/BeechatNetworkSystemsLtd/Reticulum-rs/pull/55), [PR #56](https://github.com/BeechatNetworkSystemsLtd/Reticulum-rs/pull/56))
- [esp-idf-sys](https://github.com/esp-rs/esp-idf-sys) - Rust bindings to ESP-IDF

## Prerequisites

### 1. Install Rust and ESP Toolchain

```bash
# Install espup (ESP toolchain manager) and espflash (flashing tool)
cargo install espup cargo-espflash

# Install ESP Rust toolchain (Xtensa LLVM fork, rust-src, etc.)
espup install
```

**What this installs:**
- Xtensa Rust toolchain → `~/.rustup/toolchains/esp/`
- Environment script → `~/export-esp.sh`

### 2. Source ESP Environment

Required before each build session (or add to shell profile):
```bash
source ~/export-esp.sh
```

### 3. First Build (Downloads ESP-IDF)

The first `cargo build` downloads and compiles ESP-IDF v5.2:
```bash
cargo build --release   # Takes 5-15 minutes, ~2GB disk space
```

**What this installs:**
- ESP-IDF framework → `.embuild/espressif/esp-idf/v5.2/`
- ESP-IDF tools → `~/.espressif/tools/`

### 4. QEMU (Optional)

For development without hardware, see [docs/qemu-setup.md](docs/qemu-setup.md).

### Installed Tools Summary

These paths apply when following the default installation instructions above:

| Tool | Location | Installed by |
|------|----------|--------------|
| ESP Rust toolchain | `~/.rustup/toolchains/esp/` | `espup install` (step 1) |
| ESP environment script | `~/export-esp.sh` | `espup install` (step 1) |
| ESP-IDF framework | `.embuild/espressif/esp-idf/v5.2/` | First `cargo build` (step 3) |
| ESP-IDF tools | `~/.espressif/tools/` | First `cargo build` (step 3) |
| QEMU (if installed) | `~/.espressif/tools/qemu-xtensa/.../qemu/bin/` | [docs/qemu-setup.md](docs/qemu-setup.md) |

## Cargo Aliases

This project provides cargo aliases for common operations (defined in `.cargo/config.toml`):

### Building

```bash
cargo build-esp32   # Build release for hardware (ESP32-S3)
cargo build-qemu    # Build release for QEMU emulation (plain ESP32)
```

### Flashing

```bash
cargo flash-esp32   # Build and flash to connected device
cargo flash-qemu    # Build and run in QEMU emulator
```

### Testing

```bash
cargo test          # Run tests on host (fastest iteration)
cargo test-qemu     # Run tests in QEMU (ESP32 emulation)
```

For test architecture details, see [docs/testing-strategy.md](docs/testing-strategy.md).

### Development

```bash
cargo clippy -- -D warnings   # Lint (host target for faster checks)
cargo fmt                     # Format code
```

### Region Selection

Build for a different LoRa region (default is EU868):

```bash
cargo build-esp32 --features region-us915   # US 902-928 MHz
cargo build-esp32 --features region-au915   # Australia 915-928 MHz
cargo build-esp32 --features region-as923   # Asia 920-923 MHz
```

### Summary Table

| Command | Description |
|---------|-------------|
| `cargo test` | Run host tests (fastest) |
| `cargo test-qemu` | Run tests in QEMU |
| `cargo build-esp32` | Build release firmware |
| `cargo build-qemu` | Build for QEMU |
| `cargo flash-esp32` | Build and flash to device |
| `cargo flash-qemu` | Build and run in QEMU |

For build configuration details, see [docs/research-findings.md](docs/research-findings.md).

## QEMU Emulation

For development without hardware. Uses plain ESP32 target (ESP32-S3 has a QEMU stdout bug). See [docs/qemu-setup.md](docs/qemu-setup.md) for full setup instructions.

```bash
# Build for QEMU (plain ESP32 with UART console)
cargo build-qemu
```

## Architecture

### Transport Layers

1. **LoRa**: Direct integration with reticulum-rs using SX1262 driver
2. **WiFi**: TCP/UDP interfaces to connect to internet testnet
3. **BLE**: Custom mesh implementation (reticulum-rs doesn't natively support BLE mesh)

### Challenges

- BLE mesh requires custom implementation compatible with reticulum-rs
- Memory constraints on ESP32-S3 (see below)
- Power management for battery operation

## Memory Constraints

The ESP32-S3 has limited SRAM (512KB), requiring careful memory management:

| Resource | Available | Projected Usage | Margin |
|----------|-----------|-----------------|--------|
| Flash | 3.3 MB | ~1.6 MB | 52% free |
| SRAM | 512 KB | ~452 KB | 12% free |
| PSRAM | 2 MB | ~256 KB | 87% free |

Key limits enforced in code:
- `MAX_CONCURRENT_LINKS = 20` - Each link holds crypto state and buffers
- `MAX_QUEUED_MESSAGES_PER_DEST = 5` - Pending messages per destination
- `MAX_KNOWN_DESTINATIONS = 100` - Cached announce destinations

For detailed analysis, see [docs/memory-analysis.md](docs/memory-analysis.md).

## Documentation

- [Reticulum Manual](https://markqvist.github.io/Reticulum/manual/)
- [ESP-IDF Programming Guide](https://docs.espressif.com/projects/esp-idf/en/latest/esp32s3/)
- [esp-rs Book](https://esp-rs.github.io/book/)
- [SX1262 Datasheet](https://www.semtech.com/products/wireless-rf/lora-connect/sx1262)

## License

MIT
