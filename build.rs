fn main() {
    slint_build::compile("ui/tray.slint").expect("Slint tray build failed");
    slint_build::compile("ui/main-window.slint").expect("Slint build failed");

    // Embed Windows application icon
    if cfg!(target_os = "windows") {
        winres::WindowsResource::new()
            .set_icon("assets/app-icon.ico")
            .compile()
            .expect("Windows resource build failed");
    }
}
