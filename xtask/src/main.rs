//! Asset pipeline tasks that don't belong in the compile graph: slow,
//! network-dependent, or otherwise unsuited to running on every `cargo check`.
//!
//! `cargo xtask fonts` fetches the bundled font files into `crates/render/fonts/`,
//! skipping any file whose SHA-256 already matches, so it's safe to re-run.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use assert2::assert;
use sha2::{Digest, Sha256};

/// Fetch attempts before giving up. `raw.githubusercontent.com` rate-limits
/// per IP, and CI runners share IP ranges, so transient 429s are expected.
const MAX_ATTEMPTS: u32 = 4;

/// A font file to fetch, pinned by commit and content hash so the bundle is
/// reproducible and tamper-evident.
struct FontAsset {
    /// Family name, as referenced by the SVG templates' `font-family`.
    family: &'static str,
    /// Destination filename under `crates/render/fonts/`.
    file_name: &'static str,
    /// Commit-pinned download URL (`raw.githubusercontent.com/google/fonts`).
    url: &'static str,
    /// Expected SHA-256 of the downloaded bytes.
    sha256: &'static str,
    /// SPDX license identifier, recorded for the license commentary emitted
    /// alongside each fetch. All three are Google Fonts variable-font
    /// releases under `ofl/`, at commit e1118da94a8cb00cf6d06cdac9ef13eb1e5c6ab7.
    license: &'static str,
}

const FONTS: &[FontAsset] = &[
    FontAsset {
        family: "Inter",
        file_name: "inter.ttf",
        url: "https://raw.githubusercontent.com/google/fonts/e1118da94a8cb00cf6d06cdac9ef13eb1e5c6ab7/ofl/inter/Inter%5Bopsz,wght%5D.ttf",
        sha256: "29160a80ff49ddcab2c97711247e08b1fab27a484a329ce8b813d820dc559031",
        license: "OFL-1.1",
    },
    FontAsset {
        family: "Roboto Mono",
        file_name: "RobotoMono.ttf",
        url: "https://raw.githubusercontent.com/google/fonts/e1118da94a8cb00cf6d06cdac9ef13eb1e5c6ab7/ofl/robotomono/RobotoMono%5Bwght%5D.ttf",
        sha256: "66a80e79d17e4c7cabd162e2916578a4cc08fd19eef6e2a643305eae9c567b2b",
        license: "OFL-1.1",
    },
    FontAsset {
        family: "Arimo",
        file_name: "arimo.ttf",
        url: "https://raw.githubusercontent.com/google/fonts/e1118da94a8cb00cf6d06cdac9ef13eb1e5c6ab7/ofl/arimo/Arimo%5Bwght%5D.ttf",
        sha256: "e43898b143ec826ac8cb4034816458a7047fbe0836558de2a1f8c6223ae3e0ca",
        license: "OFL-1.1",
    },
];

fn fonts_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask is a workspace member")
        .join("crates/render/fonts")
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Downloads `asset.url`, retrying transient failures (network errors, 429,
/// 5xx) with linear backoff. Panics after exhausting `MAX_ATTEMPTS`.
fn download_with_retry(asset: &FontAsset) -> Vec<u8> {
    for attempt in 1..=MAX_ATTEMPTS {
        let outcome = ureq::get(asset.url)
            .call()
            .map_err(|e| e.to_string())
            .and_then(|mut response| response.body_mut().read_to_vec().map_err(|e| e.to_string()));

        match outcome {
            Ok(bytes) => return bytes,
            Err(e) if attempt < MAX_ATTEMPTS => {
                let backoff = Duration::from_secs(attempt as u64);
                println!(
                    "{:<12} attempt {attempt}/{MAX_ATTEMPTS} failed ({e}), retrying in {backoff:?}",
                    asset.family
                );
                std::thread::sleep(backoff);
            }
            Err(e) => panic!(
                "failed to fetch {} from {} after {MAX_ATTEMPTS} attempts: {e}",
                asset.family, asset.url
            ),
        }
    }
    unreachable!("loop always returns or panics on the final attempt");
}

fn fetch_fonts() {
    let dir = fonts_dir();
    fs::create_dir_all(&dir).expect("failed to create fonts directory");

    for asset in FONTS {
        let dest = dir.join(asset.file_name);

        if let Ok(existing) = fs::read(&dest)
            && sha256_hex(&existing) == asset.sha256
        {
            println!("{:<12} up to date ({})", asset.family, asset.file_name);
            continue;
        }

        println!("{:<12} downloading {}", asset.family, asset.url);
        let bytes = download_with_retry(asset);

        let actual = sha256_hex(&bytes);
        assert!(
            actual == asset.sha256,
            "checksum mismatch for {}: upstream content changed, or the download was corrupted",
            asset.family,
        );

        fs::write(&dest, &bytes)
            .unwrap_or_else(|e| panic!("failed to write {}: {e}", dest.display()));
        println!(
            "{:<12} wrote {} ({} bytes, {})",
            asset.family,
            asset.file_name,
            bytes.len(),
            asset.license
        );
    }
}

fn main() {
    let task = std::env::args().nth(1);
    match task.as_deref() {
        Some("fonts") => fetch_fonts(),
        Some(other) => {
            eprintln!("unknown xtask: {other}\nAvailable: fonts");
            std::process::exit(1);
        }
        None => {
            eprintln!("usage: cargo xtask <task>\nAvailable: fonts");
            std::process::exit(1);
        }
    }
}
