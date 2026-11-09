fn main() {
    // Tauri context codegen only when the desktop bridge is enabled, so
    // `cargo test --lib` stays free of GUI dependencies. The binary target
    // requires the `desktop` feature, where `tauri` is always present.
    #[cfg(feature = "desktop")]
    tauri_build::build();
}
