fn main() {
    slint_build::compile("ui/tray.slint").expect("Slint tray build failed");
    slint_build::compile("ui/main-window.slint").expect("Slint build failed");
}
