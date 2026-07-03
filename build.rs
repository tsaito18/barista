fn main() {
    // Icon/manifest embedding is only meaningful (and only compiles) when
    // targeting Windows; skip entirely on other host/target platforms so
    // the crate remains cross-platform-buildable during development.
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os != "windows" {
        return;
    }

    let mut res = winresource::WindowsResource::new();
    // tray_icon::Icon::from_resource(1, ...) がこの ID を前提に読み込むため、
    // set_icon の既定値（"1"）に依存せず明示的に固定する。
    res.set_icon_with_id("assets/icon.ico", "1");
    res.set_icon_with_id("assets/icon_gray.ico", "2");
    res.set_manifest_file("assets/app.manifest");

    if let Err(e) = res.compile() {
        // Fail the build loudly: a tray app silently shipped without its
        // icon/manifest is a worse outcome than a build error here.
        eprintln!("failed to embed Windows resources: {e}");
        std::process::exit(1);
    }
}
