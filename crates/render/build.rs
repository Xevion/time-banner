//! Fails fast with a helpful message if the face bundle is missing, instead
//! of letting `include_bytes!` report a bare "file not found" for a path the
//! error doesn't explain.
//!
//! Neither the upstream faces nor the subsetted bundle is committed, so this
//! is what a fresh clone hits before running the task.

use std::path::Path;

const REQUIRED: &[&str] = &["arimo.ttf", "inter.ttf", "roboto-mono.ttf", "manifest.rs"];

fn main() {
    let bundle = Path::new(env!("CARGO_MANIFEST_DIR")).join("fonts/bundle");

    let missing: Vec<&str> = REQUIRED
        .iter()
        .filter(|name| !bundle.join(name).is_file())
        .copied()
        .collect();

    if !missing.is_empty() {
        panic!(
            "missing face bundle file(s) in {}: {}. Run `just fonts` (or `cargo xtask fonts`) to rebuild it.",
            bundle.display(),
            missing.join(", "),
        );
    }

    println!("cargo:rerun-if-changed=fonts/bundle");
}
