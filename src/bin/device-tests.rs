//! TAP test runner binary.
//!
//! Runs all tests registered with `#[tap_test]` and outputs TAP format.
//!
//! # Usage
//!
//! ```bash
//! # Run on host
//! cargo run --bin device-tests --no-default-features --features tap-tests
//!
//! # Run on QEMU (plain ESP32)
//! cargo run --bin device-tests --features tap-tests --target xtensa-esp32-espidf --release
//!
//! # Flash to hardware
//! cargo espflash flash --bin device-tests --features tap-tests --release --monitor
//! ```

#[cfg(feature = "esp32")]
use esp_idf_svc::sys as _;

fn main() {
    #[cfg(feature = "esp32")]
    {
        esp_idf_svc::sys::link_patches();
        esp_idf_svc::log::EspLogger::initialize_default();
    }

    let success = reticulum_rs_esp32::testing::run_all_tests();

    #[cfg(feature = "esp32")]
    {
        log::info!("Tests complete. Halting.");
        loop {
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
    }

    #[cfg(not(feature = "esp32"))]
    std::process::exit(if success { 0 } else { 1 });
}
