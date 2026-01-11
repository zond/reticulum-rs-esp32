# QEMU Emulator Setup

## Current Status (2026-01-11)

**Progress:** QEMU installed but needs upgrade for ESP32-S3 support.

### What's Working
- QEMU 8.1.3 installed via `idf_tools.py` at: `/Users/zond/.espressif/tools/qemu-xtensa/esp_develop_8.1.3_20231206/qemu/bin/qemu-system-xtensa`
- Dependencies installed via Homebrew: `libgcrypt glib pixman sdl2 libslirp`
- Firmware builds successfully: `cargo espflash save-image --chip esp32s3 --merge --flash-size 4mb --release target/firmware.bin`

### Problem
QEMU 8.1.3 only supports plain ESP32, not ESP32-S3:
```
$ qemu-system-xtensa -machine help
Supported machines are:
esp32                Espressif ESP32 machine
...
```

### Solution: Upgrade QEMU
Need to install newer QEMU version with ESP32-S3 support.

**Available releases from [espressif/qemu](https://github.com/espressif/qemu/releases):**
- `esp-develop-9.2.2-20250817` (latest) - ESP32, ESP32-C3
- `esp-develop-9.2.2-20250228` - ESP32, ESP32-C3, **ESP32-S3** (has S3 support!)

### Next Steps

1. **Download newer QEMU with S3 support:**
   ```bash
   # For x86_64 macOS:
   curl -LO https://github.com/espressif/qemu/releases/download/esp-develop-9.2.2-20250228/qemu-xtensa-softmmu-esp_develop_9.2.2_20250228-x86_64-apple-darwin.tar.xz

   # Extract to ~/.espressif/tools/qemu-xtensa/
   mkdir -p ~/.espressif/tools/qemu-xtensa/esp_develop_9.2.2_20250228
   tar -xf qemu-xtensa-softmmu-esp_develop_9.2.2_20250228-x86_64-apple-darwin.tar.xz -C ~/.espressif/tools/qemu-xtensa/esp_develop_9.2.2_20250228
   ```

2. **Run firmware in QEMU:**
   ```bash
   # Create merged firmware image
   cargo espflash save-image --chip esp32s3 --merge --flash-size 4mb --release target/firmware.bin

   # Run in QEMU (adjust path to new version)
   ~/.espressif/tools/qemu-xtensa/esp_develop_9.2.2_20250228/qemu/bin/qemu-system-xtensa \
     -machine esp32s3 \
     -nographic \
     -drive file=target/firmware.bin,if=mtd,format=raw
   ```

3. **Add convenience scripts** (optional):
   - Add QEMU path to shell config or create wrapper script
   - Add `cargo qemu` alias for quick testing

---

## Installation Reference

### Dependencies (macOS)
```bash
brew install libgcrypt glib pixman sdl2 libslirp
```

### Install QEMU via ESP-IDF tools
```bash
python3 .embuild/espressif/esp-idf/v5.2/tools/idf_tools.py install qemu-xtensa
```

Note: This installs an older version (8.1.3) that doesn't support ESP32-S3.

### Build Firmware for QEMU
```bash
# Build release
cargo build --release

# Create merged flash image (required for QEMU)
cargo espflash save-image --chip esp32s3 --merge --flash-size 4mb --release target/firmware.bin
```

### QEMU Command Reference
```bash
# Basic run with serial output
qemu-system-xtensa -machine esp32s3 -nographic -drive file=target/firmware.bin,if=mtd,format=raw

# With GDB server for debugging
qemu-system-xtensa -machine esp32s3 -nographic -drive file=target/firmware.bin,if=mtd,format=raw -s -S

# Connect GDB
xtensa-esp32s3-elf-gdb target/xtensa-esp32s3-espidf/release/reticulum-rs-esp32 -ex "target remote :1234"
```

---

## Known Limitations

- QEMU emulation is slower than real hardware (~4-5x)
- BLE/WiFi radio emulation may be limited or absent
- Virtual eFuse mode for flash encryption is for testing only (not real security)
- Some peripherals not fully emulated

## Features to Test in QEMU

- [ ] Basic firmware boot and logging
- [ ] NVS read/write operations
- [ ] Identity persistence (load/save/create)
- [ ] WiFi configuration BLE service (BLE may have limited support)
- [ ] LoRa duty cycle limiter logic (no actual radio, but timing logic)

## References

- [ESP-IDF QEMU Guide](https://docs.espressif.com/projects/esp-idf/en/stable/esp32s3/api-guides/tools/qemu.html)
- [Espressif QEMU Releases](https://github.com/espressif/qemu/releases)
- [Flash Encryption Guide](https://docs.espressif.com/projects/esp-idf/en/stable/esp32/security/flash-encryption.html)

---

*Updated 2026-01-11*
