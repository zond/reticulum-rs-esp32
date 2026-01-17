# LoRa Communication Testing Strategy

Testing LoRa communication between our Rust ESP32 implementation and the reference Reticulum network.

## Overview

To verify our LoRa implementation works correctly with the Reticulum network, we use a two-device setup:

1. **RNode Device**: ESP32 with official RNode firmware, connected via USB to host running Python Reticulum
2. **Test Device**: ESP32 with our Rust firmware, communicating over LoRa

This setup allows us to test real LoRa packet transmission and reception against the reference implementation.

## Hardware Requirements

- 2x ESP32 boards with LoRa capability (e.g., LILYGO T3-S3, Heltec LoRa32)
- Both boards must use compatible LoRa transceivers (SX1262/SX1268 or SX1276/SX1278)
- USB cables for both devices

## Software Requirements

### Host Machine

```bash
# Install Python Reticulum
pip install rns

# Verify installation
rnsd --version
rnodeconf --version
```

### RNode Firmware

Flash official RNode firmware to one device:

```bash
# Auto-install RNode firmware (interactive)
rnodeconf --autoinstall

# Or specify port explicitly
rnodeconf --autoinstall --port /dev/cu.usbserial-RNODE_SERIAL
```

## Setup Procedure

### Step 1: Identify Device Serial Numbers

Connect both ESP32 devices and identify their USB serial numbers:

```bash
# macOS
ls /dev/cu.usbserial-*

# Linux
ls /dev/serial/by-id/
```

Example output:
```
/dev/cu.usbserial-12345678  # RNode device
/dev/cu.usbserial-87654321  # Test device (our Rust firmware)
```

**Write these down** - you'll need them to avoid flashing the wrong device.

### Step 2: Flash RNode Firmware

Flash the RNode firmware to the first device:

```bash
# Set the RNode device port
export RNODE_PORT=/dev/cu.usbserial-12345678

rnodeconf --autoinstall --port $RNODE_PORT
```

Follow the prompts to configure:
- Frequency band (868 MHz for EU, 915 MHz for US)
- Device model

Verify the RNode is working:

```bash
rnodeconf --info $RNODE_PORT
```

### Step 3: Configure Python Reticulum

Create or edit `~/.reticulum/config`:

```ini
[reticulum]
  enable_transport = yes
  share_instance = yes

[logging]
  loglevel = 4

[[RNode LoRa Interface]]
  type = RNodeInterface
  enabled = yes
  port = /dev/cu.usbserial-12345678
  frequency = 867200000
  bandwidth = 125000
  txpower = 7
  spreadingfactor = 8
  codingrate = 5
```

**Important**: The LoRa parameters must match our Rust implementation:
- `frequency`: Must be same as our config (region-dependent)
- `bandwidth`: 125000 Hz (125 kHz)
- `spreadingfactor`: 8
- `codingrate`: 5 (4/5 coding rate)

### Step 4: Start Reticulum Daemon

```bash
# Run in foreground for testing (shows logs)
rnsd -v

# Or run as background service
rnsd &
```

### Step 5: Flash Our Rust Firmware

Use the PORT environment variable to target the correct device:

```bash
# Set the test device port
export PORT=/dev/cu.usbserial-87654321

# Flash and monitor
cargo flash-esp32

# Or run tests
cargo test-esp32
```

## Testing Scenarios

### 1. Basic Announce Reception

**Goal**: Verify our device receives announces from the RNode/Reticulum network.

**Procedure**:
1. Start `rnsd` with RNode interface
2. Flash our firmware to test device
3. Monitor test device serial output
4. Create a destination on the Reticulum side:
   ```bash
   # Use rncp or another Reticulum utility to create traffic
   rncp --announce
   ```
5. Verify test device logs show received announce packets

**Expected**: Test device should log received LoRa packets with valid Reticulum headers.

### 2. Announce Transmission

**Goal**: Verify our device can transmit announces that Reticulum receives.

**Procedure**:
1. Start `rnsd -v` (verbose mode)
2. Flash our firmware with announce enabled
3. Watch Reticulum logs for received announces
4. Verify announce appears in Reticulum's destination table

**Expected**: Reticulum should log received announce from our device's identity hash.

### 3. Bidirectional Link

**Goal**: Establish a link between our device and a Reticulum destination.

**Procedure**:
1. Start `rnsd` with a destination listening for links
2. Have our device attempt to create a link to that destination
3. Verify link establishment handshake completes
4. Send test data over the link

**Expected**: Full link establishment and data transfer.

### 4. Duty Cycle Compliance

**Goal**: Verify our duty cycle limiter works correctly.

**Procedure**:
1. Configure aggressive transmission rate
2. Monitor transmit count and timing
3. Verify transmission is throttled to legal limits (1% for EU 868 MHz)

**Expected**: Transmission rate should be limited appropriately.

## LoRa Parameter Compatibility

Our implementation must use compatible parameters with RNode. Check `src/lora/config.rs`:

| Parameter | RNode Default | Our Implementation |
|-----------|---------------|-------------------|
| Frequency | Region-dependent | Same |
| Bandwidth | 125 kHz | 125000 Hz |
| Spreading Factor | 8 | 8 |
| Coding Rate | 4/5 | 5 |
| Sync Word | 0x12 | 0x12 |
| Preamble | 8 symbols | 8 |
| TX Power | 7 dBm | Configurable |

## Troubleshooting

### No Communication

1. **Check frequencies match** - Both devices must use identical frequency
2. **Check LoRa parameters** - SF, BW, CR must all match
3. **Check sync word** - Must be 0x12 for Reticulum
4. **Check antenna** - Ensure antennas are connected
5. **Check distance** - Start with devices close together

### Wrong Device Flashed

If you accidentally flash the wrong device:

1. Note which serial number got the wrong firmware
2. Reflash the correct firmware using explicit PORT:
   ```bash
   # Restore RNode firmware
   rnodeconf --autoinstall --port /dev/cu.usbserial-WRONG_SERIAL

   # Or restore our firmware
   PORT=/dev/cu.usbserial-WRONG_SERIAL cargo flash-esp32
   ```

### Serial Port Conflicts

If `rnsd` holds a port that you need to flash:

```bash
# Stop Reticulum daemon
pkill rnsd

# Flash the device
PORT=/dev/cu.usbserial-XXX cargo flash-esp32

# Restart Reticulum
rnsd
```

## Environment Variables Reference

| Variable | Description | Example |
|----------|-------------|---------|
| `PORT` | Serial port for flash utilities | `/dev/cu.usbserial-12345678` |
| `RNODE_PORT` | Serial port for RNode (convention) | `/dev/cu.usbserial-87654321` |

## Quick Reference Commands

```bash
# List available serial ports (macOS)
ls /dev/cu.usbserial-*

# List available serial ports (Linux)
ls /dev/serial/by-id/

# Flash our firmware to specific device
PORT=/dev/cu.usbserial-XXX cargo flash-esp32

# Run tests on specific device
PORT=/dev/cu.usbserial-XXX cargo test-esp32

# Configure WiFi on specific device
PORT=/dev/cu.usbserial-XXX WIFI_SSID="net" WIFI_PASSWORD="pass" cargo configure-wifi

# Check RNode status
rnodeconf --info /dev/cu.usbserial-XXX

# Start Reticulum with verbose logging
rnsd -v

# Stop Reticulum daemon
pkill rnsd
```

## Future Improvements

1. **Automated test harness**: Script that manages both devices and runs test suite
2. **CI integration**: Use two devices in CI for hardware-in-the-loop testing
3. **Signal quality metrics**: Log RSSI/SNR for range testing
4. **Stress testing**: High-volume packet transmission tests
