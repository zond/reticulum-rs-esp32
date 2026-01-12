//! BLE packet fragmentation and reassembly.
//!
//! BLE has a small MTU (typically 20-512 bytes after negotiation), while
//! Reticulum packets can be up to 500 bytes. This module handles splitting
//! large packets into fragments and reassembling them.
//!
//! # Fragment Format
//!
//! Each fragment has a 2-byte header:
//! ```text
//! [sequence: 1 byte][flags: 1 byte][payload: N bytes]
//! ```
//!
//! Flags:
//! - Bit 0: FIRST_FRAGMENT - This is the first fragment of a packet
//! - Bit 1: MORE_FRAGMENTS - More fragments follow this one
//!
//! # Limitations
//!
//! The current `Reassembler` does not track source addresses. This means:
//! - Multiple concurrent reassemblies from different sources may interfere
//! - For production use, the BLE layer should provide source addresses
//! - Use `with_limits(timeout, 1, max_frags)` to enforce single reassembly
//!
//! # Example
//!
//! ```
//! use reticulum_rs_esp32::ble::{Fragmenter, Reassembler};
//! use std::time::Duration;
//!
//! // Fragment a large packet
//! let mut fragmenter = Fragmenter::new(20); // 20-byte MTU
//! let packet = vec![0u8; 100]; // 100-byte packet
//! let fragments = fragmenter.fragment(&packet).unwrap();
//!
//! // Reassemble fragments
//! let mut reassembler = Reassembler::new(Duration::from_secs(5));
//! for fragment in fragments {
//!     if let Some(complete) = reassembler.add_fragment(fragment) {
//!         assert_eq!(complete, packet);
//!     }
//! }
//! ```

use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Header size in bytes (sequence + flags).
pub const HEADER_SIZE: usize = 2;

/// Flag indicating this is the first fragment of a packet.
pub const FLAG_FIRST_FRAGMENT: u8 = 0x01;

/// Flag indicating more fragments follow.
pub const FLAG_MORE_FRAGMENTS: u8 = 0x02;

/// Valid flag bits mask.
const VALID_FLAGS_MASK: u8 = FLAG_FIRST_FRAGMENT | FLAG_MORE_FRAGMENTS;

/// Maximum reasonable sequence distance (half the u8 space).
/// Used to distinguish forward progression from backward wraparound.
const MAX_SEQUENCE_DISTANCE: u8 = 128;

/// Default maximum number of concurrent pending reassemblies.
const DEFAULT_MAX_PENDING: usize = 8;

/// Default maximum fragments per packet.
const DEFAULT_MAX_FRAGMENTS: usize = 32;

/// A single fragment of a larger packet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fragment {
    /// Sequence number (0-255, wraps around).
    pub sequence: u8,
    /// Fragment flags.
    pub flags: u8,
    /// Payload data (without header).
    pub payload: Vec<u8>,
}

impl Fragment {
    /// Create a new fragment.
    pub fn new(sequence: u8, flags: u8, payload: Vec<u8>) -> Self {
        Self {
            sequence,
            flags,
            payload,
        }
    }

    /// Check if this is the first fragment of a packet.
    pub fn is_first(&self) -> bool {
        self.flags & FLAG_FIRST_FRAGMENT != 0
    }

    /// Check if more fragments follow this one.
    pub fn has_more(&self) -> bool {
        self.flags & FLAG_MORE_FRAGMENTS != 0
    }

    /// Check if flags are valid\  (only defined bits set).
    pub fn has_valid_flags(&self) -> bool {
        self.flags & !VALID_FLAGS_MASK == 0
    }

    /// Serialize fragment to bytes (header + payload).
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(HEADER_SIZE + self.payload.len());
        bytes.push(self.sequence);
        bytes.push(self.flags);
        bytes.extend_from_slice(&self.payload);
        bytes
    }

    /// Serialize fragment into provided buffer.
    ///
    /// Returns the number of bytes written, or error if buffer too small.
    pub fn write_to(&self, buf: &mut [u8]) -> Result<usize, FragmentError> {
        let total_len = HEADER_SIZE + self.payload.len();
        if buf.len() < total_len {
            return Err(FragmentError::BufferTooSmall);
        }
        buf[0] = self.sequence;
        buf[1] = self.flags;
        buf[HEADER_SIZE..total_len].copy_from_slice(&self.payload);
        Ok(total_len)
    }

    /// Deserialize fragment from bytes.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, FragmentError> {
        if bytes.len() < HEADER_SIZE {
            return Err(FragmentError::TooShort);
        }
        Ok(Self {
            sequence: bytes[0],
            flags: bytes[1],
            payload: bytes[HEADER_SIZE..].to_vec(),
        })
    }
}

/// Errors that can occur during fragmentation/reassembly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FragmentError {
    /// Fragment data is too short to contain header.
    TooShort,
    /// MTU is too small to fit header plus at least one byte.
    MtuTooSmall,
    /// Packet is empty.
    EmptyPacket,
    /// Buffer too small for serialization.
    BufferTooSmall,
    /// Missing fragment during reassembly.
    MissingFragment(u8),
    /// Invalid flags on fragment.
    InvalidFlags,
}

impl std::fmt::Display for FragmentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooShort => write!(f, "fragment too short"),
            Self::MtuTooSmall => write!(f, "MTU too small (minimum: {})", HEADER_SIZE + 1),
            Self::EmptyPacket => write!(f, "cannot fragment empty packet"),
            Self::BufferTooSmall => write!(f, "buffer too small for fragment"),
            Self::MissingFragment(seq) => write!(f, "missing fragment with sequence {}", seq),
            Self::InvalidFlags => write!(f, "invalid flags on fragment"),
        }
    }
}

impl std::error::Error for FragmentError {}

/// Splits large packets into BLE-sized fragments.
pub struct Fragmenter {
    /// Maximum fragment size (including header).
    mtu: usize,
    /// Next sequence number to use.
    next_sequence: u8,
}

impl Fragmenter {
    /// Create a new fragmenter with the given MTU.
    ///
    /// The MTU should be the BLE characteristic's maximum write size,
    /// typically 20-512 bytes depending on negotiated MTU.
    ///
    /// # Panics
    ///
    /// Panics if MTU is less than HEADER_SIZE + 1 (minimum 3 bytes).
    pub fn new(mtu: usize) -> Self {
        assert!(
            mtu > HEADER_SIZE,
            "MTU must be greater than header size ({})",
            HEADER_SIZE
        );
        Self {
            mtu,
            next_sequence: 0,
        }
    }

    /// Get the maximum payload size per fragment.
    pub fn max_payload(&self) -> usize {
        self.mtu - HEADER_SIZE
    }

    /// Fragment a packet into one or more fragments.
    ///
    /// Returns an iterator over fragments. The first fragment will have
    /// FLAG_FIRST_FRAGMENT set. All fragments except the last will have
    /// FLAG_MORE_FRAGMENTS set.
    pub fn fragment(&mut self, packet: &[u8]) -> Result<Vec<Fragment>, FragmentError> {
        if packet.is_empty() {
            return Err(FragmentError::EmptyPacket);
        }

        let max_payload = self.max_payload();
        let fragment_count = packet.len().div_ceil(max_payload);
        let mut fragments = Vec::with_capacity(fragment_count);
        let mut offset = 0;
        let mut is_first = true;

        while offset < packet.len() {
            let remaining = packet.len() - offset;
            let payload_len = remaining.min(max_payload);
            let has_more = offset + payload_len < packet.len();

            let mut flags = 0u8;
            if is_first {
                flags |= FLAG_FIRST_FRAGMENT;
                is_first = false;
            }
            if has_more {
                flags |= FLAG_MORE_FRAGMENTS;
            }

            let payload = packet[offset..offset + payload_len].to_vec();
            fragments.push(Fragment::new(self.next_sequence, flags, payload));

            self.next_sequence = self.next_sequence.wrapping_add(1);
            offset += payload_len;
        }

        Ok(fragments)
    }

    /// Check if a packet needs fragmentation for this MTU.
    pub fn needs_fragmentation(&self, packet_len: usize) -> bool {
        packet_len > self.max_payload()
    }
}

/// Key for identifying a pending packet reassembly.
///
/// Note: In a production implementation, this should include source address
/// to properly disambiguate concurrent reassemblies from different peers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct ReassemblyKey {
    /// Sequence number of the first fragment.
    first_sequence: u8,
}

/// State for a packet being reassembled.
#[derive(Debug)]
struct PendingPacket {
    /// Received fragments, keyed by sequence number.
    fragments: HashMap<u8, Vec<u8>>,
    /// Sequence number of the first fragment.
    first_sequence: u8,
    /// Sequence number of the last fragment (when known).
    last_sequence: Option<u8>,
    /// When this reassembly started.
    started: Instant,
}

impl PendingPacket {
    fn new(first_sequence: u8) -> Self {
        Self {
            fragments: HashMap::new(),
            first_sequence,
            last_sequence: None,
            started: Instant::now(),
        }
    }

    /// Check if all fragments have been received.
    fn is_complete(&self) -> bool {
        let Some(last_seq) = self.last_sequence else {
            return false;
        };

        // Calculate expected fragment count using wraparound-safe arithmetic:
        // last - first + 1, all in u8 with wrapping
        let expected = last_seq.wrapping_sub(self.first_sequence).wrapping_add(1) as usize;

        self.fragments.len() == expected
    }

    /// Assemble the complete packet from fragments.
    ///
    /// Returns error if any fragment is missing.
    fn assemble(&self) -> Result<Vec<u8>, FragmentError> {
        // Pre-calculate total size for efficient allocation
        let total_size: usize = self.fragments.values().map(|v| v.len()).sum();
        let mut result = Vec::with_capacity(total_size);

        let mut seq = self.first_sequence;
        loop {
            let payload = self
                .fragments
                .get(&seq)
                .ok_or(FragmentError::MissingFragment(seq))?;
            result.extend_from_slice(payload);

            if Some(seq) == self.last_sequence {
                break;
            }
            seq = seq.wrapping_add(1);
        }

        Ok(result)
    }
}

/// Reassembles fragments back into complete packets.
///
/// # Memory Safety
///
/// The reassembler has configurable limits to prevent memory exhaustion:
/// - `max_pending`: Maximum concurrent reassemblies (default: 8)
/// - `max_fragments_per_packet`: Maximum fragments per packet (default: 32)
///
/// When limits are exceeded, oldest entries are evicted.
pub struct Reassembler {
    /// Pending packet reassemblies.
    pending: HashMap<ReassemblyKey, PendingPacket>,
    /// Timeout for incomplete packets.
    timeout: Duration,
    /// Maximum number of concurrent pending reassemblies.
    max_pending: usize,
    /// Maximum fragments allowed per packet.
    max_fragments_per_packet: usize,
}

impl Reassembler {
    /// Create a new reassembler with the given timeout and default limits.
    ///
    /// Default limits:
    /// - max_pending: 8 concurrent reassemblies
    /// - max_fragments_per_packet: 32 fragments
    ///
    /// Incomplete packets will be discarded after the timeout.
    pub fn new(timeout: Duration) -> Self {
        Self::with_limits(timeout, DEFAULT_MAX_PENDING, DEFAULT_MAX_FRAGMENTS)
    }

    /// Create a new reassembler with custom limits.
    ///
    /// # Arguments
    ///
    /// * `timeout` - How long to wait for incomplete packets
    /// * `max_pending` - Maximum concurrent reassemblies (prevents memory exhaustion)
    /// * `max_fragments_per_packet` - Maximum fragments per packet
    pub fn with_limits(
        timeout: Duration,
        max_pending: usize,
        max_fragments_per_packet: usize,
    ) -> Self {
        Self {
            pending: HashMap::new(),
            timeout,
            max_pending,
            max_fragments_per_packet,
        }
    }

    /// Add a fragment and return the complete packet if reassembly is done.
    ///
    /// Returns `Some(packet)` when a packet is fully reassembled,
    /// `None` if more fragments are needed or fragment was rejected.
    ///
    /// Fragments are rejected if:
    /// - They have invalid flags
    /// - The reassembly would exceed fragment limits
    /// - No matching reassembly exists for non-first fragments
    pub fn add_fragment(&mut self, fragment: Fragment) -> Option<Vec<u8>> {
        // Validate flags
        if !fragment.has_valid_flags() {
            return None;
        }

        // Clean up expired entries
        self.cleanup_expired();

        if fragment.is_first() {
            // Single-fragment packet - return immediately
            if !fragment.has_more() {
                return Some(fragment.payload);
            }

            // Start a new reassembly
            let key = ReassemblyKey {
                first_sequence: fragment.sequence,
            };

            // Enforce max_pending limit by evicting oldest if needed
            if self.pending.len() >= self.max_pending {
                if let Some(oldest_key) = self.find_oldest_pending() {
                    self.pending.remove(&oldest_key);
                }
            }

            // Don't overwrite existing reassembly with same first sequence
            if self.pending.contains_key(&key) {
                return None;
            }

            let mut pending = PendingPacket::new(fragment.sequence);
            pending
                .fragments
                .insert(fragment.sequence, fragment.payload);
            self.pending.insert(key, pending);
            None
        } else {
            // Find the pending reassembly this fragment belongs to
            let key = self.find_key_for_fragment(&fragment)?;

            let pending = self.pending.get_mut(&key)?;

            // Enforce fragment limit
            if pending.fragments.len() >= self.max_fragments_per_packet {
                self.pending.remove(&key);
                return None;
            }

            let has_more = fragment.has_more();
            let sequence = fragment.sequence;
            pending.fragments.insert(sequence, fragment.payload);

            if !has_more {
                pending.last_sequence = Some(sequence);
            }

            if pending.is_complete() {
                // Use ok() to silently handle missing fragments (shouldn't happen if is_complete is correct)
                let packet = pending.assemble().ok()?;
                self.pending.remove(&key);
                Some(packet)
            } else {
                None
            }
        }
    }

    /// Find the reassembly key for a non-first fragment.
    ///
    /// This is a simplified approach that works when fragments arrive in order
    /// or when there's only one pending reassembly. For production use with
    /// multiple concurrent sources, the BLE layer should provide source addresses.
    fn find_key_for_fragment(&self, fragment: &Fragment) -> Option<ReassemblyKey> {
        for key in self.pending.keys() {
            // Check if this fragment's sequence is within reasonable range
            // of the first fragment's sequence (within MAX_SEQUENCE_DISTANCE)
            let seq_diff = fragment.sequence.wrapping_sub(key.first_sequence);
            if seq_diff > 0 && seq_diff < MAX_SEQUENCE_DISTANCE {
                return Some(*key);
            }
        }
        None
    }

    /// Find the oldest pending reassembly for eviction.
    fn find_oldest_pending(&self) -> Option<ReassemblyKey> {
        self.pending
            .iter()
            .min_by_key(|(_, p)| p.started)
            .map(|(k, _)| *k)
    }

    /// Remove expired pending reassemblies.
    fn cleanup_expired(&mut self) {
        let now = Instant::now();
        self.pending
            .retain(|_, pending| now.duration_since(pending.started) < self.timeout);
    }

    /// Get the number of pending reassemblies.
    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }

    /// Clear all pending reassemblies.
    pub fn clear(&mut self) {
        self.pending.clear();
    }
}

#[cfg(feature = "tap-tests")]
mod tap_tests {
    use super::*;
    use reticulum_rs_esp32_macros::tap_test;

    // ==================== Fragment Tests ====================

    #[tap_test]
    fn test_fragment_serialize_deserialize() {
        let fragment = Fragment::new(42, FLAG_FIRST_FRAGMENT | FLAG_MORE_FRAGMENTS, vec![1, 2, 3]);
        let bytes = fragment.to_bytes();

        assert_eq!(bytes, vec![42, 0x03, 1, 2, 3]);

        let decoded = Fragment::from_bytes(&bytes).unwrap();
        assert_eq!(decoded, fragment);
    }

    #[tap_test]
    fn test_fragment_write_to_buffer() {
        let fragment = Fragment::new(1, FLAG_FIRST_FRAGMENT, vec![10, 20, 30]);
        let mut buf = [0u8; 10];

        let len = fragment.write_to(&mut buf).unwrap();
        assert_eq!(len, 5);
        assert_eq!(&buf[..5], &[1, FLAG_FIRST_FRAGMENT, 10, 20, 30]);

        // Buffer too small
        let mut small_buf = [0u8; 2];
        assert_eq!(
            fragment.write_to(&mut small_buf),
            Err(FragmentError::BufferTooSmall)
        );
    }

    #[tap_test]
    fn test_fragment_flags() {
        let first = Fragment::new(0, FLAG_FIRST_FRAGMENT, vec![]);
        assert!(first.is_first());
        assert!(!first.has_more());
        assert!(first.has_valid_flags());

        let middle = Fragment::new(1, FLAG_MORE_FRAGMENTS, vec![]);
        assert!(!middle.is_first());
        assert!(middle.has_more());
        assert!(middle.has_valid_flags());

        let first_with_more = Fragment::new(0, FLAG_FIRST_FRAGMENT | FLAG_MORE_FRAGMENTS, vec![]);
        assert!(first_with_more.is_first());
        assert!(first_with_more.has_more());
        assert!(first_with_more.has_valid_flags());

        let last = Fragment::new(2, 0, vec![]);
        assert!(!last.is_first());
        assert!(!last.has_more());
        assert!(last.has_valid_flags());

        // Invalid flags
        let invalid = Fragment::new(0, 0xFF, vec![]);
        assert!(!invalid.has_valid_flags());

        let invalid2 = Fragment::new(0, 0x04, vec![]);
        assert!(!invalid2.has_valid_flags());
    }

    #[tap_test]
    fn test_fragment_from_bytes_too_short() {
        assert_eq!(Fragment::from_bytes(&[]), Err(FragmentError::TooShort));
        assert_eq!(Fragment::from_bytes(&[0]), Err(FragmentError::TooShort));
        // Exactly header size is OK (empty payload)
        assert!(Fragment::from_bytes(&[0, 0]).is_ok());
    }

    // ==================== Fragmenter Tests ====================

    #[tap_test]
    fn test_fragmenter_single_fragment() {
        let mut fragmenter = Fragmenter::new(20);
        let packet = vec![1, 2, 3, 4, 5]; // 5 bytes, fits in one fragment

        let fragments = fragmenter.fragment(&packet).unwrap();
        assert_eq!(fragments.len(), 1);
        assert!(fragments[0].is_first());
        assert!(!fragments[0].has_more());
        assert_eq!(fragments[0].payload, packet);
    }

    #[tap_test]
    fn test_fragmenter_multiple_fragments() {
        let mut fragmenter = Fragmenter::new(5); // 5 byte MTU = 3 byte payload
        let packet = vec![1, 2, 3, 4, 5, 6, 7, 8]; // 8 bytes = 3 fragments

        let fragments = fragmenter.fragment(&packet).unwrap();
        assert_eq!(fragments.len(), 3);

        // First fragment
        assert!(fragments[0].is_first());
        assert!(fragments[0].has_more());
        assert_eq!(fragments[0].payload, vec![1, 2, 3]);

        // Middle fragment
        assert!(!fragments[1].is_first());
        assert!(fragments[1].has_more());
        assert_eq!(fragments[1].payload, vec![4, 5, 6]);

        // Last fragment
        assert!(!fragments[2].is_first());
        assert!(!fragments[2].has_more());
        assert_eq!(fragments[2].payload, vec![7, 8]);
    }

    #[tap_test]
    fn test_fragmenter_exact_fit() {
        let mut fragmenter = Fragmenter::new(5); // 3 byte payload
        let packet = vec![1, 2, 3, 4, 5, 6]; // Exactly 2 fragments

        let fragments = fragmenter.fragment(&packet).unwrap();
        assert_eq!(fragments.len(), 2);
        assert_eq!(fragments[0].payload, vec![1, 2, 3]);
        assert_eq!(fragments[1].payload, vec![4, 5, 6]);
    }

    #[tap_test]
    fn test_fragmenter_empty_packet() {
        let mut fragmenter = Fragmenter::new(20);
        assert_eq!(fragmenter.fragment(&[]), Err(FragmentError::EmptyPacket));
    }

    #[tap_test]
    fn test_fragmenter_sequence_increment() {
        let mut fragmenter = Fragmenter::new(5);

        let frags1 = fragmenter.fragment(&[1, 2, 3]).unwrap();
        let frags2 = fragmenter.fragment(&[4, 5, 6]).unwrap();

        // Sequences should be consecutive
        assert_eq!(frags1[0].sequence + 1, frags2[0].sequence);
    }

    #[tap_test]
    fn test_fragmenter_sequence_wraparound() {
        let mut fragmenter = Fragmenter::new(5);
        fragmenter.next_sequence = 254;

        let fragments = fragmenter.fragment(&[1, 2, 3, 4, 5, 6, 7, 8, 9]).unwrap();
        assert_eq!(fragments[0].sequence, 254);
        assert_eq!(fragments[1].sequence, 255);
        assert_eq!(fragments[2].sequence, 0); // Wrapped
    }

    #[tap_test]
    fn test_fragmenter_max_payload() {
        let fragmenter = Fragmenter::new(20);
        assert_eq!(fragmenter.max_payload(), 18);

        let fragmenter = Fragmenter::new(512);
        assert_eq!(fragmenter.max_payload(), 510);
    }

    #[tap_test]
    fn test_fragmenter_needs_fragmentation() {
        let fragmenter = Fragmenter::new(20); // 18 byte payload

        assert!(!fragmenter.needs_fragmentation(10));
        assert!(!fragmenter.needs_fragmentation(18));
        assert!(fragmenter.needs_fragmentation(19));
        assert!(fragmenter.needs_fragmentation(100));
    }

    #[tap_test(should_panic = "MTU must be greater than header size")]
    fn test_fragmenter_mtu_too_small() {
        Fragmenter::new(2); // Header is 2 bytes, need at least 3
    }

    // ==================== Reassembler Tests ====================

    #[tap_test]
    fn test_reassembler_single_fragment() {
        let mut reassembler = Reassembler::new(Duration::from_secs(5));
        let fragment = Fragment::new(0, FLAG_FIRST_FRAGMENT, vec![1, 2, 3]);

        let result = reassembler.add_fragment(fragment);
        assert_eq!(result, Some(vec![1, 2, 3]));
        assert_eq!(reassembler.pending_count(), 0);
    }

    #[tap_test]
    fn test_reassembler_multiple_fragments() {
        let mut reassembler = Reassembler::new(Duration::from_secs(5));

        let frag1 = Fragment::new(0, FLAG_FIRST_FRAGMENT | FLAG_MORE_FRAGMENTS, vec![1, 2]);
        let frag2 = Fragment::new(1, FLAG_MORE_FRAGMENTS, vec![3, 4]);
        let frag3 = Fragment::new(2, 0, vec![5, 6]);

        assert_eq!(reassembler.add_fragment(frag1), None);
        assert_eq!(reassembler.pending_count(), 1);

        assert_eq!(reassembler.add_fragment(frag2), None);
        assert_eq!(reassembler.pending_count(), 1);

        let result = reassembler.add_fragment(frag3);
        assert_eq!(result, Some(vec![1, 2, 3, 4, 5, 6]));
        assert_eq!(reassembler.pending_count(), 0);
    }

    #[tap_test]
    fn test_reassembler_out_of_order() {
        let mut reassembler = Reassembler::new(Duration::from_secs(5));

        // Send first, then last, then middle
        let frag1 = Fragment::new(0, FLAG_FIRST_FRAGMENT | FLAG_MORE_FRAGMENTS, vec![1, 2]);
        let frag3 = Fragment::new(2, 0, vec![5, 6]);
        let frag2 = Fragment::new(1, FLAG_MORE_FRAGMENTS, vec![3, 4]);

        assert_eq!(reassembler.add_fragment(frag1), None);
        assert_eq!(reassembler.add_fragment(frag3), None); // Have first and last
        let result = reassembler.add_fragment(frag2); // Complete!
        assert_eq!(result, Some(vec![1, 2, 3, 4, 5, 6]));
    }

    #[tap_test]
    fn test_reassembler_duplicate_fragment() {
        let mut reassembler = Reassembler::new(Duration::from_secs(5));

        let frag1 = Fragment::new(0, FLAG_FIRST_FRAGMENT | FLAG_MORE_FRAGMENTS, vec![1, 2]);
        let frag1_dup = Fragment::new(0, FLAG_FIRST_FRAGMENT | FLAG_MORE_FRAGMENTS, vec![1, 2]);
        let frag2 = Fragment::new(1, 0, vec![3, 4]);

        assert_eq!(reassembler.add_fragment(frag1), None);
        assert_eq!(reassembler.add_fragment(frag1_dup), None); // Duplicate rejected
        let result = reassembler.add_fragment(frag2);
        assert_eq!(result, Some(vec![1, 2, 3, 4]));
    }

    #[tap_test]
    fn test_reassembler_orphan_fragment() {
        let mut reassembler = Reassembler::new(Duration::from_secs(5));

        // Non-first fragment with no pending reassembly
        let orphan = Fragment::new(5, FLAG_MORE_FRAGMENTS, vec![1, 2, 3]);
        assert_eq!(reassembler.add_fragment(orphan), None);
        assert_eq!(reassembler.pending_count(), 0);
    }

    #[tap_test]
    fn test_reassembler_invalid_flags() {
        let mut reassembler = Reassembler::new(Duration::from_secs(5));

        // Fragment with undefined flags
        let invalid = Fragment::new(0, 0xFF, vec![1, 2, 3]);
        assert_eq!(reassembler.add_fragment(invalid), None);
        assert_eq!(reassembler.pending_count(), 0);
    }

    #[tap_test]
    fn test_reassembler_max_pending_limit() {
        let mut reassembler = Reassembler::with_limits(Duration::from_secs(5), 2, 32);

        // Start 3 reassemblies (one over limit)
        reassembler.add_fragment(Fragment::new(
            0,
            FLAG_FIRST_FRAGMENT | FLAG_MORE_FRAGMENTS,
            vec![1],
        ));
        reassembler.add_fragment(Fragment::new(
            10,
            FLAG_FIRST_FRAGMENT | FLAG_MORE_FRAGMENTS,
            vec![2],
        ));
        reassembler.add_fragment(Fragment::new(
            20,
            FLAG_FIRST_FRAGMENT | FLAG_MORE_FRAGMENTS,
            vec![3],
        ));

        // Should have evicted oldest
        assert_eq!(reassembler.pending_count(), 2);
    }

    #[tap_test]
    fn test_reassembler_max_fragments_limit() {
        let mut reassembler = Reassembler::with_limits(Duration::from_secs(5), 8, 2);

        // Start reassembly with first fragment
        reassembler.add_fragment(Fragment::new(
            0,
            FLAG_FIRST_FRAGMENT | FLAG_MORE_FRAGMENTS,
            vec![1],
        ));
        // Add second fragment
        reassembler.add_fragment(Fragment::new(1, FLAG_MORE_FRAGMENTS, vec![2]));
        // Third fragment exceeds limit - should drop entire reassembly
        reassembler.add_fragment(Fragment::new(2, 0, vec![3]));

        assert_eq!(reassembler.pending_count(), 0);
    }

    #[tap_test]
    fn test_reassembler_clear() {
        let mut reassembler = Reassembler::new(Duration::from_secs(5));

        let frag = Fragment::new(0, FLAG_FIRST_FRAGMENT | FLAG_MORE_FRAGMENTS, vec![1, 2]);
        reassembler.add_fragment(frag);
        assert_eq!(reassembler.pending_count(), 1);

        reassembler.clear();
        assert_eq!(reassembler.pending_count(), 0);
    }

    // ==================== Integration Tests ====================

    #[tap_test]
    fn test_fragment_and_reassemble_roundtrip() {
        let mut fragmenter = Fragmenter::new(10); // 8 byte payload
        let mut reassembler = Reassembler::new(Duration::from_secs(5));

        let original = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15];

        let fragments = fragmenter.fragment(&original).unwrap();
        assert!(fragments.len() > 1);

        let mut result = None;
        for fragment in fragments {
            result = reassembler.add_fragment(fragment);
        }

        assert_eq!(result, Some(original));
    }

    #[tap_test]
    fn test_multiple_concurrent_reassemblies() {
        let mut fragmenter = Fragmenter::new(5);
        let mut reassembler = Reassembler::new(Duration::from_secs(5));

        let packet1 = vec![1, 2, 3, 4, 5, 6];
        let packet2 = vec![10, 20, 30, 40, 50, 60];

        let frags1 = fragmenter.fragment(&packet1).unwrap();
        let frags2 = fragmenter.fragment(&packet2).unwrap();

        // Interleave fragments (send first of each, then rest)
        reassembler.add_fragment(frags1[0].clone());
        reassembler.add_fragment(frags2[0].clone());
        assert_eq!(reassembler.pending_count(), 2);

        // Complete packet 1
        let result1 = reassembler.add_fragment(frags1[1].clone());
        assert_eq!(result1, Some(packet1));

        // Complete packet 2
        let result2 = reassembler.add_fragment(frags2[1].clone());
        assert_eq!(result2, Some(packet2));

        assert_eq!(reassembler.pending_count(), 0);
    }

    #[tap_test]
    fn test_large_packet_fragmentation() {
        let mut fragmenter = Fragmenter::new(20); // Typical BLE default MTU
        let mut reassembler = Reassembler::new(Duration::from_secs(5));

        // 500 byte Reticulum packet
        let original: Vec<u8> = (0..=255).cycle().take(500).collect();

        let fragments = fragmenter.fragment(&original).unwrap();

        // Should need ceil(500 / 18) = 28 fragments
        assert_eq!(fragments.len(), 28);

        let mut result = None;
        for fragment in fragments {
            result = reassembler.add_fragment(fragment);
        }

        assert_eq!(result.as_ref().map(|v| v.len()), Some(500));
        assert_eq!(result, Some(original));
    }

    #[tap_test]
    fn test_sequence_wraparound_in_reassembly() {
        let mut fragmenter = Fragmenter::new(5);
        fragmenter.next_sequence = 254;

        let mut reassembler = Reassembler::new(Duration::from_secs(5));

        let packet = vec![1, 2, 3, 4, 5, 6, 7, 8, 9]; // 3 fragments

        let fragments = fragmenter.fragment(&packet).unwrap();
        assert_eq!(fragments[0].sequence, 254);
        assert_eq!(fragments[1].sequence, 255);
        assert_eq!(fragments[2].sequence, 0);

        let mut result = None;
        for fragment in fragments {
            result = reassembler.add_fragment(fragment);
        }

        assert_eq!(result, Some(packet));
    }
}
