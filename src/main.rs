//! Reticulum-rs ESP32 firmware binary.

#[cfg(feature = "esp32")]
fn main() {
    // Link ESP-IDF patches (must be first!)
    esp_idf_sys::link_patches();

    println!("=== Reticulum-rs ESP32 starting ===");

    use reticulum_rs_esp32::lora::{calculate_airtime_us, DutyCycleLimiter, LoRaParams};
    use std::time::Duration;

    // Initialize ESP-IDF logger for log crate integration
    esp_idf_svc::log::EspLogger::initialize_default();

    log::info!("Logger initialized");

    // Demonstrate duty cycle limiter
    let duty_cycle = DutyCycleLimiter::new(1.0, Duration::from_secs(3600));
    let params = LoRaParams::default();

    println!(
        "Duty cycle budget: {} us ({:.2} seconds)",
        duty_cycle.budget(),
        duty_cycle.budget() as f64 / 1_000_000.0
    );

    let airtime = calculate_airtime_us(100, &params);
    println!(
        "100-byte packet airtime: {} us ({:.2} ms)",
        airtime,
        airtime as f64 / 1000.0
    );

    // TODO: Initialize transport layers
    // TODO: Initialize reticulum-rs

    println!("Entering main loop...");
    loop {
        std::thread::sleep(Duration::from_secs(2));
        println!("Heartbeat...");
    }
}

#[cfg(not(feature = "esp32"))]
fn main() {
    println!("This binary requires the 'esp32' feature.");
    println!("Use 'cargo test --no-default-features' for host testing.");
}
