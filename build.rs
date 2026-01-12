fn main() {
    // Only run ESP-IDF build system when targeting ESP32 (Xtensa architecture)
    // Build scripts run on the host, so we check the TARGET env var
    if let Ok(target) = std::env::var("TARGET") {
        if target.contains("xtensa") {
            embuild::espidf::sysenv::output();
        }
    }
}
