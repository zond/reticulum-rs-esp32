//! Flash node binary to ESP32-S3 hardware.
//!
//! Usage: cargo run --bin flash-esp32

use std::process::{exit, Command};

fn main() {
    println!("=== Building node for ESP32-S3 ===\n");

    let status = Command::new("cargo")
        .args([
            "build",
            "--bin",
            "node",
            "--release",
            "--target",
            "xtensa-esp32s3-espidf",
            "--features",
            "esp32",
        ])
        .status();

    if !matches!(status, Ok(s) if s.success()) {
        eprintln!("\nBuild failed!");
        exit(1);
    }

    println!("\n=== Flashing to device ===\n");

    let status = Command::new("espflash")
        .args([
            "flash",
            "--monitor",
            "target/xtensa-esp32s3-espidf/release/node",
        ])
        .status();

    if !matches!(status, Ok(s) if s.success()) {
        eprintln!("\nFlash failed!");
        exit(1);
    }
}
