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
- WiFi connection
- Testnet connectivity
- Stats endpoint accessibility
- Identity persistence across reboots

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

## Stats HTTP Endpoint

✅ **Implemented** - See `src/network/stats_server.rs`. Available at `http://localhost:8080/stats`.

## Interrupt-Driven Radio

The LoRa radio driver currently uses polling. Switching to DIO1 interrupt would improve power efficiency. See TODO in `src/lora/radio.rs:138`.

## Chat Interface Improvements

The serial chat interface (`src/chat.rs`, `src/bin/node.rs`) has known limitations:

### Known Limitations

1. **Stdin blocking on ESP32** - The stdin reader uses `spawn_blocking` which cannot be cancelled mid-read. On ESP32, the task runs forever if no input arrives. No clean shutdown mechanism exists. (See `src/bin/node.rs:298`)

2. **Link activation delay** - ✅ Fixed: Messages sent to pending links are now queued and automatically sent when the link activates. See `MAX_QUEUED_MESSAGES_PER_DEST` in `src/bin/node.rs`.

3. **Linear search for hash prefix** - `get_destination()` does O(n) search when matching by hash prefix. With MAX_KNOWN_DESTINATIONS=100, this is acceptable but could be improved with a trie.

### Completed Improvements

| Improvement | Description |
|-------------|-------------|
| LRU cache eviction | Evicts oldest when destination cache is full |
| Link state checking | Check if link is activated before sending |
| Extract link helper | DRY "get or create link" pattern |
| Inbound link cleanup | Remove closed inbound links from cache |
| Message queueing | Queue messages for pending links, send on activation |
| Queue message TTL | Expire queued messages after 60s to prevent stale sends |

### Remaining Improvements

| Improvement | Description | Priority |
|-------------|-------------|----------|
| Batch broadcast sends | Create all packets then send in single lock hold | Low |
| O(1) LRU eviction | Replace O(n) scan with doubly-linked list | Low |
| Platform-specific stdin | Use non-blocking stdin on host for clean shutdown | Low |

---

## Code Quality Improvements

Quality issues identified during code review. Grouped by priority.

### High Priority

All high priority code quality issues have been addressed:
- ✅ Network task extracted to `spawn_network_task()` function
- ✅ Lock ordering violation fixed in `LinkEvent::Closed` handler
- ✅ Queue TTL logic extracted to `src/message_queue.rs` with 7 tests

### Medium Priority

| Issue | Location | Description |
|-------|----------|-------------|
| Complete chat state tests | `src/chat.rs:410-418` | Tests cannot construct valid AddressHash/DestinationDesc. Add fixtures and test eviction logic |
| Fix fragile hash formatting | `src/chat.rs:50-56` | Uses `format!("{:?}", hash)` which depends on Debug impl. Add proper hex helper |
| Consistent lock variable names | `src/bin/node.rs` | Use consistent naming: `transport_lock`, `links_cache_guard`, etc. |

### Low Priority

| Issue | Location | Description |
|-------|----------|-------------|
| Document magic numbers rationale | `src/bin/node.rs:53-72` | Add comments explaining why each constant was chosen |
| Merge link event handlers | `src/bin/node.rs:240-340` | Inbound/outbound handlers have structural overlap |
| Add queue metrics to NodeStats | `src/bin/node.rs` | Track queue_size, expired_messages for ESP32 memory monitoring |
| Remove unused `is_expired_after` | `src/message_queue.rs` | Method only used in tests, consider removing or documenting |
| Add TTL boundary tests | `src/message_queue.rs` | Test exact TTL boundary (at TTL vs just past TTL) |
| Clarify lock ordering docs | `src/bin/node.rs:20-29` | Explain partial order and when to acquire multiple locks |

---

## Documentation Improvements

Issues identified during documentation review.

### High Priority

All high priority documentation issues have been addressed:
- ✅ Skill references in CLAUDE.md made conditional
- ✅ Test count verified (166 after message_queue module added)
- ✅ Risk table in research-findings.md updated

### Medium Priority

| Issue | Location | Description |
|-------|----------|-------------|
| Consolidate testing docs | README, CLAUDE.md, testing-strategy.md, qemu-setup.md | Same test commands repeated 4 places. Keep summary in README, details in testing-strategy.md |
| Add memory constraints to README | `README.md` | 512KB SRAM constraint not mentioned. Add section with link to memory-analysis.md |
| Create status dashboard | New: `docs/STATUS.md` | No single view of completion %. Create checklist with links |
| Restructure dense section | `docs/implementation-guide.md:520-585` | Section 5 is 65 lines of mixed content. Add checklist summary |

### Low Priority

| Issue | Location | Description |
|-------|----------|-------------|
| Add architecture diagram to README | `README.md` | Best diagram is buried in implementation-guide.md:745-765 |
| Create troubleshooting guide | New: `docs/troubleshooting.md` | No docs for common problems (QEMU boot, WiFi, LoRa) |
| Standardize cross-references | All docs | Mix of relative paths, full paths, bare URLs |
| Standardize timestamps | All docs | "*Updated YYYY-MM-DD*" format inconsistent, meaning unclear |
| Add context to code examples | `docs/implementation-guide.md` | Large code blocks (30-50 lines) lack explanation |
