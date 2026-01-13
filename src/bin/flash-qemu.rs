//! Create flash image for QEMU and optionally run it.

use std::process::{exit, Command};

fn main() {
    let run_qemu = std::env::args().any(|arg| arg == "--run");

    println!("=== Building for QEMU (ESP32) ===\n");

    let status = Command::new("cargo")
        .args([
            "build",
            "--release",
            "--target",
            "xtensa-esp32-espidf",
            "--no-default-features",
            "--features",
            "esp32",
            "--config",
            "env.ESP_IDF_SDKCONFIG_DEFAULTS='config/sdkconfig.defaults;config/sdkconfig.qemu'",
        ])
        .status();

    if !matches!(status, Ok(s) if s.success()) {
        eprintln!("\nBuild failed!");
        exit(1);
    }

    println!("\n=== Creating flash image ===\n");

    let status = Command::new("espflash")
        .args([
            "save-image",
            "--chip",
            "esp32",
            "--merge",
            "--flash-size",
            "4mb",
            "target/xtensa-esp32-espidf/release/reticulum-rs-esp32",
            "target/qemu.bin",
        ])
        .status();

    if !matches!(status, Ok(s) if s.success()) {
        eprintln!("\nFailed to create flash image!");
        exit(1);
    }

    println!("Flash image created: target/qemu.bin");

    if run_qemu {
        println!("\n=== Running in QEMU ===\n");

        let home = std::env::var("HOME").unwrap_or_default();
        let qemu_path = format!(
            "{}/.espressif/tools/qemu-xtensa/esp_develop_9.2.2_20250228/qemu/bin/qemu-system-xtensa",
            home
        );

        let status = Command::new(&qemu_path)
            .args([
                "-machine",
                "esp32",
                "-nographic",
                "-serial",
                "mon:stdio",
                "-drive",
                "file=target/qemu.bin,if=mtd,format=raw",
            ])
            .status();

        if !matches!(status, Ok(s) if s.success()) {
            eprintln!("\nQEMU failed!");
            exit(1);
        }
    } else {
        println!("\nTo run in QEMU:");
        println!("  cargo flash-qemu -- --run");
        println!("  # or manually:");
        println!("  qemu-system-xtensa -machine esp32 -nographic -serial mon:stdio -drive file=target/qemu.bin,if=mtd,format=raw");
    }
}
