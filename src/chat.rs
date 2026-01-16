//! Simple serial chat for testing node-to-node communication.
//!
//! Provides a command interface over USB serial for sending messages
//! between Reticulum nodes. Useful for testing LoRa connectivity.
//!
//! # Commands
//!
//! - `msg <dest_hash> <text>` - Send message to a specific destination
//! - `broadcast <text>` - Send message to all known destinations
//! - `list` - Show known destinations (from received announces)
//! - `status` - Show node status (identity, uptime, interfaces)
//! - `help` - Show available commands
//!
//! # Example Session
//!
//! ```text
//! > list
//! Known destinations:
//!   [0] a1b2c3d4 (seen 30s ago)
//!   [1] e5f6g7h8 (seen 5s ago)
//!
//! > msg 0 Hello from node A!
//! Sent to a1b2c3d4
//!
//! [e5f6g7h8]: Hello from node B!
//!
//! > broadcast Anyone there?
//! Sent to 2 destinations
//! ```

use log::info;
use reticulum::destination::DestinationDesc;
use reticulum::hash::AddressHash;
use std::collections::HashMap;
use std::time::Instant;

/// Maximum number of known destinations to cache.
/// Prevents memory exhaustion from announce flooding.
const MAX_KNOWN_DESTINATIONS: usize = 100;

/// Number of characters to show from hash for display.
/// 8 hex chars = 4 bytes = ~1 in 4 billion collision probability.
const DISPLAY_HASH_CHARS: usize = 8;

/// Format an address hash as a short lowercase hex display string.
fn format_hash_short(hash: &AddressHash) -> String {
    hash.to_hex_string()
        .chars()
        .take(DISPLAY_HASH_CHARS)
        .collect()
}

/// A known destination discovered via announce.
#[derive(Clone)]
pub struct KnownDestination {
    /// The destination's address hash.
    pub hash: AddressHash,
    /// Full destination descriptor (needed for creating links).
    pub descriptor: DestinationDesc,
    /// When we last saw an announce from this destination.
    pub last_seen: Instant,
    /// Display name (truncated hash for now).
    pub display_name: String,
}

impl std::fmt::Debug for KnownDestination {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KnownDestination")
            .field("hash", &self.hash)
            .field("last_seen", &self.last_seen)
            .field("display_name", &self.display_name)
            .finish_non_exhaustive()
    }
}

impl KnownDestination {
    /// Create a new known destination.
    pub fn new(hash: AddressHash, descriptor: DestinationDesc) -> Self {
        let display_name = format_hash_short(&hash);
        Self {
            hash,
            descriptor,
            last_seen: Instant::now(),
            display_name,
        }
    }

    /// How long ago we saw this destination (in seconds).
    pub fn seconds_ago(&self) -> u64 {
        self.last_seen.elapsed().as_secs()
    }
}

/// Manages known destinations and provides command parsing.
pub struct ChatState {
    /// Our own identity hash (for display).
    pub identity_hash: String,
    /// Known destinations indexed by short ID (0, 1, 2...).
    destinations: Vec<KnownDestination>,
    /// Map from address hash to index for quick lookup.
    hash_to_index: HashMap<AddressHash, usize>,
    /// When the node started.
    start_time: Instant,
}

impl ChatState {
    /// Create new chat state.
    pub fn new(identity_hash: String) -> Self {
        Self {
            identity_hash,
            destinations: Vec::new(),
            hash_to_index: HashMap::new(),
            start_time: Instant::now(),
        }
    }

    /// Add or update a known destination.
    ///
    /// Returns `true` if this is a new destination, `false` if updated existing.
    /// When the cache is full, evicts the least recently seen destination.
    pub fn add_destination(&mut self, hash: AddressHash, descriptor: DestinationDesc) -> bool {
        if let Some(&idx) = self.hash_to_index.get(&hash) {
            // Update existing - refresh last_seen time
            self.destinations[idx].last_seen = Instant::now();
            false
        } else {
            // Need to add new entry
            if self.destinations.len() >= MAX_KNOWN_DESTINATIONS {
                // Cache full - evict oldest (LRU)
                self.evict_oldest();
            }

            // Add new entry
            let idx = self.destinations.len();
            self.destinations
                .push(KnownDestination::new(hash, descriptor));
            self.hash_to_index.insert(hash, idx);
            info!(
                "[chat] New destination discovered: {}",
                self.destinations[idx].display_name
            );
            true
        }
    }

    /// Evict the least recently seen destination using swap-remove.
    ///
    /// Note: Finding the oldest entry is O(n) where n = destination count.
    /// This is acceptable for MAX_KNOWN_DESTINATIONS=100 on ESP32 (<1μs at 240MHz).
    /// A doubly-linked list would give O(1) but adds complexity. Consider if
    /// the limit increases significantly.
    ///
    /// The swap-remove maintains O(1) for the actual removal by swapping the
    /// oldest entry with the last, then popping from the end.
    fn evict_oldest(&mut self) {
        if self.destinations.is_empty() {
            return;
        }

        // Find index of oldest entry (minimum last_seen)
        let oldest_idx = self
            .destinations
            .iter()
            .enumerate()
            .min_by_key(|(_, d)| d.last_seen)
            .map(|(i, _)| i)
            .unwrap();

        let oldest_hash = self.destinations[oldest_idx].hash;
        let last_idx = self.destinations.len() - 1;

        if oldest_idx != last_idx {
            // Swap oldest with last, update the moved entry's index
            let last_hash = self.destinations[last_idx].hash;
            self.destinations.swap(oldest_idx, last_idx);
            self.hash_to_index.insert(last_hash, oldest_idx);
        }

        // Remove the last entry (which now contains the oldest data)
        self.destinations.pop();
        self.hash_to_index.remove(&oldest_hash);

        info!(
            "[chat] Evicted oldest destination: {}",
            format_hash_short(&oldest_hash)
        );
    }

    /// Get a destination by index or hash prefix.
    pub fn get_destination(&self, id: &str) -> Option<&KnownDestination> {
        // Try as index first
        if let Ok(idx) = id.parse::<usize>() {
            return self.destinations.get(idx);
        }

        // Try as hash prefix
        let id_lower = id.to_lowercase();
        self.destinations
            .iter()
            .find(|d| d.display_name.starts_with(&id_lower))
    }

    /// Get all known destinations.
    pub fn all_destinations(&self) -> &[KnownDestination] {
        &self.destinations
    }

    /// Get node uptime in seconds.
    pub fn uptime_secs(&self) -> u64 {
        self.start_time.elapsed().as_secs()
    }

    /// Format the list of known destinations.
    pub fn format_list(&self) -> String {
        if self.destinations.is_empty() {
            return "No known destinations. Wait for announces...".to_string();
        }

        let mut output = String::from("Known destinations:\n");
        for (idx, dest) in self.destinations.iter().enumerate() {
            output.push_str(&format!(
                "  [{}] {} (seen {}s ago)\n",
                idx,
                dest.display_name,
                dest.seconds_ago()
            ));
        }
        output
    }

    /// Format node status.
    pub fn format_status(&self) -> String {
        format!(
            "Node Status:\n  Identity: {}\n  Uptime: {}s\n  Known destinations: {}\n",
            self.identity_hash,
            self.uptime_secs(),
            self.destinations.len()
        )
    }
}

/// Parsed chat command.
#[derive(Debug)]
pub enum ChatCommand {
    /// Send message to specific destination.
    Message { dest_id: String, text: String },
    /// Broadcast to all known destinations.
    Broadcast { text: String },
    /// List known destinations.
    List,
    /// Show node status.
    Status,
    /// Show help.
    Help,
    /// Unknown or invalid command.
    Unknown(String),
}

impl ChatCommand {
    /// Parse a command from input line.
    pub fn parse(input: &str) -> Self {
        let input = input.trim();
        if input.is_empty() {
            return ChatCommand::Unknown(String::new());
        }

        let mut parts = input.splitn(2, ' ');
        let cmd = parts.next().unwrap_or("");
        let args = parts.next().unwrap_or("").trim();

        match cmd.to_lowercase().as_str() {
            "msg" | "m" | "send" => {
                let mut msg_parts = args.splitn(2, ' ');
                let dest_id = msg_parts.next().unwrap_or("").to_string();
                let text = msg_parts.next().unwrap_or("").to_string();
                if dest_id.is_empty() || text.is_empty() {
                    ChatCommand::Unknown("Usage: msg <dest_id> <message>".to_string())
                } else {
                    ChatCommand::Message { dest_id, text }
                }
            }
            "broadcast" | "bc" | "b" => {
                if args.is_empty() {
                    ChatCommand::Unknown("Usage: broadcast <message>".to_string())
                } else {
                    ChatCommand::Broadcast {
                        text: args.to_string(),
                    }
                }
            }
            "list" | "ls" | "l" => ChatCommand::List,
            "status" | "stat" | "s" => ChatCommand::Status,
            "help" | "h" | "?" => ChatCommand::Help,
            _ => ChatCommand::Unknown(format!(
                "Unknown command: {}. Type 'help' for commands.",
                cmd
            )),
        }
    }
}

/// Help text for available commands.
pub const HELP_TEXT: &str = r#"
Available commands:
  msg <id> <text>    Send message to destination (by index or hash prefix)
  broadcast <text>   Send message to all known destinations
  list               Show known destinations
  status             Show node status
  help               Show this help

Shortcuts: m=msg, b=broadcast, l=list, s=status, h=help

Examples:
  msg 0 Hello!       Send "Hello!" to destination [0]
  msg a1b2 Hi        Send "Hi" to destination starting with "a1b2"
  broadcast Anyone?  Send to all known destinations
"#;

/// Format an incoming message for display.
pub fn format_incoming_message(sender_hash: &AddressHash, message: &[u8]) -> String {
    let sender = format_hash_short(sender_hash);

    match std::str::from_utf8(message) {
        Ok(text) => format!("[{}]: {}", sender, text),
        Err(_) => format!("[{}]: <binary {} bytes>", sender, message.len()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use reticulum::destination::DestinationName;
    use reticulum::identity::Identity;
    use reticulum_rs_esp32_macros::esp32_test;

    /// Create a test AddressHash from a simple index.
    fn test_hash(index: u8) -> AddressHash {
        let mut bytes = [0u8; 16];
        bytes[0] = index;
        bytes[15] = index; // Make it more recognizable in hex
        AddressHash::new(bytes)
    }

    /// Create a test DestinationDesc.
    fn test_descriptor(index: u8) -> DestinationDesc {
        DestinationDesc {
            identity: Identity::default(),
            address_hash: test_hash(index),
            name: DestinationName::new("test", "dest"),
        }
    }

    #[esp32_test]
    fn test_parse_msg_command() {
        match ChatCommand::parse("msg 0 Hello world") {
            ChatCommand::Message { dest_id, text } => {
                assert_eq!(dest_id, "0");
                assert_eq!(text, "Hello world");
            }
            _ => panic!("Expected Message command"),
        }
    }

    #[esp32_test]
    fn test_parse_msg_shortcut() {
        match ChatCommand::parse("m a1b2 Test") {
            ChatCommand::Message { dest_id, text } => {
                assert_eq!(dest_id, "a1b2");
                assert_eq!(text, "Test");
            }
            _ => panic!("Expected Message command"),
        }
    }

    #[esp32_test]
    fn test_parse_broadcast() {
        match ChatCommand::parse("broadcast Hello everyone") {
            ChatCommand::Broadcast { text } => {
                assert_eq!(text, "Hello everyone");
            }
            _ => panic!("Expected Broadcast command"),
        }
    }

    #[esp32_test]
    fn test_parse_list() {
        assert!(matches!(ChatCommand::parse("list"), ChatCommand::List));
        assert!(matches!(ChatCommand::parse("ls"), ChatCommand::List));
        assert!(matches!(ChatCommand::parse("l"), ChatCommand::List));
    }

    #[esp32_test]
    fn test_parse_status() {
        assert!(matches!(ChatCommand::parse("status"), ChatCommand::Status));
        assert!(matches!(ChatCommand::parse("s"), ChatCommand::Status));
    }

    #[esp32_test]
    fn test_parse_help() {
        assert!(matches!(ChatCommand::parse("help"), ChatCommand::Help));
        assert!(matches!(ChatCommand::parse("?"), ChatCommand::Help));
    }

    #[esp32_test]
    fn test_parse_unknown() {
        assert!(matches!(ChatCommand::parse("foo"), ChatCommand::Unknown(_)));
    }

    #[esp32_test]
    fn test_parse_empty() {
        assert!(matches!(ChatCommand::parse(""), ChatCommand::Unknown(_)));
        assert!(matches!(ChatCommand::parse("   "), ChatCommand::Unknown(_)));
    }

    #[esp32_test]
    fn test_msg_missing_args() {
        assert!(matches!(ChatCommand::parse("msg"), ChatCommand::Unknown(_)));
        assert!(matches!(
            ChatCommand::parse("msg 0"),
            ChatCommand::Unknown(_)
        ));
    }

    #[esp32_test]
    fn test_chat_state_empty() {
        let state = ChatState::new("test_identity".to_string());
        assert_eq!(state.all_destinations().len(), 0);
        assert!(state.format_list().contains("No known destinations"));
        assert!(state.format_status().contains("test_identity"));
    }

    #[esp32_test]
    fn test_chat_state_add_destination() {
        let mut state = ChatState::new("test".to_string());

        let hash = test_hash(1);
        let desc = test_descriptor(1);
        state.add_destination(hash, desc);

        assert_eq!(state.all_destinations().len(), 1);
        let list = state.format_list();
        assert!(list.contains("[0]"));
        // Hash should start with "01" (first byte is 1)
        assert!(list.contains("01"));
    }

    #[esp32_test]
    fn test_chat_state_get_destination_by_index() {
        let mut state = ChatState::new("test".to_string());

        let hash = test_hash(42);
        let desc = test_descriptor(42);
        state.add_destination(hash, desc);

        // Should find by index "0"
        let found = state.get_destination("0");
        assert!(found.is_some());
        assert_eq!(found.unwrap().hash, hash);

        // Should not find by invalid index
        assert!(state.get_destination("1").is_none());
        assert!(state.get_destination("999").is_none());
    }

    #[esp32_test]
    fn test_chat_state_get_destination_by_hash_prefix() {
        let mut state = ChatState::new("test".to_string());

        let hash = test_hash(0xAB);
        let desc = test_descriptor(0xAB);
        state.add_destination(hash, desc);

        // Should find by hash prefix (first byte is 0xAB)
        let found = state.get_destination("ab");
        assert!(found.is_some());
        assert_eq!(found.unwrap().hash, hash);

        // Should not find by non-matching prefix
        assert!(state.get_destination("ff").is_none());
    }

    #[esp32_test]
    fn test_chat_state_lru_eviction() {
        let mut state = ChatState::new("test".to_string());

        // Add MAX_KNOWN_DESTINATIONS entries
        for i in 0..MAX_KNOWN_DESTINATIONS {
            state.add_destination(test_hash(i as u8), test_descriptor(i as u8));
        }
        assert_eq!(state.all_destinations().len(), MAX_KNOWN_DESTINATIONS);

        // Add one more - should evict the oldest
        let new_hash = test_hash(255);
        state.add_destination(new_hash, test_descriptor(255));

        // Should still be at max capacity
        assert_eq!(state.all_destinations().len(), MAX_KNOWN_DESTINATIONS);

        // The new entry should exist
        assert!(state.all_destinations().iter().any(|d| d.hash == new_hash));

        // The first entry (hash 0) should have been evicted
        let first_hash = test_hash(0);
        assert!(!state
            .all_destinations()
            .iter()
            .any(|d| d.hash == first_hash));
    }

    #[esp32_test]
    fn test_chat_state_update_existing() {
        let mut state = ChatState::new("test".to_string());

        let hash = test_hash(1);
        state.add_destination(hash, test_descriptor(1));

        // Add again with same hash - should update, not duplicate
        state.add_destination(hash, test_descriptor(1));

        assert_eq!(state.all_destinations().len(), 1);
    }
}
