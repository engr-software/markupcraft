//! Windows only: embed the app icon and version info (VERSIONINFO) in `markupcraft.exe`, so it
//! shows in Explorer, the taskbar and Alt-Tab. Other targets: nothing. A missing resource
//! compiler is a warning (cross builds still link) unless `MARKUPCRAFT_REQUIRE_WINRES=1`.

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=../../assets/markupcraft.ico");
    println!("cargo:rerun-if-env-changed=MARKUPCRAFT_REQUIRE_WINRES");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let mut res = winresource::WindowsResource::new();
    res.set_icon("../../assets/markupcraft.ico")
        .set("ProductName", "MarkupCraft")
        .set("FileDescription", "MarkupCraft PDF markup and takeoff")
        .set(
            "LegalCopyright",
            "Copyright (c) the MarkupCraft contributors. MIT OR Apache-2.0.",
        )
        .set("OriginalFilename", "markupcraft.exe")
        .set("InternalName", "markupcraft");
    if let Err(e) = res.compile() {
        if std::env::var_os("MARKUPCRAFT_REQUIRE_WINRES").is_some() {
            println!("cargo::error=embedding Windows resources failed: {e}");
            return;
        }
        println!("cargo:warning=markupcraft.exe built without icon/version resources: {e}");
    }
}
