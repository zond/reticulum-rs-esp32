# Future Work

Items planned for future implementation. See also [implementation-guide.md](implementation-guide.md) for current status.

## Immediate Priorities

### 1. Hardware Testing (LoRa)

✅ **Interface adapter implemented** - See `src/lora/iface.rs`.

The `LoRaInterface` implements reticulum's Interface trait and bridges the radio with transport channels. Remaining work:
1. Test on actual ESP32 hardware with LILYGO T3-S3 board
2. Verify SPI and GPIO pin assignments match the board
3. Validate LoRa TX/RX through the transport layer

### 2. BLE Mesh Interface (BLOCKER)

BLE fragmentation (`src/ble/fragmentation.rs`) is complete. Need to:
1. Create GATT service for Reticulum packets (separate from WiFi config)
2. Implement peer discovery and connection management
3. Bridge GATT RX/TX with transport channels

### 3. Hardware Testing

Flash to actual ESP32 hardware and verify:
- ✅ WiFi connection - Auto-configures from NVS (`cargo configure-wifi`)
- Testnet connectivity (requires WiFi config first)
- Stats endpoint accessibility
- Identity persistence across reboots

### 4. Two-Node Integration Test

**Partially Resolved (2026-01)** - The `Node` abstraction in `src/node.rs` provides a testable node interface that encapsulates Transport, identity, destination, and event processing.

The test (`test_two_node_communication`) is currently `#[ignore]` due to a testnet routing issue: when two nodes connect from the same IP address, the testnet server appears unable to route directed packets (like link requests) to the correct client. Broadcast packets (announces) work fine.

**Next steps:**
- Investigate if this is a reticulum-rs library limitation or testnet server behavior
- Consider testing with two separate processes or machines
- May need patches to reticulum-rs interface management

Run manually with:
```bash
cargo test test_two_node -- --ignored --nocapture
```

Excluded from ESP32/QEMU builds via `#[cfg(not(feature = "esp32"))]`.

---

## BLE Configuration Expansion

The BLE GATT service (`src/config/ble_service.rs`) currently only configures WiFi credentials. Future extensions:

### Planned Configuration Options

| Setting | Description | Priority |
|---------|-------------|----------|
| WiFi credentials | SSID and password | Done |
| Testnet server | Which testnet entry point to use | High |
| Announce filtering | Whether gateway filters internet→mesh announces (see [scalable-routing-proposal.md](scalable-routing-proposal.md)) | High |
| LoRa region | EU868, US915, etc. | Medium |
| DHT participation | Whether to join routing DHT (future) | Low |

### Implementation Notes

The BLE configuration service (`src/config/ble_service.rs`) uses a simple command-response protocol. Extensions should:

1. Add new command types to `ConfigCommand` enum in `src/config/wifi.rs`
2. Handle new commands in the BLE service
3. Store configuration in NVS (like WiFi credentials)

### Protocol Extension

Current implementation uses string-based commands via separate BLE characteristics:
- SSID characteristic: write network name
- Password characteristic: write password
- Command characteristic: write "connect" | "disconnect" | "clear"
- Status characteristic: read current status

Proposed additions (new characteristics or command extensions):
- Testnet server: "dublin" | "frankfurt" | custom host:port
- Announce filtering: "filter_announces:true" | "filter_announces:false"
- LoRa region: "region:EU868" | "region:US915" | etc.
- Full config read: JSON response with all settings

## Routing DHT Integration

See [scalable-routing-proposal.md](scalable-routing-proposal.md) for details on the DHT-based routing proposal.

When implemented, the BLE configuration should allow:
- Enabling/disabling DHT participation
- Configuring DHT bootstrap nodes
- Setting local mesh identifier

## Interrupt-Driven Radio

✅ **Resolved (2026-01)** - The LoRa radio driver now uses interrupt-driven waiting instead of polling. DIO1 is configured for positive edge interrupts, and `wait_tx_done()` / `wait_rx_done()` block on a FreeRTOS notification instead of polling every 1ms. See `src/lora/radio.rs` and [docs/interrupt-driven-radio-plan.md](interrupt-driven-radio-plan.md) for implementation details.

## Test Infrastructure Improvements

From code review (2026-01):

| Improvement | Description | Priority |
|-------------|-------------|----------|
| Configurable flash size | Hardcoded 4MB flash size in test runner | Low |

**Resolved (2026-01)**:
- ✅ Cargo JSON metadata - Uses `--message-format=json` for deterministic test binary detection
- ✅ Cross-platform monitor - Now uses `espflash monitor --non-interactive` instead of macOS-specific `script` wrapper
- ✅ Crash detection state machine - Uses `TestState` enum (Booting/Initialized/Running) for context-aware crash detection
- ✅ Port detection glob optimization - Uses specific `/dev/ttyUSB*` and `/dev/ttyACM*` patterns instead of `/dev/tty*` filtering

## Chat Interface Improvements

The serial chat interface (`src/chat.rs`, `src/bin/node.rs`) has known limitations:

### Known Limitations

1. **Stdin blocking on ESP32** - The stdin reader uses `spawn_blocking` which cannot be cancelled mid-read. On ESP32, the task runs forever if no input arrives. No clean shutdown mechanism exists. (See `src/bin/node.rs:461`)

2. **Linear search for hash prefix** - `get_destination()` does O(n) search when matching by hash prefix. With MAX_KNOWN_DESTINATIONS=100, this is acceptable but could be improved with a trie.

### Potential Improvements

| Improvement | Description | Priority |
|-------------|-------------|----------|
| Platform-specific stdin | Use non-blocking stdin on host for clean shutdown | Low |

**Resolved (2026-01)**:
- ✅ Batch broadcast sends - Packets collected first, then sent in single transport lock
- ✅ O(1) LRU eviction - Evaluated; current O(n) scan takes <1μs at 100 entries, LRU crates don't fit the data model well (need sequential indexing + multi-field structs)

