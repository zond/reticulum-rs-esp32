use esp_idf_sys as _;
use log::info;

fn main() {
    // Initialize ESP-IDF
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    info!("Reticulum-rs ESP32 starting...");

    // TODO: Initialize transport layers
    // TODO: Initialize reticulum-rs

    loop {
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
}
