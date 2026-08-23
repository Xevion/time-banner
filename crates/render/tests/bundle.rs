//! Guards on the committed face bundle.
//!
//! Subsetting is the one step in the pipeline that can quietly remove
//! something the service still needs. A dropped script renders as boxes, a
//! dropped layout feature shifts advances by a fraction of a pixel, and
//! neither fails a build. These assert the properties that would otherwise
//! only be noticed in production.

use assert2::check;
use rstest::rstest;
use sha2::{Digest, Sha256};
use time_banner_render::font::{self, Family};

const SIZE: f64 = 27.0;

fn families() -> [Family; 3] {
    [Family::Inter, Family::RobotoMono, Family::Arimo]
}

#[rstest]
fn every_family_has_a_manifest_entry(
    #[values(Family::Inter, Family::RobotoMono, Family::Arimo)] family: Family,
) {
    let face = family.manifest();
    check!(face.key == family.as_str());
    check!(face.family == family.css_name());
    check!(!face.coverage.is_empty());
}

/// The bundle exists to be smaller than what it was built from. If a change
/// ever inverted that, the pipeline is doing work for nothing.
#[rstest]
fn every_face_is_smaller_than_the_face_it_came_from(
    #[values(Family::Inter, Family::RobotoMono, Family::Arimo)] family: Family,
) {
    let face = family.manifest();
    check!(face.subset_bytes < face.source_bytes);
}

/// The bundle is not committed, only the manifest that describes it. A face
/// rebuilt without regenerating the manifest, or the other way round, would
/// leave the two describing different things with nothing to notice.
#[rstest]
fn every_face_matches_the_checksum_its_manifest_records(
    #[values(Family::Inter, Family::RobotoMono, Family::Arimo)] family: Family,
) {
    let face = family.manifest();
    let bytes = font::face_bytes(family);
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let actual: String = hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();

    check!(bytes.len() == face.subset_bytes);
    check!(
        actual == face.subset_sha256,
        "{}: the embedded face is not the one manifest.rs describes; run `just fonts`",
        family.as_str()
    );
}

/// OFL-1.1 requires the notice and license to travel with the font, and
/// `?text=embed` ships these faces to clients. Recording them is not enough:
/// they have to be readable off the artifact, which is where they came from.
#[rstest]
fn every_face_carries_its_attribution(
    #[values(Family::Inter, Family::RobotoMono, Family::Arimo)] family: Family,
) {
    let face = family.manifest();
    check!(face.license == "OFL-1.1");
    check!(face.copyright.contains("Copyright"));
    check!(face.license_url.starts_with("https://"));
}

/// Coverage ranges are consulted with a binary search, which silently returns
/// wrong answers on unsorted or overlapping input.
#[rstest]
fn coverage_ranges_are_sorted_and_disjoint(
    #[values(Family::Inter, Family::RobotoMono, Family::Arimo)] family: Family,
) {
    let coverage = family.manifest().coverage;
    for window in coverage.windows(2) {
        let [(first, last), (next_first, next_last)] = window else {
            unreachable!("windows(2) yields pairs");
        };
        check!(first <= last);
        check!(next_first <= next_last);
        // Adjacent ranges would have been coalesced, so a gap is required.
        check!(*last + 1 < *next_first);
    }
}

/// `covers` answers from the manifest; shaping answers from the face itself.
/// They describe one artifact, so any disagreement means the committed
/// manifest no longer matches the committed face.
#[rstest]
#[case('0')]
#[case('A')]
#[case('\u{00e9}')] // é, Latin-1
#[case('\u{0142}')] // ł, Latin Extended-A
#[case('\u{03c0}')] // π, Greek
#[case('\u{0439}')] // й, Cyrillic
#[case('\u{2014}')] // em dash
#[case('\u{20ac}')] // €, currency
#[case('\u{4eca}')] // 今, covered by nothing bundled
#[case('\u{05d0}')] // א, Hebrew, likewise
fn the_manifest_agrees_with_the_face_about_coverage(#[case] c: char) {
    let text = c.to_string();
    for family in families() {
        let shaped = font::shape(family, &text, SIZE);
        // The chain only grows past the requested face when that face left a
        // `.notdef`, so a single-entry complete chain is exactly "this face
        // drew it".
        let shaped_it = shaped.chain.len() == 1 && !shaped.incomplete;
        check!(
            family.manifest().covers(c) == shaped_it,
            "{}: manifest says covers({c:?}) == {}, shaping says {shaped_it}",
            family.as_str(),
            family.manifest().covers(c),
        );
    }
}

/// Every language the service will negotiate has to be renderable by
/// something bundled, or `?locale=` hands out boxes.
///
/// The four CJK and Thai locales are the known exception: no bundled face has
/// ever covered them, and the list is spelled out so that adding a face which
/// does covers them here too, rather than leaving the gap unnoticed.
#[rstest]
#[case::english("in 3 hours")]
#[case::german("vor 5 Minuten")]
#[case::french("il y a 2 jours")]
#[case::spanish("hace 7 a\u{f1}os")]
#[case::portuguese("h\u{e1} 2 meses")]
#[case::romanian("acum \u{15f}apte ore")]
#[case::polish("2 miesi\u{105}ce temu")]
#[case::turkish("3 saat \u{f6}nce")]
#[case::swedish("f\u{f6}r 5 minuter sedan")]
#[case::danish("for 5 minutter siden")]
#[case::italian("2 ore fa")]
#[case::basque("duela 3 ordu")]
#[case::russian("4 \u{447}\u{430}\u{441}\u{430} \u{43d}\u{430}\u{437}\u{430}\u{434}")]
#[case::ukrainian("4 \u{433}\u{43e}\u{434}\u{438}\u{43d}\u{438} \u{442}\u{43e}\u{43c}\u{443}")]
#[case::belarusian(
    "4 \u{433}\u{430}\u{434}\u{437}\u{456}\u{43d}\u{44b} \u{442}\u{430}\u{43c}\u{443}"
)]
fn a_negotiable_locale_renders_without_boxes(#[case] text: &str) {
    for family in families() {
        let shaped = font::shape(family, text, SIZE);
        check!(
            !shaped.incomplete,
            "{}: {text:?} has no glyph in any bundled face",
            family.as_str()
        );
    }
}

/// Characters a `?format=` string can plausibly carry, beyond what the
/// service generates on its own. These are the reason the bundle keeps more
/// than the locales strictly need.
#[rstest]
#[case::currency("\u{20ac}12 \u{a3}9 \u{a5}5")]
#[case::dashes_and_quotes("\u{2014}\u{2013}\u{201c}\u{201d}\u{2018}\u{2019}\u{2026}")]
#[case::arrows("start \u{2192} end")]
#[case::vietnamese("Ti\u{1ebf}ng Vi\u{1ec7}t")]
#[case::greek("\u{3c0}\u{3c1}\u{3b9}\u{3bd}")]
#[case::minus_sign("\u{2212}05:30")]
fn custom_format_text_stays_covered(#[case] text: &str) {
    let shaped = font::shape(Family::Inter, text, SIZE);
    check!(
        !shaped.incomplete,
        "{text:?} lost coverage; widen COVERAGE in xtask/src/fonts.rs"
    );
}

/// A script no bundled face has still has to report itself rather than
/// silently drawing boxes, which is what the `Font` header's
/// `coverage=partial` exists to say.
#[rstest]
#[case::japanese("\u{4eca}\u{65e5}")]
#[case::korean("\u{c9c0}\u{ae08}")]
#[case::thai("\u{e0a}\u{e31}\u{e48}\u{e27}\u{e42}\u{e21}\u{e07}")]
fn an_uncovered_script_reports_partial_coverage(#[case] text: &str) {
    let shaped = font::shape(Family::Inter, text, SIZE);
    check!(shaped.incomplete);
    check!(shaped.header_value().ends_with("coverage=partial"));
}
