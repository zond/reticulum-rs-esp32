//! QEMU test runner for ESP32 tests.
//!
//! This binary builds tests for ESP32, creates a flash image, runs QEMU,
//! and reports test results.

use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{exit, Child, Command, Stdio};
use std::time::{Duration, Instant};

const QEMU_TIMEOUT_SECS: u64 = 120;

/// RAII guard to ensure QEMU process is always cleaned up.
struct QemuGuard(Child);

impl Drop for QemuGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn main() {
    if let Err(e) = run() {
        eprintln!("Error: {}", e);
        exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Building tests for ESP32 (QEMU) ===");

    // Build tests
    let status = Command::new("cargo")
        .args([
            "test",
            "--no-run",
            "--target",
            "xtensa-esp32-espidf",
            "--features",
            "esp32",
            "--release",
        ])
        .status()?;

    if !status.success() {
        return Err("Build failed".into());
    }

    // Find the test binary
    let test_binary = find_test_binary()?;
    println!("Found test binary: {}", test_binary.display());

    // Create flash image
    println!("\n=== Creating flash image ===");
    let image_path = PathBuf::from("target/qemu-tests.bin");

    let test_binary_str = test_binary
        .to_str()
        .ok_or("Test binary path contains invalid UTF-8")?;
    let image_path_str = image_path
        .to_str()
        .ok_or("Image path contains invalid UTF-8")?;

    let status = Command::new("espflash")
        .args([
            "save-image",
            "--chip",
            "esp32",
            "--merge",
            test_binary_str,
            image_path_str,
            "--flash-size",
            "4mb",
        ])
        .status()?;

    if !status.success() {
        return Err("Failed to create flash image".into());
    }

    // Find QEMU
    let qemu_path = find_qemu()?;
    println!("Using QEMU: {}", qemu_path.display());

    // Run QEMU
    println!("\n=== Running tests in QEMU ===\n");

    let mut qemu = Command::new(&qemu_path)
        .args([
            "-machine",
            "esp32",
            "-nographic",
            "-serial",
            "mon:stdio",
            "-drive",
            &format!("file={},if=mtd,format=raw", image_path.display()),
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit()) // Inherit stderr to avoid deadlock
        .spawn()?;

    // Take stdout before wrapping in guard
    // Safe: stdout is Some because we configured Stdio::piped()
    let stdout = qemu.stdout.take().unwrap();

    // Wrap in guard to ensure cleanup on any exit path
    let _guard = QemuGuard(qemu);

    let reader = BufReader::new(stdout);

    let start = Instant::now();
    let timeout = Duration::from_secs(QEMU_TIMEOUT_SECS);
    let mut test_result: Option<bool> = None;
    let mut in_test_output = false;

    // Note: timeout is checked between output lines, so a completely hung
    // process that produces no output may exceed QEMU_TIMEOUT_SECS slightly
    for line in reader.lines() {
        if start.elapsed() > timeout {
            eprintln!(
                "\nTimeout: QEMU ran for more than {} seconds",
                QEMU_TIMEOUT_SECS
            );
            return Err("Test timeout".into());
        }

        let line = line?;

        // Print test-relevant output (matches Rust test framework format)
        if line.contains("running") && line.contains("tests") {
            in_test_output = true;
        }

        if in_test_output {
            println!("{}", line);
        }

        // Check for test completion
        if line.contains("test result:") {
            if line.contains("ok") && !line.contains("FAILED") {
                test_result = Some(true);
            } else {
                test_result = Some(false);
            }
            break;
        }

        // Check for crash/reboot (indicates test failure)
        if line.contains("Guru Meditation Error") || line.contains("abort() was called") {
            eprintln!("\nTest crashed!");
            test_result = Some(false);
            break;
        }
    }

    // Guard will kill QEMU on drop

    // Report result
    println!();
    match test_result {
        Some(true) => {
            println!("=== All tests passed ===");
            Ok(())
        }
        Some(false) => Err("Tests failed".into()),
        None => Err("Could not determine test result".into()),
    }
}

fn find_test_binary() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let deps_dir = PathBuf::from("target/xtensa-esp32-espidf/release/deps");

    if !deps_dir.exists() {
        return Err("Build output directory not found".into());
    }

    // Look for the test binary (named reticulum_rs_esp32-<hash>)
    for entry in std::fs::read_dir(&deps_dir)? {
        let entry = entry?;
        let path = entry.path();

        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            // Match test binary: starts with crate name, no extension, not a .d or .rmeta file
            if name.starts_with("reticulum_rs_esp32-") && !name.contains('.') && path.is_file() {
                // Verify it's a reasonable size for a test binary
                let metadata = std::fs::metadata(&path)?;
                if metadata.len() > 100_000 {
                    return Ok(path);
                }
            }
        }
    }

    Err("Test binary not found in target/xtensa-esp32-espidf/release/deps/".into())
}

fn find_qemu() -> Result<PathBuf, Box<dyn std::error::Error>> {
    // Check standard ESP tools location
    let home = std::env::var("HOME")?;
    let qemu_path = PathBuf::from(format!(
        "{}/.espressif/tools/qemu-xtensa/esp_develop_9.2.2_20250228/qemu/bin/qemu-system-xtensa",
        home
    ));

    if qemu_path.exists() {
        return Ok(qemu_path);
    }

    // Try PATH
    if let Ok(output) = Command::new("which").arg("qemu-system-xtensa").output() {
        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !path.is_empty() {
                return Ok(PathBuf::from(path));
            }
        }
    }

    Err("QEMU not found. Install from: https://github.com/espressif/qemu/releases".into())
}
