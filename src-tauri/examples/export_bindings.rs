//! cargo run --example export_bindings   (headless; writes src/bindings.ts)
fn main() {
    companionframe_lib::specta_builder()
        .export(
            specta_typescript::Typescript::default(),
            concat!(env!("CARGO_MANIFEST_DIR"), "/../src/bindings.ts"),
        )
        .expect("failed to export typescript bindings");
}
