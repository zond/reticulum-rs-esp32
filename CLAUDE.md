# Agent Instructions

## Development Workflow

### Before Every Commit

1. Run tests: `cargo test`
2. Run linting: `cargo clippy -- -D warnings`
3. Format code: `cargo fmt`
4. Ensure README.md and CLAUDE.md are up to date.
5. Check if any files in the docs directory need update.
6. Review with the code-simplifier agent
7. Review with the rust-code-guardian agent

### Commit Standards

- Keep commits reasonably small and focused
- Each commit should be well-tested
- Write clear, descriptive commit messages
- Prefer many small commits over large monolithic ones

## Build Commands

```bash
# Build for ESP32-S3
cargo build --release --target xtensa-esp32s3-espidf

# Flash to device
cargo espflash flash --release --monitor

# Run tests (host)
cargo test

# Lint
cargo clippy -- -D warnings

# Format
cargo fmt
```

## Project Context

This is ESP32-S3 embedded firmware using esp-idf-sys. The target is a LILYGO T3-S3 board with SX1262 LoRa.

Key constraints:
- 512KB SRAM - be mindful of allocations
- ESP-IDF provides std environment (not bare metal no_std)
- BLE mesh is not upstream in reticulum-rs - needs custom implementation

## Upstream Status

Using forked reticulum-rs with ESP32 compatibility patches:
- [PR #55](https://github.com/BeechatNetworkSystemsLtd/Reticulum-rs/pull/55) - Use specific tokio features (no signal handling)
- [PR #56](https://github.com/BeechatNetworkSystemsLtd/Reticulum-rs/pull/56) - Make gRPC optional

Once merged, switch back to upstream.
