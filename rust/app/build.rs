//! On Windows: put the icon and version details into Mindloom.exe.
fn main() {
    println!("cargo:rerun-if-changed=../packaging/windows/mindloom.ico");
    #[cfg(windows)]
    {
        if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
            let mut res = winresource::WindowsResource::new();
            res.set_icon("../packaging/windows/mindloom.ico")
                .set("ProductName", "Mindloom")
                .set("FileDescription", "Mindloom — mind mapping")
                .set("CompanyName", "Mindloom contributors")
                .set("LegalCopyright", "Copyright © 2026 Mindloom contributors. MIT licence.")
                .set("OriginalFilename", "Mindloom.exe");
            if let Err(e) = res.compile() {
                println!("cargo:warning=could not embed the Windows icon: {e}");
            }
        }
    }
}
