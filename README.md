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

## Build

```bash
# Build for hardware (ESP32-S3, default)
cargo build --release

# Flash to device
cargo espflash flash --release --monitor

# Run tests on host (TAP format output)
# Use your host triple: x86_64-apple-darwin (macOS), x86_64-unknown-linux-gnu (Linux)
cargo run --bin device-tests --no-default-features --features tap-tests \
    --target x86_64-apple-darwin

# Lint and format
cargo clippy --no-default-features --target x86_64-apple-darwin -- -D warnings
cargo fmt
```

## Build Targets

| Target | Use Case | Notes |
|--------|----------|-------|
| xtensa-esp32s3-espidf | Hardware (LILYGO T3-S3) | Default target |
| xtensa-esp32-espidf | QEMU testing | ESP32-S3 QEMU has stdout bug |
| Host native | Host tests | Use `--no-default-features --target <triple>` |

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
- Memory constraints on ESP32-S3 (512KB SRAM)
- Power management for battery operation

## Documentation

- [Reticulum Manual](https://markqvist.github.io/Reticulum/manual/)
- [ESP-IDF Programming Guide](https://docs.espressif.com/projects/esp-idf/en/latest/esp32s3/)
- [esp-rs Book](https://esp-rs.github.io/book/)
- [SX1262 Datasheet](https://www.semtech.com/products/wireless-rf/lora-connect/sx1262)

## License

MIT
