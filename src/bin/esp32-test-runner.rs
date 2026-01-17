//! Unified test runner for ESP32 tests.
//!
//! Supports running tests in QEMU (emulator) or on real hardware.
//!
//! Usage:
//!   cargo test-qemu      # Run in QEMU emulator
//!   cargo test-esp32     # Run on real ESP32 hardware

// This binary only runs on the host, not on ESP32
#![cfg(not(target_os = "espidf"))]

use reticulum_rs_esp32::host_utils::{
    find_esp32_port, find_qemu, flash_binary, list_available_ports, monitor_output, start_monitor,
    ProcessGuard, TerminalGuard,
};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Write;
use std::ops::ControlFlow;
use std::path::{Path, PathBuf};
use std::process::{exit, Command, Stdio};

const TEST_TIMEOUT_SECS: u64 = 120;
const RUST_TARGET: &str = "xtensa-esp32-espidf";
const CHIP: &str = "esp32";

/// Cargo compiler artifact message (subset of fields we care about).
#[derive(Deserialize)]
struct CargoMessage {
    reason: String,
    #[serde(default)]
    executable: Option<String>,
    #[serde(default)]
    target: Option<CargoTarget>,
}

/// Cargo target info from compiler artifact message.
#[derive(Deserialize)]
struct CargoTarget {
    kind: Vec<String>,
}

/// Target environment for running tests.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Target {
    /// QEMU emulator (ESP32)
    Qemu,
    /// Real ESP32 hardware
    Hardware,
}

impl Target {
    fn name(&self) -> &'static str {
        match self {
            Target::Qemu => "QEMU",
            Target::Hardware => "ESP32",
        }
    }
}

/// Calculate SHA256 hash of a file.
fn hash_file(path: &Path) -> Result<String, std::io::Error> {
    let data = fs::read(path)?;
    let mut hasher = Sha256::new();
    hasher.update(&data);
    Ok(format!("{:x}", hasher.finalize()))
}

/// Get path to the hash cache file for a given binary.
fn hash_cache_path(binary_path: &Path) -> PathBuf {
    let parent = binary_path.parent().unwrap_or(Path::new("."));
    parent.join(".last_flashed_hash")
}

/// Check if binary needs to be reflashed by comparing hashes.
/// Returns true if flash is needed (hash mismatch or no cached hash).
fn needs_reflash(binary_path: &Path) -> bool {
    let cache_path = hash_cache_path(binary_path);

    // Calculate current binary hash
    let current_hash = match hash_file(binary_path) {
        Ok(h) => h,
        Err(_) => return true, // Can't read binary, need to flash
    };

    // Read cached hash
    let cached_hash = match fs::read_to_string(&cache_path) {
        Ok(h) => h.trim().to_string(),
        Err(_) => return true, // No cache, need to flash
    };

    current_hash != cached_hash
}

/// Save hash after successful flash.
///
/// Uses atomic write (temp file + rename) to prevent corruption if interrupted.
fn save_flash_hash(binary_path: &Path) {
    if let Ok(hash) = hash_file(binary_path) {
        let cache_path = hash_cache_path(binary_path);
        let temp_path = cache_path.with_extension("tmp");
        // Write to temp file, then atomically rename
        if fs::write(&temp_path, &hash).is_ok() {
            let _ = fs::rename(temp_path, cache_path);
        }
    }
}

fn main() {
    let target = parse_args();

    if let Err(e) = run(target) {
        eprintln!("Error: {}", e);
        exit(1);
    }
}

fn parse_args() -> Target {
    let args: Vec<String> = std::env::args().collect();

    // Check for explicit flag
    for arg in &args[1..] {
        match arg.as_str() {
            "--qemu" | "-q" => return Target::Qemu,
            "--hardware" | "--hw" => return Target::Hardware,
            "--help" => {
                println!("ESP32 Test Runner");
                println!();
                println!("Usage:");
                println!("  {} [OPTIONS]", args[0]);
                println!();
                println!("Options:");
                println!("  --qemu, -q       Run tests in QEMU emulator (default)");
                println!("  --hardware, --hw Run tests on real ESP32 hardware");
                println!("  --help           Show this help");
                exit(0);
            }
            _ => {}
        }
    }

    // Auto-detect based on binary name
    if let Some(name) = args.first().and_then(|s| s.split('/').next_back()) {
        if name.contains("esp32") && !name.contains("qemu") {
            return Target::Hardware;
        }
    }

    // Default to QEMU
    Target::Qemu
}

fn run(target: Target) -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Building tests for {} ===", target.name());

    // Build with JSON output to reliably find the test binary
    let output = Command::new("cargo")
        .args([
            "test",
            "--no-run",
            "--target",
            RUST_TARGET,
            "--features",
            "esp32",
            "--release",
            "--message-format=json",
        ])
        .stderr(Stdio::inherit()) // Show build progress
        .output()?;

    if !output.status.success() {
        return Err("Build failed".into());
    }

    // Parse JSON output to find the test binary
    let test_binary = find_test_binary_from_json(&output.stdout)?;
    println!("Found test binary: {}", test_binary.display());

    match target {
        Target::Qemu => run_qemu_tests(&test_binary),
        Target::Hardware => run_hardware_tests(&test_binary),
    }
}

fn run_qemu_tests(test_binary: &Path) -> Result<(), Box<dyn std::error::Error>> {
    // Create flash image
    println!("\n=== Creating flash image ===");
    let image_path = PathBuf::from("target/qemu-tests.bin");

    let status = Command::new("espflash")
        .args([
            "save-image",
            "--chip",
            "esp32",
            "--merge",
            &test_binary.to_string_lossy(),
            &image_path.to_string_lossy(),
            "--flash-size",
            "4mb",
        ])
        .status()?;

    if !status.success() {
        return Err("Failed to create flash image".into());
    }

    // Find QEMU
    let qemu_path = find_qemu()
        .ok_or("QEMU not found. Install from: https://github.com/espressif/qemu/releases")?;
    println!("Using QEMU: {}", qemu_path.display());

    // Run QEMU
    println!("\n=== Running tests in QEMU ===\n");

    let mut process = Command::new(&qemu_path)
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
        .stderr(Stdio::inherit())
        .spawn()?;

    let stdout = process
        .stdout
        .take()
        .ok_or("Failed to capture stdout from QEMU")?;
    let _guard = ProcessGuard(process);

    run_test_monitor(stdout)
}

fn run_hardware_tests(test_binary: &Path) -> Result<(), Box<dyn std::error::Error>> {
    // Find device
    let port = match find_esp32_port() {
        Some(p) => p,
        None => {
            let available = list_available_ports();
            if !available.is_empty() {
                eprintln!("Available serial ports:");
                for port in &available {
                    eprintln!("  {}", port);
                }
            }
            return Err("No ESP32 device found. Check USB connection.".into());
        }
    };
    println!("Found device: {}", port);

    // Check if we need to reflash (binary changed since last flash)
    if needs_reflash(test_binary) {
        println!("\n=== Flashing to ESP32 ===\n");
        flash_binary(test_binary, &port, CHIP).map_err(|e| format!("Flash failed: {}", e))?;
        save_flash_hash(test_binary);
    } else {
        println!("\n=== Binary unchanged, skipping flash (reset only) ===\n");
    }

    // Monitor for test output (espflash monitor does a hard-reset by default)
    println!("=== Monitoring test output ===\n");

    // TerminalGuard ensures terminal is reset even if we panic or exit early
    let _term_guard = TerminalGuard;

    let mut process =
        start_monitor(&port).map_err(|e| format!("Failed to start monitor: {}", e))?;

    let stdout = process
        .stdout
        .take()
        .ok_or("Failed to capture stdout from espflash monitor")?;

    // ProcessGuard ensures cleanup even if run_test_monitor panics
    let _process_guard = ProcessGuard(process);

    run_test_monitor(stdout)
}

/// Test execution state machine for context-aware crash detection.
#[derive(Debug, Clone, Copy, PartialEq)]
enum TestState {
    /// Device is booting, before test framework starts.
    Booting,
    /// Test framework initialized ("running X tests" seen).
    Initialized,
    /// First actual test started running ("test ..." seen).
    Running,
}

/// Result from test monitoring.
enum TestResult {
    Passed,
    Failed,
    Crashed(String),
}

fn run_test_monitor(stdout: impl std::io::Read) -> Result<(), Box<dyn std::error::Error>> {
    let mut test_state = TestState::Booting;
    let mut test_result: Option<TestResult> = None;

    let monitor_result: Result<(), String> = monitor_output(stdout, TEST_TIMEOUT_SECS, |line| {
        // Update test state based on output
        if line.contains("running") && line.contains("tests") {
            test_state = TestState::Initialized;
        }
        if test_state == TestState::Initialized && line.starts_with("test ") {
            test_state = TestState::Running;
        }

        // Print test-relevant output (once initialized or running)
        // Use explicit \r\n because espflash may leave terminal in raw mode
        if test_state != TestState::Booting {
            print!("{}\r\n", line);
            let _ = std::io::stdout().flush();
        }

        // Check for test completion
        if line.contains("test result:") {
            test_result = if line.contains("ok") && !line.contains("FAILED") {
                Some(TestResult::Passed)
            } else {
                Some(TestResult::Failed)
            };
            return ControlFlow::Break(Ok(()));
        }

        // Check for crash patterns with state context
        if let Some(reason) = check_crash_pattern(line, test_state) {
            test_result = Some(TestResult::Crashed(reason));
            return ControlFlow::Break(Ok(()));
        }

        ControlFlow::Continue(())
    });

    // Handle timeout or other monitor errors
    if let Err(e) = monitor_result {
        let message = if e.contains("Timeout") {
            format!(
                "Timeout: tests ran for more than {} seconds",
                TEST_TIMEOUT_SECS
            )
        } else {
            e
        };
        eprintln!("\n{}", message);
        return Err(message.into());
    }

    print!("\r\n");
    match test_result {
        Some(TestResult::Passed) => {
            print!("=== All tests passed ===\r\n");
            Ok(())
        }
        Some(TestResult::Failed) => Err("Tests failed".into()),
        Some(TestResult::Crashed(reason)) => {
            eprint!("*** TEST CRASHED: {} ***\r\n", reason);
            Err("Tests failed".into())
        }
        None => Err("Could not determine test result".into()),
    }
}

/// Check for crash patterns in output.
///
/// Some patterns (Guru Meditation, panic) are always crashes.
/// Other patterns (WDT reset, reboot) are only crashes if tests have started running,
/// since boot logs often show the previous reset reason.
fn check_crash_pattern(line: &str, state: TestState) -> Option<String> {
    // Immediate crash indicators - always a crash regardless of state
    if line.contains("Guru Meditation Error") {
        return Some("Guru Meditation Error (CPU exception)".to_string());
    }
    if line.contains("abort() was called") || line.contains("assert failed") {
        return Some("Assertion/abort failure".to_string());
    }
    if line.contains("stack overflow") {
        return Some("Stack overflow detected".to_string());
    }
    if line.contains("CORRUPTED") {
        return Some("Stack corruption detected".to_string());
    }
    if line.contains("panic") && line.contains("occurred") {
        return Some("Panic occurred".to_string());
    }

    // Reboot/reset patterns - only consider a crash if tests are running.
    // Boot logs often show the previous reset reason (e.g., WDT_SYS_RESET from a prior run).
    if state == TestState::Running {
        if line.contains("WDT_SYS_RESET")
            || line.contains("TG0WDT_SYS_RESET")
            || line.contains("TG1WDT_SYS_RESET")
        {
            return Some("Watchdog timer reset (possible hang)".to_string());
        }
        // Use "rst:0x" to match ESP-IDF boot format specifically (e.g., "rst:0x1 (POWERON_RESET)")
        if line.contains("Rebooting...") || line.starts_with("rst:0x") {
            return Some("Device rebooted (crash detected)".to_string());
        }
    }
    None
}

/// Parse cargo JSON output to find the test binary path.
///
/// Looks for compiler-artifact messages with kind "lib" and an executable path.
fn find_test_binary_from_json(json_output: &[u8]) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let output_str = String::from_utf8_lossy(json_output);

    // Each line is a separate JSON message
    for line in output_str.lines() {
        if line.trim().is_empty() {
            continue;
        }

        // Parse JSON message (ignore parse errors for non-JSON lines)
        let msg: CargoMessage = match serde_json::from_str(line) {
            Ok(m) => m,
            Err(_) => continue,
        };

        // Look for compiler-artifact with executable
        if msg.reason == "compiler-artifact" {
            if let (Some(executable), Some(target)) = (msg.executable, msg.target) {
                // We want the lib target (test binary), not proc-macro or bin
                if target.kind.contains(&"lib".to_string()) {
                    return Ok(PathBuf::from(executable));
                }
            }
        }
    }

    Err("No test binary found in cargo output".into())
}
