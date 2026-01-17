//! Production flash tool with flash encryption.
//!
//! THIS TOOL PERMANENTLY MODIFIES THE ESP32 DEVICE.
//!
//! Flash encryption burns eFuses on first boot. This cannot be undone.
//! Only use for final production devices.

// This binary only runs on the host, not on ESP32
#![cfg(not(target_os = "espidf"))]

use reticulum_rs_esp32::host_utils::{find_esp32_port, flash_binary};
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::{exit, Command};

const CHIP: &str = "esp32";

const WARNING: &str = r#"
================================================================================
                    *** PRODUCTION FLASH - DANGER ***
================================================================================

This tool will build and flash firmware with FLASH ENCRYPTION ENABLED.

PERMANENT CONSEQUENCES:
  - eFuses will be burned on first boot (IRREVERSIBLE)
  - Device can only run encrypted firmware after this
  - In Release mode: losing the key = bricked device
  - Cannot be undone or disabled

ONLY USE THIS FOR:
  - Final production devices
  - Devices you never need to reflash with different firmware
  - When you have proper key backup procedures

================================================================================
"#;

fn main() {
    eprintln!("{}", WARNING);

    // Require explicit confirmation
    eprint!("Type 'I UNDERSTAND THIS IS PERMANENT' to continue: ");
    io::stderr().flush().unwrap();

    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();

    if input.trim() != "I UNDERSTAND THIS IS PERMANENT" {
        eprintln!("\nAborted. No changes made.");
        exit(1);
    }

    eprintln!("\n=== Building with production config ===\n");

    // Set environment to use production sdkconfig
    // ESP-IDF looks for SDKCONFIG_DEFAULTS
    let status = Command::new("cargo")
        .args([
            "build",
            "--release",
            "--target",
            "xtensa-esp32-espidf",
            "--features",
            "esp32",
        ])
        .env(
            "SDKCONFIG_DEFAULTS",
            "config/sdkconfig.defaults;config/sdkconfig.production",
        )
        .status();

    match status {
        Ok(s) if s.success() => {}
        Ok(_) => {
            eprintln!("\nBuild failed!");
            exit(1);
        }
        Err(e) => {
            eprintln!("\nFailed to run cargo: {}", e);
            exit(1);
        }
    }

    // Auto-detect ESP32 port
    let port = match find_esp32_port() {
        Some(p) => p,
        None => {
            eprintln!("\nNo ESP32 device found. Check USB connection.");
            exit(1);
        }
    };

    eprintln!("\n=== Flashing to device ({}) ===\n", port);
    eprintln!("WARNING: First boot will burn eFuses permanently!\n");

    let binary_path = PathBuf::from("target/xtensa-esp32-espidf/release/reticulum-rs-esp32");
    match flash_binary(&binary_path, &port, CHIP) {
        Ok(()) => {
            eprintln!("\n=== Flash complete ===");
            eprintln!("Device will enable encryption on first boot.");
            eprintln!("DO NOT interrupt the first boot process!");
        }
        Err(e) => {
            eprintln!("\nFlash failed: {}", e);
            exit(1);
        }
    }
}
