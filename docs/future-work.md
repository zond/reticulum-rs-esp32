# Future Work

Items planned for future implementation. See also [implementation-guide.md](implementation-guide.md) for current status.

## Immediate Priorities

### 1. Hardware Testing (LoRa)

The `LoRaInterface` in `src/lora/iface.rs` implements reticulum's Interface trait. Remaining work:
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

## Test Infrastructure Improvements

| Improvement | Description | Priority |
|-------------|-------------|----------|
| Configurable flash size | Hardcoded 4MB flash size in test runner | Low |

## Chat Interface Improvements

The serial chat interface (`src/chat.rs`, `src/bin/node.rs`) has known limitations:

### Known Limitations

1. **Stdin blocking on ESP32** - The stdin reader uses `spawn_blocking` which cannot be cancelled mid-read. On ESP32, the task runs forever if no input arrives. No clean shutdown mechanism exists. (See `src/bin/node.rs:461`)

2. **Linear search for hash prefix** - `get_destination()` does O(n) search when matching by hash prefix. With MAX_KNOWN_DESTINATIONS=100, this is acceptable but could be improved with a trie.

### Potential Improvements

| Improvement | Description | Priority |
|-------------|-------------|----------|
| Platform-specific stdin | Use non-blocking stdin on host for clean shutdown | Low |
