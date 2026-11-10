fn main() {
    // Computer app control captures windows through ScreenCaptureKit, whose
    // screenshot API arrived in macOS 14; the app starts on macOS 11, so a
    // missing framework must not stop it (agent_tools/computer/mac/capture.rs).
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        println!("cargo:rustc-link-arg=-Wl,-weak_framework,ScreenCaptureKit");
    }
    tauri_build::build()
}
