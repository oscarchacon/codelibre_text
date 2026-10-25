//! Embeds the application icon into the Windows executable (Explorer, shortcuts, Alt+Tab).
//! Other platforms get their icon from the window at runtime (`main.rs`) and from packaging.

fn main() {
    println!("cargo:rerun-if-changed=../../assets/icons/codelibre_text.ico");

    #[cfg(windows)]
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut resource = winresource::WindowsResource::new();
        resource.set_icon("../../assets/icons/codelibre_text.ico");
        if let Err(e) = resource.compile() {
            println!("cargo:warning=could not embed the Windows icon: {e}");
        }
    }
}
