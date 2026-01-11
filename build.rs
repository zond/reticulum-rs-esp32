fn main() {
    // Only run ESP-IDF build system when targeting ESP32
    // Build scripts run on the host, so we check the TARGET env var
    if let Ok(target) = std::env::var("TARGET") {
        if target.contains("xtensa") || target.contains("riscv32imc-esp") {
            embuild::espidf::sysenv::output();
        }
    }
}
