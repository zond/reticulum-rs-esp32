# QEMU Emulator Setup

## Current Status (2026-01-12)

**Status:** QEMU 9.2.2 with ESP32 support works fully. ESP32-S3 has a known stdout bug.

### Recommendation

Use **plain ESP32** target for QEMU testing. The ESP32-S3 QEMU emulation has a bug where application stdout doesn't appear after the bootloader completes.

| Target | QEMU Machine | Status |
|--------|--------------|--------|
| xtensa-esp32-espidf | esp32 | Works fully |
| xtensa-esp32s3-espidf | esp32s3 | Bootloader only (stdout bug) |

### What's Working (ESP32)
- Full bootloader output
- Application println! output
- log crate integration via esp_idf_svc::log
- Heartbeat loop runs correctly
- All console output visible

---

## Quick Start (Plain ESP32 for QEMU)

### Installation

Choose your platform:

#### macOS (x86_64)
```bash
# Install dependencies
brew install libgcrypt glib pixman sdl2 libslirp

# Download QEMU 9.2.2 with ESP32 support
curl -LO https://github.com/espressif/qemu/releases/download/esp-develop-9.2.2-20250228/qemu-xtensa-softmmu-esp_develop_9.2.2_20250228-x86_64-apple-darwin.tar.xz

# Extract to standard ESP tools location
mkdir -p ~/.espressif/tools/qemu-xtensa/esp_develop_9.2.2_20250228
tar -xf qemu-xtensa-softmmu-esp_develop_9.2.2_20250228-x86_64-apple-darwin.tar.xz \
    -C ~/.espressif/tools/qemu-xtensa/esp_develop_9.2.2_20250228
```

#### Linux (x86_64)
```bash
# Install dependencies (Debian/Ubuntu)
sudo apt-get install libgcrypt20 libglib2.0-0 libpixman-1-0 libsdl2-2.0-0 libslirp0

# Download QEMU 9.2.2 with ESP32 support
curl -LO https://github.com/espressif/qemu/releases/download/esp-develop-9.2.2-20250228/qemu-xtensa-softmmu-esp_develop_9.2.2_20250228-x86_64-linux-gnu.tar.xz

# Extract to standard ESP tools location
mkdir -p ~/.espressif/tools/qemu-xtensa/esp_develop_9.2.2_20250228
tar -xf qemu-xtensa-softmmu-esp_develop_9.2.2_20250228-x86_64-linux-gnu.tar.xz \
    -C ~/.espressif/tools/qemu-xtensa/esp_develop_9.2.2_20250228
```

**After installation, QEMU binary is at:**
```
~/.espressif/tools/qemu-xtensa/esp_develop_9.2.2_20250228/qemu/bin/qemu-system-xtensa
```

### Run Tests in QEMU
```bash
# Run all tests in QEMU (builds, creates firmware, launches QEMU automatically)
source ~/export-esp.sh
cargo test-qemu
```

The `cargo test-qemu` command automatically:
1. Builds tests for ESP32 target
2. Creates merged firmware image with `espflash save-image`
3. Launches QEMU with the firmware
4. Parses test output and exits with appropriate status code

---

## Build Configuration

The project supports multiple targets:

| Target | Use Case | Command |
|--------|----------|---------|
| Host | Host tests (fastest) | `cargo test` |
| ESP32 | QEMU tests | `cargo test-qemu` |
| ESP32-S3 | Release firmware | `cargo build-esp32` |

The `test-qemu` alias (defined in `.cargo/config.toml`) runs the QEMU test runner which builds for plain ESP32.

Configuration files in `config/`:
- `sdkconfig.defaults` - Common settings (BLE, WiFi, stack sizes)
- `sdkconfig.qemu` - QEMU overrides (disables USB_SERIAL_JTAG)

---

## ESP32-S3 QEMU Bug (Reference)

The ESP32-S3 QEMU machine has a known bug where application stdout doesn't appear after the bootloader. Boot messages up through `spi_flash: flash io: dio` are visible, but Rust application output is not.

**Attempted fixes that did NOT work:**
- Configured `CONFIG_ESP_CONSOLE_SECONDARY_NONE=y` to disable USB_SERIAL_JTAG
- Tried `-serial mon:stdio` QEMU flag
- Verified `CONFIG_LOG_DEFAULT_LEVEL_INFO=y` in sdkconfig
- Simplified main.rs to match esp-idf-sys example pattern
- Used `esp_rom_printf` for direct ROM output

**Workaround:** Use plain ESP32 target for QEMU testing.

---

## QEMU Command Reference

```bash
# Basic run with serial output (ESP32)
qemu-system-xtensa -machine esp32 -nographic \
    -drive file=target/firmware-esp32.bin,if=mtd,format=raw

# With explicit serial config
qemu-system-xtensa -machine esp32 -nographic -serial mon:stdio \
    -drive file=target/firmware-esp32.bin,if=mtd,format=raw

# With GDB server for debugging
qemu-system-xtensa -machine esp32 -nographic \
    -drive file=target/firmware-esp32.bin,if=mtd,format=raw -s -S

# Connect GDB
xtensa-esp32-elf-gdb target/xtensa-esp32-espidf/release/reticulum-rs-esp32 \
    -ex "target remote :1234"
```

---

## Known Limitations

- **ESP32-S3 stdout bug** - Use plain ESP32 for QEMU testing
- **QEMU emulation is slower** - ~4-5x slower than real hardware
- **BLE not emulated** - NimBLE stack loads but no radio
- **WiFi not emulated** - ESP-IDF WiFi driver loads but no radio
- **Some peripherals missing** - GPIO, SPI work; USB_SERIAL_JTAG doesn't
- **No LoRa** - SX1262 peripheral not emulated

## Hardware Differences

Plain ESP32 lacks some features of ESP32-S3:
- No USB OTG (ESP32-S3 has built-in USB)
- Smaller SRAM (520KB vs 512KB usable)
- Older dual-core Xtensa LX6 vs LX7

For firmware testing, these differences are usually not significant. Hardware-specific features (LoRa, USB) aren't emulated anyway.

## References

- [ESP-IDF QEMU Guide](https://docs.espressif.com/projects/esp-idf/en/stable/esp32s3/api-guides/tools/qemu.html)
- [ESP-IDF stdio Guide](https://docs.espressif.com/projects/esp-idf/en/stable/esp32s3/api-guides/stdio.html)
- [Espressif QEMU Releases](https://github.com/espressif/qemu/releases)
- [esp-idf-sys BUILD-OPTIONS.md](https://github.com/esp-rs/esp-idf-sys/blob/master/BUILD-OPTIONS.md)

---

*Updated 2026-01-12: Simplified to `cargo test-qemu` command*
