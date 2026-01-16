//! Message queue for pending link messages.
//!
//! When messages are sent to a destination before the link is fully established,
//! they are queued and automatically sent when the link activates.

use std::time::{Duration, Instant};

/// Time-to-live for queued messages. Messages older than this are dropped
/// to prevent stale messages from being sent if a link takes too long to
/// establish or never activates.
pub const QUEUE_MESSAGE_TTL: Duration = Duration::from_secs(60);

/// Maximum queued messages per destination to prevent memory exhaustion.
pub const MAX_QUEUED_MESSAGES_PER_DEST: usize = 5;

/// A message queued for a pending link.
#[derive(Clone, Debug)]
pub struct QueuedMessage {
    /// The message text.
    text: String,
    /// When the message was queued.
    queued_at: Instant,
}

impl QueuedMessage {
    /// Create a new queued message with the current timestamp.
    pub fn new(text: String) -> Self {
        Self {
            text,
            queued_at: Instant::now(),
        }
    }

    /// Create a queued message with a specific timestamp (for testing).
    #[cfg(test)]
    pub fn with_timestamp(text: String, queued_at: Instant) -> Self {
        Self { text, queued_at }
    }

    /// Returns the message text.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Returns when the message was queued.
    pub fn queued_at(&self) -> Instant {
        self.queued_at
    }

    /// Returns true if this message has expired based on QUEUE_MESSAGE_TTL.
    pub fn is_expired(&self) -> bool {
        self.queued_at.elapsed() > QUEUE_MESSAGE_TTL
    }

    /// Returns true if this message would be expired after the given duration.
    pub fn is_expired_after(&self, duration: Duration) -> bool {
        self.queued_at.elapsed() > duration
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_message_not_expired() {
        let msg = QueuedMessage::new("hello".to_string());
        assert!(!msg.is_expired());
        assert_eq!(msg.text(), "hello");
    }

    #[test]
    fn test_message_expires_after_ttl() {
        // Create a message that appears to be queued in the past
        let old_time = Instant::now() - QUEUE_MESSAGE_TTL - Duration::from_secs(1);
        let msg = QueuedMessage::with_timestamp("old message".to_string(), old_time);
        assert!(msg.is_expired());
    }

    #[test]
    fn test_message_not_expired_before_ttl() {
        // Create a message that appears to be queued recently
        let recent_time = Instant::now() - QUEUE_MESSAGE_TTL + Duration::from_secs(10);
        let msg = QueuedMessage::with_timestamp("recent message".to_string(), recent_time);
        assert!(!msg.is_expired());
    }

    #[test]
    fn test_is_expired_after_custom_duration() {
        let msg = QueuedMessage::new("test".to_string());
        // A brand new message shouldn't be expired after 1 second
        assert!(!msg.is_expired_after(Duration::from_secs(1)));

        // Create an older message
        let old_time = Instant::now() - Duration::from_secs(5);
        let old_msg = QueuedMessage::with_timestamp("old".to_string(), old_time);
        // Should be expired after a 3 second threshold
        assert!(old_msg.is_expired_after(Duration::from_secs(3)));
        // Should not be expired after a 10 second threshold
        assert!(!old_msg.is_expired_after(Duration::from_secs(10)));
    }

    #[test]
    fn test_queue_constants() {
        // Verify constants are reasonable
        assert_eq!(QUEUE_MESSAGE_TTL, Duration::from_secs(60));
        assert_eq!(MAX_QUEUED_MESSAGES_PER_DEST, 5);
    }

    #[test]
    fn test_message_clone() {
        let msg = QueuedMessage::new("test".to_string());
        let cloned = msg.clone();
        assert_eq!(msg.text(), cloned.text());
        // Cloned timestamp should be the same
        assert_eq!(msg.queued_at(), cloned.queued_at());
    }

    #[test]
    fn test_message_debug() {
        let msg = QueuedMessage::new("debug test".to_string());
        let debug_str = format!("{:?}", msg);
        assert!(debug_str.contains("debug test"));
    }
}
