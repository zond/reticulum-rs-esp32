//! Duty cycle limiter using token bucket algorithm.
//!
//! Tracks airtime budget in microseconds. Budget refills continuously
//! over the window duration, allowing bursty transmissions while
//! maintaining average duty cycle compliance.
//!
//! # Example
//!
//! ```
//! use std::time::Duration;
//! use reticulum_rs_esp32::lora::DutyCycleLimiter;
//!
//! // 1% duty cycle over 1 hour (EU 868 MHz band)
//! let mut limiter = DutyCycleLimiter::new(1.0, Duration::from_secs(3600));
//!
//! // Try to transmit (airtime in microseconds)
//! let airtime_us = 100_000; // 100ms transmission
//! if limiter.try_consume(airtime_us) {
//!     println!("Transmission allowed");
//! } else {
//!     println!("Duty cycle exceeded, packet dropped");
//! }
//! ```

use std::time::{Duration, Instant};

/// Duty cycle limiter using token bucket algorithm.
///
/// This limiter ensures LoRa transmissions comply with regulatory duty cycle
/// limits (e.g., 1% in EU 868 MHz band, 10% in US 915 MHz band).
///
/// The token bucket algorithm allows bursty traffic while maintaining the
/// average duty cycle over the configured time window.
pub struct DutyCycleLimiter {
    /// Maximum budget in microseconds (total allowed airtime per window)
    budget_us: u64,
    /// Remaining budget in microseconds
    remaining_us: u64,
    /// Last time we refilled the budget
    last_refill: Instant,
    /// Window duration for duty cycle calculation
    window: Duration,
}

impl DutyCycleLimiter {
    /// Create a new duty cycle limiter.
    ///
    /// # Arguments
    ///
    /// * `duty_cycle_percent` - Duty cycle limit as percentage (e.g., 1.0 for 1%)
    /// * `window` - Time window for duty cycle calculation (e.g., 1 hour)
    ///
    /// # Example
    ///
    /// ```
    /// use std::time::Duration;
    /// use reticulum_rs_esp32::DutyCycleLimiter;
    ///
    /// // EU regulations: 1% duty cycle over 1 hour
    /// let eu_limiter = DutyCycleLimiter::new(1.0, Duration::from_secs(3600));
    ///
    /// // US regulations: typically 10% or more relaxed
    /// let us_limiter = DutyCycleLimiter::new(10.0, Duration::from_secs(3600));
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

    /// Attempt to consume airtime budget.
    ///
    /// Returns `true` if transmission is allowed (budget was consumed),
    /// `false` if duty cycle would be exceeded (budget unchanged).
    ///
    /// # Arguments
    ///
    /// * `airtime_us` - Required airtime in microseconds
    pub fn try_consume(&mut self, airtime_us: u64) -> bool {
        self.refill();
        if self.remaining_us >= airtime_us {
            self.remaining_us -= airtime_us;
            true
        } else {
            false
        }
    }

    /// Get remaining budget in microseconds.
    pub fn remaining(&mut self) -> u64 {
        self.refill();
        self.remaining_us
    }

    /// Get remaining budget as percentage of total.
    pub fn remaining_percent(&mut self) -> f32 {
        self.refill();
        if self.budget_us == 0 {
            return 0.0;
        }
        (self.remaining_us as f64 / self.budget_us as f64 * 100.0) as f32
    }

    /// Get the maximum budget in microseconds.
    pub fn budget(&self) -> u64 {
        self.budget_us
    }

    /// Refill budget based on elapsed time.
    fn refill(&mut self) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill);

        // Calculate refill: budget_us * (elapsed / window)
        // Use u128 to avoid overflow in intermediate calculation
        let window_us = self.window.as_micros();
        if window_us == 0 {
            return;
        }

        let refill_amount = (self.budget_us as u128 * elapsed.as_micros() / window_us) as u64;

        if refill_amount > 0 {
            self.remaining_us = (self.remaining_us + refill_amount).min(self.budget_us);
            self.last_refill = now;
        }
    }
}

#[cfg(feature = "tap-tests")]
mod tap_tests {
    use super::*;
    use reticulum_rs_esp32_macros::tap_test;

    #[tap_test]
    fn test_new_limiter_has_full_budget() {
        let limiter = DutyCycleLimiter::new(1.0, Duration::from_secs(3600));
        // 1% of 1 hour = 36 seconds = 36_000_000 microseconds
        assert_eq!(limiter.budget(), 36_000_000);
    }

    #[tap_test]
    fn test_consume_reduces_budget() {
        let mut limiter = DutyCycleLimiter::new(1.0, Duration::from_secs(3600));
        let initial = limiter.remaining();

        assert!(limiter.try_consume(1_000_000)); // 1 second
        assert_eq!(limiter.remaining(), initial - 1_000_000);
    }

    #[tap_test]
    fn test_consume_fails_when_exceeded() {
        let mut limiter = DutyCycleLimiter::new(1.0, Duration::from_secs(3600));
        let budget = limiter.budget();

        // Consume entire budget
        assert!(limiter.try_consume(budget));

        // Next consumption should fail
        assert!(!limiter.try_consume(1));

        // Budget should be unchanged after failed attempt
        assert_eq!(limiter.remaining(), 0);
    }

    #[tap_test]
    fn test_partial_consume_when_not_enough() {
        let mut limiter = DutyCycleLimiter::new(1.0, Duration::from_secs(3600));

        // Consume most of budget
        let budget = limiter.budget();
        assert!(limiter.try_consume(budget - 100));

        // Try to consume more than remaining
        assert!(!limiter.try_consume(200));

        // Remaining should be unchanged
        assert_eq!(limiter.remaining(), 100);
    }

    #[tap_test]
    fn test_remaining_percent() {
        let mut limiter = DutyCycleLimiter::new(1.0, Duration::from_secs(3600));

        assert!((limiter.remaining_percent() - 100.0).abs() < 0.01);

        let half = limiter.budget() / 2;
        limiter.try_consume(half);

        assert!((limiter.remaining_percent() - 50.0).abs() < 0.01);
    }

    #[tap_test]
    fn test_different_duty_cycles() {
        // 10% duty cycle (US regulations)
        let limiter = DutyCycleLimiter::new(10.0, Duration::from_secs(3600));
        assert_eq!(limiter.budget(), 360_000_000); // 360 seconds

        // 0.1% duty cycle (very restrictive)
        let limiter = DutyCycleLimiter::new(0.1, Duration::from_secs(3600));
        assert_eq!(limiter.budget(), 3_600_000); // 3.6 seconds
    }

    #[tap_test]
    fn test_zero_budget_is_safe() {
        let mut limiter = DutyCycleLimiter::new(0.0, Duration::from_secs(3600));
        assert_eq!(limiter.budget(), 0);
        assert!(!limiter.try_consume(1));
        assert_eq!(limiter.remaining_percent(), 0.0);
    }

    #[tap_test]
    fn test_multiple_small_consumptions() {
        let mut limiter = DutyCycleLimiter::new(1.0, Duration::from_secs(3600));
        let budget = limiter.budget();

        // Consume in 1000 chunks
        let chunk = budget / 1000;
        for _ in 0..1000 {
            assert!(limiter.try_consume(chunk));
        }

        // Should be at or near zero (might have rounding remainder)
        assert!(limiter.remaining() < chunk);
    }
}
