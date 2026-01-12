#!/bin/bash
# QEMU smoke test: verify firmware boots and enters main loop
set -e

QEMU_BIN="${QEMU_BIN:-$HOME/.espressif/tools/qemu-xtensa/esp_develop_9.2.2_20250228/qemu/bin/qemu-system-xtensa}"
TIMEOUT="${TIMEOUT:-30}"
OUTPUT_FILE="/tmp/qemu-test-output.txt"

echo "=== QEMU Smoke Test ==="
echo ""

# Check QEMU exists
if [ ! -x "$QEMU_BIN" ]; then
    echo "ERROR: QEMU not found at $QEMU_BIN"
    echo "Install QEMU following docs/qemu-setup.md"
    exit 1
fi

# Source ESP environment
if [ -f ~/export-esp.sh ]; then
    source ~/export-esp.sh
else
    echo "ERROR: ~/export-esp.sh not found"
    echo "Run 'espup install' first"
    exit 1
fi

echo "Building for QEMU (plain ESP32)..."
cargo build-qemu

echo ""
echo "Creating firmware image..."
cargo espflash save-image --chip esp32 --merge --flash-size 4mb \
    --target xtensa-esp32-espidf --release target/firmware-esp32.bin

echo ""
echo "Running in QEMU (timeout: ${TIMEOUT}s)..."
timeout $TIMEOUT $QEMU_BIN \
    -machine esp32 \
    -nographic \
    -serial mon:stdio \
    -drive file=target/firmware-esp32.bin,if=mtd,format=raw 2>&1 | tee "$OUTPUT_FILE" || true

echo ""
echo "=== Checking output ==="

# Check for expected output markers
CHECKS_PASSED=0
CHECKS_TOTAL=3

if grep -q "Reticulum-rs ESP32 starting" "$OUTPUT_FILE"; then
    echo "[PASS] Firmware started"
    ((CHECKS_PASSED++))
else
    echo "[FAIL] Firmware start message not found"
fi

if grep -q "Logger initialized" "$OUTPUT_FILE"; then
    echo "[PASS] Logger initialized"
    ((CHECKS_PASSED++))
else
    echo "[FAIL] Logger initialization not found"
fi

if grep -q "Entering main loop" "$OUTPUT_FILE"; then
    echo "[PASS] Main loop entered"
    ((CHECKS_PASSED++))
else
    echo "[FAIL] Main loop not entered"
fi

echo ""
echo "=== Result: $CHECKS_PASSED/$CHECKS_TOTAL checks passed ==="

if [ $CHECKS_PASSED -eq $CHECKS_TOTAL ]; then
    echo "SUCCESS: Firmware boots correctly in QEMU"
    exit 0
else
    echo "FAILURE: Some checks failed"
    echo ""
    echo "Full output:"
    cat "$OUTPUT_FILE"
    exit 1
fi
