//! `cargo xtask fonts` maintains the face bundle that `crates/render`
//! compiles in.
//!
//! Font binaries stay out of the tree, the same way the upstream faces and
//! `geoip.bin` do. Both the 1.5 MB variable faces in `crates/render/fonts/`
//! and the subsetted ones under `fonts/bundle/` are build inputs, rebuilt on
//! demand and never committed.
//!
//! `manifest.rs` is the exception, and the only artifact under review. It is
//! small, it is text, and it records a SHA-256 for each subsetted face, so it
//! pins what the bundle contains without storing it. Subsetting is
//! reproducible, so `--verify` rebuilds the bundle and fails if the manifest
//! it would write differs from the committed one. That is what catches a
//! moved upstream pin, a widened coverage set, or a subsetter that started
//! emitting different bytes.

use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use assert2::assert;
use harfrust::{
    FontRef as HrFontRef, ShapeOptions, ShaperData, ShaperInstance, UnicodeBuffer, Variation,
};
use read_fonts::FontRef;
use read_fonts::collections::IntSet;
use read_fonts::types::{NameId, Tag};
use sha2::{Digest, Sha256};
use skera::{Plan, SubsetFlags, subset_font};
use skrifa::MetadataProvider;

/// Fetch attempts before giving up. `raw.githubusercontent.com` rate-limits
/// per IP, and CI runners share IP ranges, so transient 429s are expected.
const MAX_ATTEMPTS: u32 = 4;

/// Point size the round-trip check measures at. Any size works, since the
/// bundle keeps the faces size-independent; this is the one the templates
/// draw at.
const CHECK_SIZE: f64 = 27.0;

/// A font file to fetch, pinned by commit and content hash so the bundle is
/// reproducible and tamper-evident.
struct FontAsset {
    /// The `?font=` spelling, and the bundle's filename stem.
    key: &'static str,
    /// Family name, as referenced by the SVG templates' `font-family`.
    family: &'static str,
    /// Destination filename for the upstream face, under
    /// `crates/render/fonts/`.
    file_name: &'static str,
    /// Commit-pinned download URL (`raw.githubusercontent.com/google/fonts`).
    url: &'static str,
    /// Expected SHA-256 of the downloaded bytes.
    sha256: &'static str,
    /// SPDX license identifier. All three are Google Fonts variable-font
    /// releases under `ofl/`, at commit
    /// e1118da94a8cb00cf6d06cdac9ef13eb1e5c6ab7.
    license: &'static str,
}

const FONTS: &[FontAsset] = &[
    FontAsset {
        key: "inter",
        family: "Inter",
        file_name: "inter.ttf",
        url: "https://raw.githubusercontent.com/google/fonts/e1118da94a8cb00cf6d06cdac9ef13eb1e5c6ab7/ofl/inter/Inter%5Bopsz,wght%5D.ttf",
        sha256: "29160a80ff49ddcab2c97711247e08b1fab27a484a329ce8b813d820dc559031",
        license: "OFL-1.1",
    },
    FontAsset {
        key: "roboto-mono",
        family: "Roboto Mono",
        file_name: "RobotoMono.ttf",
        url: "https://raw.githubusercontent.com/google/fonts/e1118da94a8cb00cf6d06cdac9ef13eb1e5c6ab7/ofl/robotomono/RobotoMono%5Bwght%5D.ttf",
        sha256: "66a80e79d17e4c7cabd162e2916578a4cc08fd19eef6e2a643305eae9c567b2b",
        license: "OFL-1.1",
    },
    FontAsset {
        key: "arimo",
        family: "Arimo",
        file_name: "arimo.ttf",
        url: "https://raw.githubusercontent.com/google/fonts/e1118da94a8cb00cf6d06cdac9ef13eb1e5c6ab7/ofl/arimo/Arimo%5Bwght%5D.ttf",
        sha256: "e43898b143ec826ac8cb4034816458a7047fbe0836558de2a1f8c6223ae3e0ca",
        license: "OFL-1.1",
    },
];

/// Codepoints the bundle keeps, as inclusive ranges.
///
/// `?format=` passes arbitrary literal text through to the canvas, so this is
/// wider than the strings the service generates on its own. Anything cut here
/// renders as boxes for whoever types it, and the bundled faces cover Latin,
/// Greek and Cyrillic between them; there is nothing to gain by keeping less
/// than all three.
const COVERAGE: &[(u32, u32)] = &[
    (0x0020, 0x007E), // Basic Latin
    (0x00A0, 0x00FF), // Latin-1 Supplement
    (0x0100, 0x017F), // Latin Extended-A: Polish, Turkish, Baltic, Czech
    (0x0180, 0x024F), // Latin Extended-B: Romanian comma-below, Croatian
    (0x0300, 0x036F), // Combining diacritics
    (0x0370, 0x03FF), // Greek and Coptic
    (0x0400, 0x052F), // Cyrillic, including the Ukrainian and Belarusian additions
    (0x1E00, 0x1EFF), // Latin Extended Additional: Vietnamese
    (0x2000, 0x206F), // General punctuation: dashes, quotes, ellipsis
    (0x2070, 0x209F), // Superscripts and subscripts
    (0x20A0, 0x20BF), // Currency symbols
    (0x2100, 0x214F), // Letterlike symbols
    (0x2150, 0x218F), // Number forms
    (0x2190, 0x21FF), // Arrows
    (0x2212, 0x2212), // Minus sign, which is not the ASCII hyphen
];

/// Variation tables, dropped so each bundled face ships as a static font at
/// its default axis position.
///
/// Every upstream face is variable, and the service draws one weight at one
/// optical size. Keeping the machinery that interpolates between them costs
/// more than every glyph in the bundle put together: it is most of why Inter
/// is 876 KB upstream. The default instance is already the weight the
/// templates ask for, so pinning nothing loses nothing.
const DROP_TABLES: [&[u8; 4]; 7] = [
    b"fvar", b"gvar", b"avar", b"cvar", b"HVAR", b"VVAR", b"MVAR",
];

/// Scripts the bundle keeps layout rules for, matching [`COVERAGE`].
const LAYOUT_SCRIPTS: [&[u8; 4]; 5] = [b"DFLT", b"latn", b"grek", b"cyrl", b"zinh"];

/// Layout features the bundle keeps.
///
/// These are the ones shaping applies without being asked, so measurement in
/// `render` and shaping in a client both use them. Dropping one would make a
/// client lay text out fractionally differently from the canvas measured for
/// it. Retaining every other feature costs nothing in glyph data and several
/// kilobytes in layout rules.
const LAYOUT_FEATURES: [&[u8; 4]; 9] = [
    b"kern", b"ccmp", b"mark", b"mkmk", b"liga", b"clig", b"calt", b"rlig", b"locl",
];

/// `name` records the bundle keeps.
///
/// Retention is not a manifest convenience. `?text=embed` ships a bundled
/// face to the client inside the SVG, and OFL-1.1 requires the copyright
/// notice and license to travel with the font it covers. Dropping records 0,
/// 13 and 14 would strip exactly those. The rest identify the face to a
/// client that resolves it by name.
const NAME_IDS: [u16; 9] = [
    0,  // Copyright notice
    1,  // Family
    2,  // Subfamily
    3,  // Unique identifier
    4,  // Full name
    5,  // Version
    6,  // PostScript name
    13, // License description
    14, // License URL
];

/// US English, the only language the retained records are read in. Every
/// bundled face publishes them in it, and keeping the translations would pay
/// for strings nothing reads.
const NAME_LANGUAGE_EN_US: u16 = 0x0409;

/// Strings the round-trip check measures, chosen to touch every script and
/// every feature the bundle claims to keep.
const CHECK_SAMPLES: &[&str] = &[
    "2023-11-14 16:13:20 CST",
    "in 3 hours",
    "vor 5 Minuten",
    "il y a 2 jours",
    "\u{15f}apte ore \u{15f}i 4 minute",
    "za\u{142}\u{105}czniki \u{130}stanbul",
    "\u{437}\u{430} 4 \u{433}\u{43e}\u{434}\u{438}\u{43d}\u{438} \u{442}\u{43e}\u{43c}\u{443}",
    "\u{3c0}\u{3c1}\u{3b9}\u{3bd} \u{3b1}\u{3c0}\u{3cc} 2 \u{3ce}\u{3c1}\u{3b5}\u{3c2}",
    "\u{2014}\u{2013}\u{2026}\u{20ac}\u{2212}\u{2192}",
];

fn workspace_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask is a workspace member")
}

fn sources_dir() -> PathBuf {
    workspace_root().join("crates/render/fonts")
}

fn bundle_dir() -> PathBuf {
    workspace_root().join("crates/render/fonts/bundle")
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
/// 5xx) with linear backoff. Panics after exhausting [`MAX_ATTEMPTS`].
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

/// Returns the upstream face, downloading it unless a file with the pinned
/// checksum is already on disk.
fn source_bytes(asset: &FontAsset) -> Vec<u8> {
    let dir = sources_dir();
    fs::create_dir_all(&dir).expect("failed to create fonts directory");
    let dest = dir.join(asset.file_name);

    if let Ok(existing) = fs::read(&dest)
        && sha256_hex(&existing) == asset.sha256
    {
        return existing;
    }

    println!("{:<12} downloading {}", asset.family, asset.url);
    let bytes = download_with_retry(asset);

    let actual = sha256_hex(&bytes);
    assert!(
        actual == asset.sha256,
        "checksum mismatch for {}: upstream content changed, or the download was corrupted",
        asset.family,
    );

    fs::write(&dest, &bytes).unwrap_or_else(|e| panic!("failed to write {}: {e}", dest.display()));
    bytes
}

fn tag_set(tags: &[&[u8; 4]]) -> IntSet<Tag> {
    let mut set = IntSet::empty();
    for tag in tags {
        set.insert(Tag::new(tag));
    }
    set
}

/// Subsets an upstream face down to [`COVERAGE`], as a static font.
fn subset(source: &[u8], family: &str) -> Vec<u8> {
    let font = FontRef::new(source)
        .unwrap_or_else(|e| panic!("upstream face for {family} failed to parse: {e}"));

    let mut unicodes = IntSet::empty();
    for (first, last) in COVERAGE {
        unicodes.insert_range(*first..=*last);
    }

    let mut name_ids = IntSet::empty();
    for id in NAME_IDS {
        name_ids.insert(NameId::new(id));
    }
    let mut name_languages = IntSet::empty();
    name_languages.insert(NAME_LANGUAGE_EN_US);

    let plan = Plan::new(
        &IntSet::empty(),
        &unicodes,
        &font,
        SubsetFlags::SUBSET_FLAGS_NO_HINTING | SubsetFlags::SUBSET_FLAGS_NOTDEF_OUTLINE,
        &tag_set(&DROP_TABLES),
        &tag_set(&LAYOUT_SCRIPTS),
        &tag_set(&LAYOUT_FEATURES),
        &name_ids,
        &name_languages,
    );

    subset_font(&font, &plan).unwrap_or_else(|e| panic!("failed to subset {family}: {e}"))
}

/// The codepoints a face can actually draw, coalesced into inclusive ranges.
///
/// Read back from the subsetted artifact rather than copied from
/// [`COVERAGE`], because the two differ wherever an upstream face never had
/// the codepoint to begin with. A manifest that reported what was requested
/// would claim coverage the bundle does not have.
fn coverage_of(face: &[u8], family: &str) -> Vec<(u32, u32)> {
    let font = FontRef::new(face)
        .unwrap_or_else(|e| panic!("subsetted face for {family} failed to parse: {e}"));

    let mut codepoints: Vec<u32> = font.charmap().mappings().map(|(c, _)| c).collect();
    codepoints.sort_unstable();
    codepoints.dedup();

    let mut ranges: Vec<(u32, u32)> = Vec::new();
    for codepoint in codepoints {
        match ranges.last_mut() {
            Some(last) if last.1 + 1 == codepoint => last.1 = codepoint,
            _ => ranges.push((codepoint, codepoint)),
        }
    }
    ranges
}

/// Reads a `name` table entry, which is where a face carries its own
/// attribution. Taking it from the artifact keeps the manifest honest about
/// what is actually being shipped.
fn name_entry(face: &[u8], id: skrifa::string::StringId) -> Option<String> {
    let font = FontRef::new(face).ok()?;
    font.localized_strings(id)
        .english_or_first()
        .map(|s| s.chars().collect())
}

/// Advance width of `text` in pixels, shaped the way `render` shapes it.
///
/// `xtask` deliberately does not depend on `render`: the bundle has to be
/// generatable before the crate that embeds it can compile, and depending on
/// the crate that embeds it makes that circular. Reproducing the handful of
/// setup lines is the cheaper side of that trade, and the check below fails
/// loudly if the two ever drift apart.
fn advance_px(font_data: &[u8], text: &str) -> Option<f64> {
    let font = HrFontRef::from_index(font_data, 0).ok()?;
    let variations = [Variation {
        tag: Tag::from_be_bytes(*b"wght"),
        value: 400.0,
    }];
    let instance = ShaperInstance::from_variations(&font, variations);
    let data = ShaperData::new(&font);
    let shaper = data.shaper(&font).instance(Some(&instance)).build();

    let mut buffer = UnicodeBuffer::new();
    buffer.push_str(text);
    buffer.guess_segment_properties();
    let output = shaper.shape(buffer, ShapeOptions::new());

    let advance: i32 = output
        .glyph_positions()
        .iter()
        .map(|pos| pos.x_advance)
        .sum();

    Some(f64::from(advance) / f64::from(shaper.units_per_em()) * CHECK_SIZE)
}

/// Confirms a subsetted face lays text out exactly as the upstream face did.
///
/// This is the check that makes subsetting safe to do at all. `render`
/// measures the canvas from the bundled face and a client draws with it, so a
/// subset that shifted any advance would silently mis-size every banner in
/// the affected script. Exact equality is the right bar: subsetting removes
/// glyphs and layout rules, it never rounds.
fn assert_layout_unchanged(source: &[u8], subsetted: &[u8], family: &str) {
    for text in CHECK_SAMPLES {
        let (Some(before), Some(after)) = (advance_px(source, text), advance_px(subsetted, text))
        else {
            panic!("{family}: failed to shape {text:?} while checking the subset");
        };

        // A sample outside a face's coverage measures as `.notdef` boxes on
        // both sides, which still has to match: losing the box would mean the
        // subset dropped `.notdef` itself.
        assert!(
            (before - after).abs() < 1e-9,
            "{family}: subsetting changed the advance of {text:?} ({before} -> {after}); \
             the bundle would mis-size every banner drawn in that script",
        );
    }
}

/// One face's entry in the generated manifest.
struct Entry {
    key: &'static str,
    family: &'static str,
    file_name: String,
    license: &'static str,
    copyright: String,
    license_url: String,
    source_sha256: &'static str,
    source_bytes: usize,
    subset_bytes: usize,
    subset_sha256: String,
    coverage: Vec<(u32, u32)>,
}

fn render_manifest(entries: &[Entry]) -> String {
    let mut out = String::new();
    out.push_str(
        "// @generated by `cargo xtask fonts`. Do not edit; run the task instead.\n\
         //\n\
         // Coverage is read back from each subsetted face's `cmap`, so it\n\
         // describes the bundle as built rather than as requested.\n\n\
         pub(crate) const FACES: &[BundledFace] = &[\n",
    );

    for entry in entries {
        let _ = write!(
            out,
            "    BundledFace {{\n\
             \x20       key: {:?},\n\
             \x20       family: {:?},\n\
             \x20       file_name: {:?},\n\
             \x20       license: {:?},\n\
             \x20       copyright: {:?},\n\
             \x20       license_url: {:?},\n\
             \x20       source_sha256: {:?},\n\
             \x20       source_bytes: {},\n\
             \x20       subset_bytes: {},\n\
             \x20       subset_sha256: {:?},\n\
             \x20       coverage: &[\n",
            entry.key,
            entry.family,
            entry.file_name,
            entry.license,
            entry.copyright,
            entry.license_url,
            entry.source_sha256,
            entry.source_bytes,
            entry.subset_bytes,
            entry.subset_sha256,
        );
        for (first, last) in &entry.coverage {
            let _ = writeln!(out, "            (0x{first:04X}, 0x{last:04X}),");
        }
        out.push_str("        ],\n    },\n");
    }

    out.push_str("];\n\n");
    out.push_str(
        "/// Layout retained by the bundle, and therefore what a per-request\n\
         /// subset has to ask for again: an empty set means retain nothing, so\n\
         /// re-subsetting without these would drop `kern` and shift advances.\n",
    );
    let _ = writeln!(
        out,
        "pub(crate) const LAYOUT_SCRIPTS: &[&str] = &{:?};",
        LAYOUT_SCRIPTS.map(|t| std::str::from_utf8(t).expect("tags are ASCII")),
    );
    let _ = writeln!(
        out,
        "pub(crate) const LAYOUT_FEATURES: &[&str] = &{:?};",
        LAYOUT_FEATURES.map(|t| std::str::from_utf8(t).expect("tags are ASCII")),
    );
    out
}

/// Builds every bundle artifact in memory: the subsetted faces and the
/// manifest that describes them.
fn build() -> (Vec<(String, Vec<u8>)>, String) {
    let mut files = Vec::with_capacity(FONTS.len());
    let mut entries = Vec::with_capacity(FONTS.len());

    for asset in FONTS {
        let source = source_bytes(asset);
        let subsetted = subset(&source, asset.family);
        assert_layout_unchanged(&source, &subsetted, asset.family);

        let file_name = format!("{}.ttf", asset.key);
        entries.push(Entry {
            key: asset.key,
            family: asset.family,
            file_name: file_name.clone(),
            license: asset.license,
            copyright: name_entry(&subsetted, skrifa::string::StringId::COPYRIGHT_NOTICE)
                .unwrap_or_default(),
            license_url: name_entry(&subsetted, skrifa::string::StringId::LICENSE_URL)
                .unwrap_or_default(),
            source_sha256: asset.sha256,
            source_bytes: source.len(),
            subset_bytes: subsetted.len(),
            subset_sha256: sha256_hex(&subsetted),
            coverage: coverage_of(&subsetted, asset.family),
        });
        files.push((file_name, subsetted));
    }

    let manifest = render_manifest(&entries);
    (files, manifest)
}

/// Writes the subsetted faces, which are build inputs rather than reviewed
/// artifacts: `render` cannot compile without them, and they are ignored by
/// git exactly like the upstream faces they came from.
fn write_faces(files: &[(String, Vec<u8>)]) -> PathBuf {
    let dir = bundle_dir();
    fs::create_dir_all(&dir).expect("failed to create bundle directory");

    let mut total = 0;
    for (file_name, bytes) in files {
        let dest = dir.join(file_name);
        fs::write(&dest, bytes)
            .unwrap_or_else(|e| panic!("failed to write {}: {e}", dest.display()));
        total += bytes.len();
        println!("{file_name:<16} {:>7} bytes", bytes.len());
    }
    println!("{:<16} {total:>7} bytes embedded", "bundle total");
    dir
}

/// Rebuilds the bundle and writes the manifest describing it.
fn generate() {
    let (files, manifest) = build();
    let dir = write_faces(&files);

    let dest = dir.join("manifest.rs");
    fs::write(&dest, &manifest)
        .unwrap_or_else(|e| panic!("failed to write {}: {e}", dest.display()));
    println!("{:<16} {:>7} bytes", "manifest.rs", manifest.len());
}

/// Rebuilds the bundle and fails if the manifest it would write differs from
/// the committed one, which is how CI notices that the bundle no longer
/// matches its description.
///
/// The faces are still written, because they are build inputs and CI needs
/// them on disk regardless; only the manifest is compared. Every face's
/// SHA-256 is in that manifest, so comparing it covers the bytes too.
fn verify() {
    let (files, manifest) = build();
    let dir = write_faces(&files);

    let committed = fs::read_to_string(dir.join("manifest.rs")).unwrap_or_default();
    if committed == manifest {
        println!("{:<16} matches the rebuilt bundle", "manifest.rs");
        return;
    }

    eprintln!("\ncrates/render/fonts/bundle/manifest.rs does not describe the bundle this");
    eprintln!("pipeline builds. An upstream pin, the coverage set, or the subsetter");
    eprintln!("changed. Run `cargo xtask fonts` and commit the manifest.\n");

    let committed_lines: Vec<&str> = committed.lines().collect();
    for (n, line) in manifest.lines().enumerate() {
        if committed_lines.get(n) != Some(&line) {
            eprintln!("  first difference at line {}:", n + 1);
            eprintln!(
                "    committed:  {}",
                committed_lines.get(n).unwrap_or(&"<missing>")
            );
            eprintln!("    rebuilt:    {line}");
            break;
        }
    }
    std::process::exit(1);
}

pub fn run(args: &[String]) {
    match args.iter().any(|a| a == "--verify") {
        true => verify(),
        false => generate(),
    }
}
