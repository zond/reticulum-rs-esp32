//! Production flash tool with flash encryption.
//!
//! THIS TOOL PERMANENTLY MODIFIES THE ESP32 DEVICE.
//!
//! Flash encryption burns eFuses on first boot. This cannot be undone.
//! Only use for final production devices.

use std::io::{self, Write};
use std::process::{exit, Command};

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

    eprintln!("\n=== Flashing to device ===\n");
    eprintln!("WARNING: First boot will burn eFuses permanently!\n");

    let status = Command::new("espflash")
        .args([
            "flash",
            "--release",
            "target/xtensa-esp32-espidf/release/reticulum-rs-esp32",
        ])
        .status();

    match status {
        Ok(s) if s.success() => {
            eprintln!("\n=== Flash complete ===");
            eprintln!("Device will enable encryption on first boot.");
            eprintln!("DO NOT interrupt the first boot process!");
        }
        Ok(_) => {
            eprintln!("\nFlash failed!");
            exit(1);
        }
        Err(e) => {
            eprintln!("\nFailed to run espflash: {}", e);
            exit(1);
        }
    }
}
