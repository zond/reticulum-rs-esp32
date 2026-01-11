# QEMU Emulator Setup

## TODO: Set Up ESP32 QEMU Development Environment

Espressif maintains an official QEMU fork that can emulate ESP32, allowing development and testing without physical hardware.

### Why QEMU?

- **Faster iteration**: No flash/reboot cycle
- **GDB debugging**: Full debugging support
- **CI/CD integration**: Automated testing in GitHub Actions
- **Security testing**: Virtual eFuse support for flash encryption testing

### Setup Steps (To Be Implemented)

1. **Install QEMU for ESP32**
   ```bash
   # Pre-built binaries available for Linux/macOS/Windows x86_64 and arm64
   # See: https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-guides/tools/qemu.html
   ```

2. **Configure ESP-IDF for QEMU**
   ```bash
   # Build for QEMU target
   idf.py set-target esp32
   idf.py build

   # Run in QEMU
   idf.py qemu monitor
   ```

3. **Cargo integration**
   - Need to investigate how `cargo espflash` / `cargo build` integrates with QEMU
   - May need custom runner configuration in `.cargo/config.toml`

### Features to Test in QEMU

- [ ] Basic firmware boot and logging
- [ ] NVS read/write operations
- [ ] Identity persistence (load/save/create)
- [ ] WiFi configuration BLE service (BLE may have limited support)
- [ ] LoRa duty cycle limiter logic (no actual radio, but timing logic)

### Security Features in QEMU

QEMU supports testing security features with **virtual eFuses**:

- **Flash encryption**: Can test encrypted flash workflow
- **Secure boot v2**: Bootloader signing verification
- **NVS encryption**: Can test AES-XTS encrypted NVS partition

**Important**: Virtual eFuse mode does NOT provide real security - it's for testing code paths only. Real security verification requires physical hardware with burned eFuses.

### Known QEMU Limitations

- Some peripherals not fully emulated
- SHA emulation doesn't support concurrent operations with different SHA types
- BLE/WiFi radio emulation may be limited or absent
- Chip revision detection may need `CONFIG_ESP32_IGNORE_CHIP_REVISION_CHECK`

### References

- [ESP-IDF QEMU Guide](https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-guides/tools/qemu.html)
- [QEMU ESP32 GitHub](https://github.com/espressif/esp-toolchain-docs/blob/main/qemu/esp32/README.md)
- [Flash Encryption Guide](https://docs.espressif.com/projects/esp-idf/en/stable/esp32/security/flash-encryption.html)
- [NVS Encryption Guide](https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/storage/nvs_encryption.html)

---

*Added 2026-01-11 - Priority: High (enables ESP32 development without hardware)*
