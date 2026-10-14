// Embed the app icon into the Windows executable (Explorer, taskbar, Alt-Tab).
// macOS/Linux take their icons from the bundle metadata in Cargo.toml instead.
fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        winresource::WindowsResource::new()
            .set_icon("assets/icon.ico")
            .compile()
            .expect("failed to embed Windows icon");
    }
}
