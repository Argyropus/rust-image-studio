//! Windows: embed the app icon (assets/icon.ico, the docs/logo.svg logo) in the .exe, so
//! Explorer, the taskbar and shortcuts show it. Other targets need nothing here; the window
//! icon is set at run time from assets/icon.png.

fn main() {
    println!("cargo:rerun-if-changed=assets/icon.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon.ico");
        if let Err(e) = res.compile() {
            // A missing resource compiler must not break the build: the exe just keeps
            // the default icon.
            println!("cargo:warning=app icon not embedded: {e}");
        }
    }
}
