//! Simple TAP (Test Anything Protocol) test harness for device testing.
//!
//! This module provides a minimal test framework that outputs results in TAP format,
//! which can be parsed by standard TAP consumers. It works on:
//! - QEMU (via emulated UART)
//! - Real hardware (via USB serial)
//! - Host (for testing the harness itself)
//!
//! **Note:** This module is only available when the `tap-tests` feature is enabled.
//! This ensures test code is never included in production builds.
//!
//! # Usage
//!
//! Tests are defined in a `tap_tests` module guarded by the feature flag,
//! using the `#[tap_test]` attribute (similar to `#[test]`):
//!
//! ```ignore
//! // In src/mymodule.rs
//! #[cfg(feature = "tap-tests")]
//! mod tap_tests {
//!     use super::*;
//!     use reticulum_rs_esp32_macros::tap_test;
//!
//!     #[tap_test]
//!     fn addition_works() {
//!         assert_eq!(2 + 2, 4);
//!     }
//!
//!     #[tap_test]
//!     fn parsing_works() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
//!         let value: i32 = "42".parse()?;
//!         assert_eq!(value, 42);
//!         Ok(())
//!     }
//! }
//! ```
//!
//! The test binary runs all collected tests automatically:
//!
//! ```ignore
//! // In src/bin/device-tests.rs (built with --features tap-tests)
//! use reticulum_rs_esp32::testing;
//!
//! fn main() {
//!     let success = testing::run_all_tests();
//!     std::process::exit(if success { 0 } else { 1 });
//! }
//! ```

use std::panic::{catch_unwind, AssertUnwindSafe};

// Re-export inventory for use by the proc-macro
pub use inventory;

/// Result type for test functions.
pub type TestResult = Result<(), Box<dyn std::error::Error + Send + Sync>>;

/// Type alias for test registration functions.
pub type TestRegisterFn = fn(&mut TestRunner);

/// Entry for a TAP test, collected via inventory.
pub struct TapTestEntry {
    /// Name of the test.
    pub name: &'static str,
    /// Function that registers and runs the test.
    pub register: TestRegisterFn,
}

impl TapTestEntry {
    /// Create a new test entry.
    pub const fn new(name: &'static str, register: TestRegisterFn) -> Self {
        Self { name, register }
    }
}

// Collect all tests registered via #[tap_test]
inventory::collect!(TapTestEntry);

/// Count all registered TAP tests.
pub fn test_count() -> usize {
    inventory::iter::<TapTestEntry>.into_iter().count()
}

/// Run all registered TAP tests and return success status.
pub fn run_all_tests() -> bool {
    let mut runner = TestRunner::new();
    let count = test_count();

    runner.print_header(count);

    for entry in inventory::iter::<TapTestEntry> {
        (entry.register)(&mut runner);
    }

    runner.finish()
}

/// A simple test runner that outputs TAP format.
pub struct TestRunner {
    tests_run: usize,
    tests_passed: usize,
    tests_failed: usize,
}

impl Default for TestRunner {
    fn default() -> Self {
        Self::new()
    }
}

/// Extract a human-readable message from panic payload.
fn extract_panic_message(payload: &Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        s.to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "unknown panic".to_string()
    }
}

impl TestRunner {
    /// Create a new test runner.
    pub fn new() -> Self {
        Self {
            tests_run: 0,
            tests_passed: 0,
            tests_failed: 0,
        }
    }

    /// Run a test function and record the result.
    ///
    /// The test function should return `Ok(())` on success or `Err(...)` on failure.
    /// Panics are caught and recorded as failures.
    pub fn run<F>(&mut self, name: &str, test_fn: F)
    where
        F: FnOnce() -> TestResult + std::panic::UnwindSafe,
    {
        self.tests_run += 1;
        let test_num = self.tests_run;

        let result = catch_unwind(AssertUnwindSafe(test_fn));

        match result {
            Ok(Ok(())) => {
                self.tests_passed += 1;
                println!("ok {} - {}", test_num, name);
            }
            Ok(Err(e)) => {
                self.tests_failed += 1;
                println!("not ok {} - {}", test_num, name);
                println!("# Error: {}", e);
            }
            Err(panic_info) => {
                self.tests_failed += 1;
                println!("not ok {} - {}", test_num, name);
                println!("# Panic: {}", extract_panic_message(&panic_info));
            }
        }
    }

    /// Run a test that uses assert! macros (may panic).
    pub fn run_assert<F>(&mut self, name: &str, test_fn: F)
    where
        F: FnOnce() + std::panic::UnwindSafe,
    {
        self.run(name, || {
            test_fn();
            Ok(())
        });
    }

    /// Run a test that should panic.
    ///
    /// If `expected` is Some, the panic message must contain the expected string.
    pub fn run_should_panic<F>(&mut self, name: &str, test_fn: F, expected: Option<&str>)
    where
        F: FnOnce() + std::panic::UnwindSafe,
    {
        self.tests_run += 1;
        let test_num = self.tests_run;

        let result = catch_unwind(AssertUnwindSafe(test_fn));

        match result {
            Ok(()) => {
                // Test didn't panic but should have
                self.tests_failed += 1;
                println!("not ok {} - {}", test_num, name);
                println!("# Expected panic but test completed normally");
            }
            Err(panic_info) => {
                let msg = extract_panic_message(&panic_info);

                // Check if panic message matches expected
                if let Some(expected_msg) = expected {
                    if msg.contains(expected_msg) {
                        self.tests_passed += 1;
                        println!("ok {} - {}", test_num, name);
                    } else {
                        self.tests_failed += 1;
                        println!("not ok {} - {}", test_num, name);
                        println!(
                            "# Expected panic containing '{}', got '{}'",
                            expected_msg, msg
                        );
                    }
                } else {
                    // Any panic is acceptable
                    self.tests_passed += 1;
                    println!("ok {} - {}", test_num, name);
                }
            }
        }
    }

    /// Print the TAP header. Call this before running tests.
    pub fn print_header(&self, planned_tests: usize) {
        println!("TAP version 14");
        println!("1..{}", planned_tests);
    }

    /// Print a diagnostic comment.
    pub fn comment(msg: &str) {
        println!("# {}", msg);
    }

    /// Finish the test run and print summary. Returns true if all tests passed.
    pub fn finish(&self) -> bool {
        println!("# -----------------------");
        println!("# Tests run: {}", self.tests_run);
        println!("# Passed: {}", self.tests_passed);
        println!("# Failed: {}", self.tests_failed);

        if self.tests_failed == 0 {
            println!("# Result: PASS");
            true
        } else {
            println!("# Result: FAIL");
            false
        }
    }

    /// Get the number of tests run.
    pub fn tests_run(&self) -> usize {
        self.tests_run
    }

    /// Get the number of tests passed.
    pub fn tests_passed(&self) -> usize {
        self.tests_passed
    }

    /// Get the number of tests failed.
    pub fn tests_failed(&self) -> usize {
        self.tests_failed
    }
}

// Tests for the test harness itself - using our own TAP system
mod tap_tests {
    use super::*;
    use reticulum_rs_esp32_macros::tap_test;

    #[tap_test]
    fn runner_tracks_passing_test() {
        let mut runner = TestRunner::new();
        runner.run("passing_test", || Ok(()));
        assert_eq!(runner.tests_run(), 1);
        assert_eq!(runner.tests_passed(), 1);
        assert_eq!(runner.tests_failed(), 0);
    }

    #[tap_test]
    fn runner_tracks_failing_test() {
        let mut runner = TestRunner::new();
        runner.run("failing_test", || Err("test error".into()));
        assert_eq!(runner.tests_run(), 1);
        assert_eq!(runner.tests_passed(), 0);
        assert_eq!(runner.tests_failed(), 1);
    }

    #[tap_test]
    fn runner_catches_panic() {
        let mut runner = TestRunner::new();
        runner.run_assert("panicking_test", || {
            panic!("intentional panic");
        });
        assert_eq!(runner.tests_run(), 1);
        assert_eq!(runner.tests_passed(), 0);
        assert_eq!(runner.tests_failed(), 1);
    }

    #[tap_test]
    fn runner_tracks_mixed_results() {
        let mut runner = TestRunner::new();
        runner.run("pass1", || Ok(()));
        runner.run("fail1", || Err("error".into()));
        runner.run("pass2", || Ok(()));
        assert_eq!(runner.tests_run(), 3);
        assert_eq!(runner.tests_passed(), 2);
        assert_eq!(runner.tests_failed(), 1);
    }
}
