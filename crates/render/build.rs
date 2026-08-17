//! Fails fast with a helpful message if the bundled fonts haven't been
//! fetched yet, instead of letting `include_bytes!` report a bare "file not
//! found" for a path the error doesn't explain.

use std::path::Path;

const REQUIRED_FONTS: &[&str] = &["arimo.ttf", "inter.ttf", "RobotoMono.ttf"];

fn main() {
    let fonts_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("fonts");

    let missing: Vec<&str> = REQUIRED_FONTS
        .iter()
        .filter(|name| !fonts_dir.join(name).is_file())
        .copied()
        .collect();

    if !missing.is_empty() {
        panic!(
            "missing bundled font(s) in {}: {}. Run `just fonts` (or `cargo xtask fonts`) first.",
            fonts_dir.display(),
            missing.join(", "),
        );
    }

    println!("cargo:rerun-if-changed=fonts");
}
