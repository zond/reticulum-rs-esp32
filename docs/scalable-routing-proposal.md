# Scalable Routing Proposal

## Problem: Announce Flooding on Internet-Connected Nodes

Reticulum's routing works well at community scale but has a fundamental scalability limit when many nodes connect via the internet (e.g., the testnet).

This document describes:
1. **Gateway filtering** - A simple mitigation we plan to implement
2. **DHT-based routing** - A full solution for global scale (future research)

### How Reticulum Routing Works

1. **Announces** propagate through all transport nodes, creating a spanning tree of paths
2. **Path requests** are answered by the first node that knows the destination
3. **Data packets** follow the established path

### What Scales

| Mechanism | Complexity | Notes |
|-----------|------------|-------|
| Path requests | O(hops to first knowledgeable node) | Stops when answered |
| Data routing | O(path length) | Direct forwarding |
| **Announces** | **O(n) per destination** | Floods to ALL transport nodes |

### The Bottleneck

Consider two local LoRa meshes connected via internet gateways:

```
┌───────────────────────────────┐       ┌───────────────────────────────┐
│            Mesh 1             │       │            Mesh 2             │
│                               │       │                               │
│  T1 ─── T2 ─── T3 ─── GW1 ◄───┼───────┼───► GW2 ─── T4 ─── T5 ─── T6 │
│   │      │      │             │  net  │              │      │      │  │
│  ...    ...    ...            │       │             ...    ...    ... │
│                               │       │                               │
│  ALL transport nodes store    │       │  ALL transport nodes store    │
│  ALL announces from Mesh 2    │       │  ALL announces from Mesh 1    │
└───────────────────────────────┘       └───────────────────────────────┘
```

When B announces in Mesh 2:
1. B's announce floods through T4, T5, T6... to GW2
2. GW2 forwards to internet peers (including GW1)
3. GW1 forwards into Mesh 1's LoRa interface
4. T3 receives, stores, forwards to T2
5. T2 receives, stores, forwards to T1
6. **Every transport node in Mesh 1 now stores B's announce**

This happens for every destination in every mesh:
- Every transport node everywhere stores every destination from everywhere
- A single announce floods through ALL meshes globally

Let N = total destinations, g = gateways, t = transport nodes per mesh:
- Each announce reaches all nodes, but only transport nodes store it
- Each transport node stores N entries (all destinations)
- Traffic per announce: O(g × t) transmissions

At scale (e.g., 1M destinations, 10K gateways, 50 transport nodes/mesh):
- Storage: 1M entries per transport node
- Traffic: each announce transmitted ~500K times (10K × 50)

## Mitigation: Gateway Announce Filtering (Planned)

A simple mitigation that protects local mesh nodes from global announce flooding.

> **Note:** This is planned for our ESP32 gateway implementation. We haven't yet
> implemented forwarding between LoRa, BLE, and WiFi/testnet - this document
> describes our intended approach. No community discussion has occurred yet.

### Concept

Gateways act as a "firewall" for announces:
- **Local → Internet:** Forward announces normally (gateway publishes local destinations)
- **Internet → Local:** Do NOT forward into local mesh (gateway stores but doesn't rebroadcast)

```
┌───────────────────────────────┐       ┌───────────────────────────────┐
│            Mesh 1             │       │            Mesh 2             │
│                               │       │                               │
│  T1 ─── T2 ─── T3 ─── GW1 ◄───┼───────┼───► GW2 ─── T4 ─── T5 ─── T6 │
│                               │  net  │                               │
│  Local nodes only store       │   ▲   │  Local nodes only store       │
│  LOCAL destinations           │   │   │  LOCAL destinations           │
│                               │   │   │                               │
└───────────────────────────────┘   │   └───────────────────────────────┘
                                    │
                          Announces flood here
                          (between gateways only)
```

### Why It Works

1. **A announces in Mesh 1** → floods locally → GW1 forwards to internet
2. **GW2 receives announce** → stores it locally, does NOT forward into Mesh 2
3. **B in Mesh 2 wants to reach A** → sends path request
4. **Path request reaches GW2** → GW2 knows path to A → responds
5. **B now has path:** B → T4 → GW2 → internet → GW1 → T3 → A
6. **Data flows normally** because gateway is on the path

### What This Solves

| Component | Before | After (filtering) |
|-----------|--------|-------------------|
| Local transport nodes | Store N entries (all global) | Store only local mesh |
| Gateway storage | O(N) | O(N) (unchanged) |
| Local mesh bandwidth | Flooded with global announces | Local traffic only |

### What This Doesn't Solve

- Gateways still flood announces to each other over internet
- Each gateway still stores O(N) entries (all global destinations)
- With millions of destinations, gateway storage grows linearly

### Why It's Good Enough For Now

- **Local mesh nodes are constrained** (ESP32: ~60KB free RAM)
- **Gateways can be beefy** (RPi, VPS, home server)
- Protects the weakest links in the network
- Zero infrastructure required - just gateway configuration
- Fully compatible with standard Reticulum

### Implementation

The gateway needs to:
1. Track which interface is "internet" vs "local mesh"
2. Store announces from internet interfaces normally
3. Not rebroadcast internet announces to local mesh interfaces
4. Still respond to path requests using stored internet announces

## Future Solution: DHT-Backed Gateway Routing

For true global scale (millions of gateways), announce flooding between gateways must also be eliminated. This section describes a more complex solution using a Distributed Hash Table.

### Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    Local Mesh (unchanged)                   │
│                                                             │
│  Announces flood locally as normal                          │
│  Path requests answered by local transport nodes            │
│  Gateway is just another transport node                     │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                    Gateway DHT Layer                        │
│                                                             │
│  Instead of flooding announces to all other gateways:       │
│  - Gateway publishes local destinations to DHT              │
│  - DHT maps: destination_hash → gateway_id                  │
│  - Path requests for unknown destinations query DHT         │
│                                                             │
│  Complexity: O(log n) lookups, O(log n) storage per node   │
└─────────────────────────────────────────────────────────────┘
```

### How It Works

This builds on the gateway filtering mitigation (see above). In addition:

**Gateway-to-gateway routing via DHT:**
1. When gateway sees a local announce, it publishes to DHT (instead of flooding):
   ```
   DHT.put(destination_hash, {gateway_id, timestamp, hops})
   ```

2. When gateway receives path request for unknown destination:
   ```
   gateway_id = DHT.get(destination_hash)
   if gateway_id:
       forward_path_request(gateway_id)
   ```

3. Response path is cached locally for future requests

### Scaling Comparison

| Metric | Filtering Only | Filtering + DHT |
|--------|----------------|-----------------|
| Storage per gateway | O(N) all destinations | O(log N) DHT + cache |
| Announce traffic (internet) | O(g²) per announce | O(1) per announce |
| Path lookup | O(1) cached | O(log N) DHT query |
| Lookup latency | Instant | ~100-500ms first lookup |
| Infrastructure needed | None | DHT bootstrap nodes |

Where N = total destinations, g = number of gateways.

### Trade-offs

**Advantages:**
- Scales to millions of gateways
- Reduces bandwidth between gateways by orders of magnitude
- Local mesh operation unchanged

**Disadvantages:**
- First path lookup has latency (DHT query)
- Requires DHT infrastructure or participation
- Gateway failure = temporary unreachability (until DHT updated)

## Implementation Options

### Option 1: BitTorrent Mainline DHT

Use existing BT DHT infrastructure for bootstrap and discovery.

- **Pros:** 25+ million nodes, zero infrastructure needed, proven
- **Cons:** UDP only, ~20 KB/hour overhead, not designed for this use case
- **Rust crate:** `mainline`

### Option 2: Custom Kademlia DHT

Run a separate DHT network for mesh routing.

- **Pros:** Optimized for our use case, lower overhead
- **Cons:** Requires bootstrap infrastructure
- **Rust crate:** `libp2p-kad` (heavy) or custom minimal implementation

### Option 3: Hybrid Approach

Use BT DHT for bootstrap, custom DHT for routing data.

```
┌─────────────────────────────────────────────────────────────┐
│                    Bootstrap Layer                          │
│                                                             │
│  BT DHT stores: mesh_network_infohash → [gateway_addrs]    │
│  Purpose: Find initial mesh DHT peers                       │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                    Mesh Routing DHT                         │
│                                                             │
│  Custom minimal Kademlia                                    │
│  Stores: destination_hash → gateway_id                      │
│  Participants: Internet gateways only                       │
└─────────────────────────────────────────────────────────────┘
```

## NAT Traversal Considerations

Most home gateways are behind NAT. Solutions:

1. **STUN:** Discover public IP:port (works ~60-70% of cases)
   - Free servers: `stun.l.google.com:19302`

2. **UDP hole punching:** Coordinated via DHT or relay
   - Both peers send simultaneously to each other's public addr

3. **Relay fallback:** For symmetric NAT (~30% of cases)
   - Requires 3-5 volunteer relay servers
   - Or use existing TURN infrastructure

## Compatibility

This proposal is **additive** - existing Reticulum behavior is unchanged:

- Local meshes work exactly as before
- Gateways that don't support DHT continue using announce flooding
- DHT-capable gateways can still participate in flooded announces
- Gradual migration possible

## Open Questions

1. Should DHT entries include path quality metrics (hops, latency)?
2. How to handle gateway mobility (IP changes)?
3. What's the right TTL for DHT entries?
4. Should we implement "anycast" for destinations reachable via multiple gateways?

## References

- [Reticulum Manual](https://reticulum.network/manual/)
- [Kademlia Paper](https://pdos.csail.mit.edu/~petar/papers/maymounkov-kademlia-lncs.pdf)
- [BitTorrent DHT (BEP 5)](https://www.bittorrent.org/beps/bep_0005.html)
- [mainline crate](https://crates.io/crates/mainline)
