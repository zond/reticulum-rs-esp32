# Research Findings

Initial research conducted January 2026 to understand project feasibility and identify blockers.

## reticulum-rs Analysis

**Upstream**: https://github.com/BeechatNetworkSystemsLtd/Reticulum-rs

**Fork with ESP32 patches**: https://github.com/zond/Reticulum-rs
- `esp32-compat` branch - **combined branch for this project** (both patches below)
- `tokio-features` branch - specific tokio features (no signal handling)
- `optional-grpc` branch - makes tonic/prost optional

### Dependencies (from Cargo.toml)

| Category | Crates |
|----------|--------|
| **Async Runtime** | tokio 1.44.2, tokio-stream, tokio-util |
| **gRPC** | tonic 0.13.0, prost 0.13.5 (optional via `grpc` feature) |
| **Crypto** | x25519-dalek 2.0.1, ed25519-dalek 2.1.1, aes 0.8.4, cbc 0.1.2, hkdf 0.12.4, hmac 0.12.1, sha2 0.10.8 |
| **Serialization** | rmp 0.8.14 (msgpack), serde 1.0.219 |
| **Other** | rand_core 0.6.4, log 0.4.27, env_logger 0.10 |

### Features (after PR #56)

- `default = ["alloc", "grpc"]`
- `alloc` (empty)
- `fernet-aes128` (empty)
- `grpc` - enables tonic/prost for Kaonic interface

### Key Observations

1. **No no_std support** - but this is irrelevant for ESP-IDF which provides std
2. **tokio features** - only needs: rt, net, io-util, sync, time, macros (not "full")
3. **tonic/prost** - Kaonic gRPC interface only, now optional via `grpc` feature
4. **RustCrypto stack** - all these crates support no_std if needed

### Resolved Questions

- **Is tonic required?** No - now optional via `grpc` feature ([PR #56](https://github.com/BeechatNetworkSystemsLtd/Reticulum-rs/pull/56))
- **Does it require rt-multi-thread?** No - single-threaded runtime works fine
- **Signal handling blocker?** Resolved by using specific tokio features ([PR #55](https://github.com/BeechatNetworkSystemsLtd/Reticulum-rs/pull/55))

## ESP32 Rust Ecosystem

### esp-idf-sys / esp-idf-svc / esp-idf-hal

**Key insight**: ESP-IDF provides a **full std environment**, not bare metal.

Provides:
- pthreads
- TCP/IP stack (lwIP)
- BSD sockets
- WiFi, BLE drivers
- File system APIs

**Source**: https://github.com/esp-rs/esp-idf-sys

### Tokio on ESP32

**Status**: Works with workarounds.

Required configuration (`.cargo/config.toml`):
```toml
[build]
rustflags = ["--cfg", "mio_unsupported_force_poll_poll"]
```

**Why**: ESP-IDF's libc has poll/select but not epoll/kqueue. The `mio_unsupported_force_poll_poll` flag forces mio to use poll.

**Limitations**:
- `rt-multi-thread` feature doesn't compile on ESP32 yet
- Memory constrained: ~5 concurrent connections before OOM on ESP32-S3
- Some frameworks (Rocket, Actix-web) require rt-multi-thread and won't work

**Working examples**:
- https://github.com/jasta/esp32-tokio-demo
- https://github.com/aedm/esp32-s3-rust-axum-example

### LoRa Driver (SX1262)

**Crate**: https://crates.io/crates/sx1262 (v0.3.0)

- embedded-hal compatible driver for SX1261/2
- Supports LoRa and FSK modulation
- Works with embedded-hal 1.0
- Actively maintained (updated January 2026)

### BLE Support

**Primary option**: esp32-nimble crate (NimBLE stack wrapper)

**Note**: BLE Mesh support in Rust is limited. ESP-IDF has ESP-BLE-MESH in C, but no mature Rust wrapper exists. Custom implementation will be needed, inspired by https://github.com/torlando-tech/ble-reticulum.

**Alternative**: https://github.com/pyaillet/esp-idf-ble (less active, maintainer recommends esp-idf-svc)

### Crypto Libraries (RustCrypto)

All support no_std with `default-features = false`:

| Crate | no_std | Notes |
|-------|--------|-------|
| ed25519-dalek | ✅ | Batch verification requires alloc |
| x25519-dalek | ✅ | Uses curve25519-dalek |
| hkdf | ✅ | |
| hmac | ✅ | |
| sha2 | ✅ | |
| aes | ✅ | |

## Alternative Reticulum Implementations

### microReticulum (C++)

**Repository**: https://github.com/attermann/microReticulum

**Status**: WIP but functional. Most mature embedded implementation.

**Implemented**:
- Identity, Destination, Packet abstractions
- Crypto (Ed25519, X25519, AES, HKDF, HMAC, PKCS7, Fernet)
- Announcements, Transport, Path finding
- Links, UDP interfaces
- Data persistence

**Not implemented**: Ratchets, Resources, Channels/Buffers

**Memory**: ~50-150KB RAM with active connections

**Useful as**: Protocol reference, performance baseline

### ESP32-C3-Reticulum-Node (C++)

**Repository**: https://github.com/AkitaEngineering/ESP32-C3-Reticulum-Node

**Interfaces**: WiFi UDP, ESP-NOW, Serial, Bluetooth Classic SPP, LoRa, HAM Modem, IPFS

**Note**: No BLE mesh support (only Bluetooth Classic)

## Hardware: LILYGO T3-S3

- **MCU**: ESP32-S3 (Xtensa dual-core, 240MHz)
- **RAM**: 512KB SRAM + 8MB PSRAM (depending on variant)
- **Flash**: 4-16MB
- **LoRa**: SX1262 (868/915 MHz)
- **Connectivity**: WiFi 802.11 b/g/n, BLE 5.0
- **Target triple**: `xtensa-esp32s3-espidf`

## Build Setup Requirements

### Prerequisites

1. Rust via rustup
2. espup: `cargo install espup && espup install`
3. Source environment: `source $HOME/export-esp.sh`
4. cargo-espflash: `cargo install cargo-espflash`

### Project Generation

```bash
cargo generate esp-rs/esp-idf-template cargo
```

### Cargo Config (.cargo/config.toml)

```toml
[build]
target = "xtensa-esp32s3-espidf"
rustflags = [
    "--cfg", "mio_unsupported_force_poll_poll",
    "-C", "default-linker-libraries",
]

[target.xtensa-esp32s3-espidf]
linker = "ldproxy"
runner = "espflash flash --monitor"

[unstable]
build-std = ["std", "panic_abort"]

[env]
ESP_IDF_VERSION = "v5.2"
```

## Compilation Issues Encountered

### signal-hook-registry errors

**Problem**: tokio "full" feature pulls in signal-hook-registry which uses Unix signal APIs not available in ESP-IDF:
- `siginfo_t` type not found
- `SIGKILL`, `SIGSTOP` constants missing
- `SA_RESTART`, `SA_SIGINFO` flags missing

**Root cause**: tokio "full" includes signal handling that ESP-IDF's libc doesn't support.

**Solution**: Changed tokio dependency to use specific features instead of "full":
```toml
tokio = { version = "1.44.2", features = ["rt", "net", "io-util", "sync", "time", "macros"] }
```

This is implemented in [PR #55](https://github.com/BeechatNetworkSystemsLtd/Reticulum-rs/pull/55).

## Risk Assessment (Updated)

| Risk | Severity | Status |
|------|----------|--------|
| reticulum-rs won't compile for ESP32 | High | **RESOLVED** - Compiles with forked patches |
| tonic dependency too heavy | Medium | **RESOLVED** - Now optional via `grpc` feature |
| Memory exhaustion | Medium | **MITIGATED** - See [memory-analysis.md](memory-analysis.md), limits in place (MAX_CONCURRENT_LINKS, MAX_QUEUED_MESSAGES_PER_DEST) |
| BLE mesh complexity | Medium | **IN PROGRESS** - BLE fragmentation complete, mesh interface pending. See [future-work.md](future-work.md) |
| tokio rt-multi-thread required | High | **RESOLVED** - Not required |

## Progress

### Completed

1. Set up ESP-IDF Rust project scaffold
2. Built reticulum-rs as dependency for ESP32-S3 target
3. Identified compilation errors (signal-hook-registry)
4. Made tonic optional via `grpc` feature
5. Created upstream PRs for both fixes

### Next Steps

1. Flash firmware to device and test basic operation
2. Implement LoRa interface using SX1262 driver
3. Implement WiFi interface to connect to testnet
4. Profile memory usage on device
5. Implement BLE mesh (custom, inspired by ble-reticulum)

## References

- [Reticulum Manual](https://markqvist.github.io/Reticulum/manual/)
- [esp-rs Book](https://esp-rs.github.io/book/)
- [ESP-IDF Programming Guide](https://docs.espressif.com/projects/esp-idf/en/latest/esp32s3/)
- [Tokio on ESP32 blog post](https://coder0xff.wixsite.com/community/post/tokio-on-esp32)
