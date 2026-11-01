fn main() {
    tauri_build::build();
    // Examples (and so headless tooling) lack the app manifest and die with
    // STATUS_ENTRYPOINT_NOT_FOUND on comctl32 v6 imports; embed the same dependency.
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        let dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
        println!("cargo:rustc-link-arg-examples=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg-examples=/MANIFESTINPUT:{dir}/examples.manifest");
    }
}
