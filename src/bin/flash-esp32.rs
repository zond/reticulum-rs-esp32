//! Flash firmware to ESP32-S3 hardware.

use std::process::{exit, Command};

fn main() {
    println!("=== Building for ESP32-S3 ===\n");

    let status = Command::new("cargo")
        .args([
            "build",
            "--release",
            "--target",
            "xtensa-esp32s3-espidf",
            "--no-default-features",
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
            "target/xtensa-esp32s3-espidf/release/reticulum-rs-esp32",
        ])
        .status();

    if !matches!(status, Ok(s) if s.success()) {
        eprintln!("\nFlash failed!");
        exit(1);
    }
}
