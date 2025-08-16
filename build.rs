fn main() {
    // Rebuild when the UI changes
    println!("cargo:rerun-if-changed=ui/ui.slint");
    println!("cargo:rerun-if-changed=ui.slint");

    let path = if std::path::Path::new("ui/ui.slint").exists() { "ui/ui.slint" } else { "ui.slint" };
    slint_build::compile(path).expect("slint-build: compiling .slint failed");
}
