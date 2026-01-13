# Future Work

Items planned for future implementation.

## BLE Configuration Expansion

Currently the BLE GATT service only configures WiFi credentials. This should be extended to support full node configuration:

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

A `/stats` HTTP endpoint for monitoring (see [implementation-guide.md](implementation-guide.md) section 6).

## Interrupt-Driven Radio

The LoRa radio driver currently uses polling. Switching to DIO1 interrupt would improve power efficiency. See TODO in `src/lora/radio.rs:138`.
