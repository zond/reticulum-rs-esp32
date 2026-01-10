# Implementation Guide

Detailed implementation plans for the ESP32 transport node.

## Table of Contents

1. [LoRa Interface (SX1262)](#1-lora-interface-sx1262)
2. [BLE Interface](#2-ble-interface)
3. [Identity Persistence](#3-identity-persistence)
4. [Bandwidth/Airtime Limiting](#4-bandwidthairtime-limiting)

**Note**: Features like Channels, Resources, Ratcheting, and Group Destinations are **out of scope** - they're endpoint/application concerns handled by client devices (e.g., Android phone running Sideband), not transport nodes.

---

## 1. LoRa Interface (SX1262)

**Scope**: This project
**Estimate**: ~800 lines
**Priority**: BLOCKER

### Available Crates

| Crate | Version | embedded-hal | Maintenance | Recommendation |
|-------|---------|--------------|-------------|----------------|
| `sx126x` | 0.3.0 | 1.0 | Active | **Recommended** - simple, low-level |
| `sx1262` | 0.3.0 | 1.0 | Active (updated recently) | Alternative - more type-safe |
| `lora-phy` | 3.0.1 | 1.0 + async | Active | Overkill - includes LoRaWAN stack |

### Recommended: `sx126x` crate

```toml
[dependencies]
sx126x = "0.3"
```

**Why**: Simple API, active maintenance, embedded-hal 1.0, works with esp-idf-hal.

### Reticulum-rs Interface Architecture

Interfaces in reticulum-rs use a **channel-based async worker pattern**:

```rust
pub trait Interface {
    fn mtu() -> usize;  // Maximum packet size (typically 500 bytes for LoRa)
}
```

Each interface gets an `InterfaceContext<T>` with:
- `rx_channel`: Send received packets to transport layer
- `tx_channel`: Receive packets to transmit from transport layer
- `cancel`: Shutdown signal

### Implementation Plan

```rust
// src/lora.rs
use sx126x::{Sx126x, Config};
use reticulum::iface::{Interface, InterfaceContext, RxMessage, TxMessage};

pub struct LoRaInterface {
    // SX1262 driver instance
    radio: Sx126x<SpiDevice, OutputPin, OutputPin, OutputPin>,
    // LoRa parameters
    frequency: u32,
    spreading_factor: u8,
    bandwidth: u32,
    coding_rate: u8,
    tx_power: i8,
}

impl Interface for LoRaInterface {
    fn mtu() -> usize { 500 }  // Reticulum default
}

impl LoRaInterface {
    pub async fn spawn(context: InterfaceContext<Self>) {
        let (rx_sender, tx_receiver) = context.channel.split();

        loop {
            tokio::select! {
                _ = context.cancel.cancelled() => break,

                // Receive from radio
                packet = receive_packet(&context.inner) => {
                    if let Ok(data) = packet {
                        if let Ok(pkt) = Packet::deserialize(&data) {
                            let msg = RxMessage { address: self_hash, packet: pkt };
                            rx_sender.send(msg).await;
                        }
                    }
                }

                // Transmit to radio
                Some(tx_msg) = tx_receiver.recv() => {
                    let bytes = tx_msg.packet.serialize();
                    transmit_packet(&context.inner, &bytes).await;
                }
            }
        }
    }
}
```

### LILYGO T3-S3 Pin Configuration

Based on typical T3-S3 board pinouts:

| Signal | GPIO | Notes |
|--------|------|-------|
| SPI MOSI | GPIO 11 | |
| SPI MISO | GPIO 13 | |
| SPI CLK | GPIO 12 | |
| NSS (CS) | GPIO 10 | Chip select |
| RESET | GPIO 5 | Radio reset |
| BUSY | GPIO 4 | Radio busy status |
| DIO1 | GPIO 1 | Interrupt |

### Key Implementation Tasks

1. **Initialize SPI** using esp-idf-hal
2. **Configure SX1262** with appropriate LoRa parameters
3. **Implement receive loop** with DIO1 interrupt or polling
4. **Implement transmit** with CSMA/CA (listen-before-talk)
5. **Handle airtime limiting** (see section 6)
6. **Register with InterfaceManager** in main.rs

### LoRa Parameter Recommendations

For Reticulum compatibility (based on RNode defaults):

| Parameter | Value | Notes |
|-----------|-------|-------|
| Frequency | 868.0 MHz (EU) / 915.0 MHz (US) | Region-dependent |
| Spreading Factor | SF7-SF12 | Higher = longer range, slower |
| Bandwidth | 125 kHz | Standard LoRa |
| Coding Rate | 4/5 | Good error correction |
| TX Power | +14 dBm | Adjust for regulations |
| Preamble | 8 symbols | Standard |
| Sync Word | 0x12 | Reticulum default |

---

## 2. BLE Interface

**Scope**: This project (custom protocol)
**Estimate**: ~1200 lines
**Priority**: BLOCKER

### Recommended Crate: `esp32-nimble`

```toml
[dependencies]
esp32-nimble = "0.11"
```

**Why**: Most mature ESP32 BLE option, active maintenance, full GATT support.

### Required sdkconfig.defaults

```
CONFIG_BT_ENABLED=y
CONFIG_BT_BLE_ENABLED=y
CONFIG_BT_BLUEDROID_ENABLED=n
CONFIG_BT_NIMBLE_ENABLED=y
```

### BLE Mesh Protocol Design

Since BLE mesh is NOT part of standard Reticulum, we need a custom protocol. Options:

#### Option A: GATT-based (Recommended for reliability)

- Create a custom GATT service for Reticulum packets
- Characteristic for TX (write), RX (notify)
- Each connected peer is a separate "interface"
- Pros: Reliable, acknowledged delivery
- Cons: Connection overhead, limited peers (~4-7 concurrent)

#### Option B: Advertisement-based (Better for mesh)

- Encode small packets in BLE advertisements
- Scan and advertise continuously
- No connection required
- Pros: True broadcast, more peers
- Cons: Size limited (~31 bytes), no acknowledgment

#### Recommended: Hybrid Approach

1. Use **advertisements** for announces (broadcast)
2. Use **GATT connections** for data packets (reliable)

### Implementation Plan

```rust
// src/ble.rs
use esp32_nimble::{BLEDevice, BLEServer, BLEAdvertising};

const RETICULUM_SERVICE_UUID: &str = "RETI-CLUM-0001-...";
const RX_CHAR_UUID: &str = "RETI-RX01-...";
const TX_CHAR_UUID: &str = "RETI-TX01-...";

pub struct BleInterface {
    device: BLEDevice,
    peers: Vec<BlePeer>,
}

impl Interface for BleInterface {
    fn mtu() -> usize { 500 }  // May need fragmentation for BLE
}

impl BleInterface {
    pub fn new() -> Self {
        let device = BLEDevice::take();
        let server = device.get_server();

        // Create Reticulum service
        let service = server.create_service(RETICULUM_SERVICE_UUID);

        // RX characteristic (for receiving from peers)
        let rx_char = service.create_characteristic(RX_CHAR_UUID)
            .write(true)
            .on_write(|data| { /* handle incoming packet */ });

        // TX characteristic (for sending to peers via notify)
        let tx_char = service.create_characteristic(TX_CHAR_UUID)
            .notify(true);

        Self { device, peers: vec![] }
    }

    pub async fn spawn(context: InterfaceContext<Self>) {
        // Start advertising
        // Handle connections
        // Route packets between GATT and reticulum channels
    }
}
```

### Packet Fragmentation

BLE MTU is typically 20-512 bytes (negotiated). For packets larger than BLE MTU:

1. Fragment into chunks with sequence number
2. Reassemble at receiver
3. Simple header: `[seq:1][total:1][data:N]`

### Key Implementation Tasks

1. **Initialize NimBLE** stack
2. **Create GATT service** with RX/TX characteristics
3. **Implement advertising** with Reticulum identifier
4. **Handle connections** and track peers
5. **Implement packet fragmentation** for large packets
6. **Bridge to reticulum-rs** interface channels

---

## 3. Identity Persistence

**Scope**: This project only (no upstream contribution needed)
**Estimate**: ~50 lines
**Priority**: HIGH

### Problem

Currently, reticulum-rs has no identity persistence. Device gets new identity on every boot.

### Solution: Simple NVS Functions

Use simple functions instead of traits - more appropriate for an MVP and avoids over-engineering.

### Implementation Plan

```rust
// src/persistence.rs
use esp_idf_svc::nvs::{EspNvs, EspNvsPartition, NvsDefault};
use esp_idf_sys::EspError;
use reticulum::identity::PrivateIdentity;

const NVS_NAMESPACE: &str = "reticulum";
const IDENTITY_KEY: &str = "device_id";

/// Load identity from NVS, returns None if not found or corrupted
pub fn load_identity(nvs: &EspNvs<NvsDefault>) -> Option<PrivateIdentity> {
    let mut buf = [0u8; 128]; // Ed25519 + X25519 keys fit in 128 bytes
    nvs.get_raw(IDENTITY_KEY, &mut buf)
        .ok()
        .flatten()
        .and_then(|bytes| PrivateIdentity::from_bytes(bytes).ok())
}

/// Save identity to NVS
pub fn save_identity(
    nvs: &mut EspNvs<NvsDefault>,
    identity: &PrivateIdentity,
) -> Result<(), EspError> {
    nvs.set_raw(IDENTITY_KEY, &identity.to_bytes())
}

/// Load existing identity or create and persist a new one
pub fn load_or_create_identity(
    nvs: &mut EspNvs<NvsDefault>,
) -> Result<PrivateIdentity, EspError> {
    if let Some(identity) = load_identity(nvs) {
        return Ok(identity);
    }

    // Create new identity and persist
    let identity = PrivateIdentity::new();
    save_identity(nvs, &identity)?;
    Ok(identity)
}

/// Initialize NVS partition and namespace
pub fn init_nvs() -> Result<EspNvs<NvsDefault>, EspError> {
    let partition = EspNvsPartition::<NvsDefault>::take()?;
    EspNvs::new(partition, NVS_NAMESPACE, true)
}
```

### Usage in main.rs

```rust
fn main() -> Result<()> {
    let mut nvs = persistence::init_nvs()?;
    let identity = persistence::load_or_create_identity(&mut nvs)?;
    log::info!("Node identity: {:?}", identity.hash());
    // ... rest of initialization
}
```

---

## 4. Bandwidth/Airtime Limiting

**Scope**: This project only (no upstream contribution needed)
**Estimate**: ~100 lines
**Priority**: HIGH (critical for LoRa)

### Why It Matters

LoRa has legal duty cycle limits (e.g., 1% in EU 868 MHz band). Without airtime tracking, the device could violate regulations and cause interference.

### Solution: Token Bucket Algorithm

Using a token bucket instead of a sliding window:
- O(1) memory (no VecDeque of transmission history)
- O(1) operations (no cleanup loops)
- Naturally handles the "refill over time" semantic of duty cycles
- More lenient for bursty traffic

**On errors**: When duty cycle is exceeded, we return an error and drop the packet. This is appropriate because Reticulum expects lossy networks and has built-in retry mechanisms. Better to drop packets than violate regulations.

### Implementation Plan

```rust
// src/lora/duty_cycle.rs
use std::time::{Duration, Instant};

/// Duty cycle limiter using token bucket algorithm
///
/// Tracks airtime budget in microseconds. Budget refills continuously
/// over the window duration, allowing for bursty transmissions as long
/// as average duty cycle is maintained.
pub struct DutyCycleLimiter {
    /// Maximum budget in microseconds
    budget_us: u64,
    /// Remaining budget
    remaining_us: u64,
    /// Last refill time
    last_refill: Instant,
    /// Window duration for refill calculation
    window: Duration,
}

impl DutyCycleLimiter {
    /// Create limiter for given duty cycle percentage over window
    ///
    /// # Arguments
    /// * `duty_cycle_percent` - Duty cycle limit (e.g., 1.0 for 1%)
    /// * `window` - Time window (e.g., 1 hour for EU regulations)
    ///
    /// # Example
    /// ```
    /// // 1% duty cycle over 1 hour (EU 868 MHz band)
    /// let limiter = DutyCycleLimiter::new(1.0, Duration::from_secs(3600));
    /// ```
    pub fn new(duty_cycle_percent: f32, window: Duration) -> Self {
        let budget_us = (window.as_micros() as f64 * duty_cycle_percent as f64 / 100.0) as u64;
        Self {
            budget_us,
            remaining_us: budget_us,
            last_refill: Instant::now(),
            window,
        }
    }

    /// Check if transmission is allowed and consume budget if so
    ///
    /// Returns true if transmission was allowed, false if duty cycle exceeded.
    pub fn try_consume(&mut self, airtime_us: u64) -> bool {
        self.refill();
        if self.remaining_us >= airtime_us {
            self.remaining_us -= airtime_us;
            true
        } else {
            false
        }
    }

    /// Get remaining budget in microseconds
    pub fn remaining(&mut self) -> u64 {
        self.refill();
        self.remaining_us
    }

    /// Get remaining budget as percentage of total
    pub fn remaining_percent(&mut self) -> f32 {
        self.refill();
        (self.remaining_us as f64 / self.budget_us as f64 * 100.0) as f32
    }

    fn refill(&mut self) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill);

        // Calculate how much budget to restore based on elapsed time
        // Using u128 to avoid overflow in intermediate calculation
        let refill_amount = (self.budget_us as u128 * elapsed.as_micros()
            / self.window.as_micros()) as u64;

        if refill_amount > 0 {
            self.remaining_us = (self.remaining_us + refill_amount).min(self.budget_us);
            self.last_refill = now;
        }
    }
}
```

### LoRa Time-on-Air Calculation

```rust
/// Calculate LoRa packet airtime in milliseconds
pub fn calculate_airtime(
    payload_bytes: usize,
    spreading_factor: u8,
    bandwidth_hz: u32,
    coding_rate: u8,  // 1-4 for 4/5 to 4/8
    preamble_symbols: u8,
    explicit_header: bool,
    low_data_rate_optimize: bool,
) -> f64 {
    let sf = spreading_factor as f64;
    let bw = bandwidth_hz as f64;
    let cr = coding_rate as f64;

    // Symbol duration
    let t_sym = (2.0_f64.powf(sf)) / bw * 1000.0; // ms

    // Preamble duration
    let t_preamble = (preamble_symbols as f64 + 4.25) * t_sym;

    // Payload symbols (simplified formula)
    let de = if low_data_rate_optimize { 1.0 } else { 0.0 };
    let h = if explicit_header { 0.0 } else { 1.0 };

    let payload_symbols = 8.0 +
        (((8 * payload_bytes as i32 - 4 * sf as i32 + 28 + 16 - 20 * h as i32) as f64 /
          (4.0 * (sf - 2.0 * de))).ceil() * (cr + 4.0)).max(0.0);

    let t_payload = payload_symbols * t_sym;

    t_preamble + t_payload
}
```

### Integration with Interface

```rust
impl LoRaInterface {
    pub async fn transmit(&mut self, packet: &[u8]) -> Result<(), Error> {
        let airtime_us = calculate_airtime_us(
            packet.len(),
            self.spreading_factor,
            self.bandwidth,
            self.coding_rate,
            8, // preamble
            true, // explicit header
            self.spreading_factor >= 11,
        );

        if !self.duty_cycle.try_consume(airtime_us) {
            log::warn!(
                "Duty cycle exceeded, {}% remaining",
                self.duty_cycle.remaining_percent()
            );
            return Err(Error::DutyCycleExceeded);
        }

        // Actual transmission
        self.radio.transmit(packet)?;
        Ok(())
    }
}
```

### Key Implementation Tasks

1. **Add DutyCycleLimiter** using token bucket algorithm
2. **Implement time-on-air calculation** for LoRa (returns microseconds)
3. **Integrate with LoRa interface** transmit path
4. **Add configurable duty cycle** per region (1% EU, 10% US, etc.)
5. **Log airtime statistics** for debugging

---

## Summary: Implementation Order

### Phase 1: Get Device Working (Blockers)

1. **LoRa Interface** - Can send/receive packets over radio
2. **Identity Persistence** - Stable identity across reboots
3. **WiFi/TCP** - Connect to testnet (should mostly work already)

### Phase 2: Full Functionality

4. **BLE Interface** - Mesh with BLE devices
5. **Airtime Limiting** - Legal compliance for LoRa

### Dependencies

```
LoRa Interface ─────┐
                    ├──► Airtime Limiting
Identity Persistence┘

BLE Interface (independent)
```

### Total Estimate

| Component | Lines |
|-----------|-------|
| LoRa Interface (SX1262) | ~800 |
| BLE Interface | ~1200 |
| Identity Persistence | ~50 |
| Airtime Limiting | ~100 |
| **Total** | **~2150** |

Reduced from original ~2400 estimate after simplifying persistence and rate limiting based on agent review.
