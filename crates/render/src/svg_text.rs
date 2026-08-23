//! How a banner's text reaches the client.
//!
//! SVG is the default format and is served as a document, not rasterized, so
//! whatever face the template names has to actually be available wherever the
//! image lands. On the surfaces this service exists for, such as README badges,
//! mail clients and wikis, it generally is not. Each mode below answers that
//! differently, and the tradeoff is genuinely open, so all three are built.

use std::str::FromStr;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use read_fonts::FontRef;
use read_fonts::collections::IntSet;
use read_fonts::types::Tag;
use skera::{Plan, SubsetFlags, subset_font};

use crate::error::RenderError;
use crate::font::{Family, Shaped};

/// How text is carried in SVG output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextMode {
    /// Glyphs become filled paths, so the image renders identically
    /// everywhere and needs no font at all. The default, because it is the
    /// only mode that is correct without knowing anything about the client.
    #[default]
    Outline,
    /// Live text plus an `@font-face` carrying a subset of the face inline.
    /// Keeps the text selectable and searchable, and is correct wherever
    /// SVG-as-image honors data-URI fonts. Blink and Gecko do; WebKit
    /// historically has not.
    Embed,
    /// Live text and nothing else: smallest possible output, correct only if
    /// the client happens to have the face. This is what the service emitted
    /// before the other two existed, kept as the baseline they are measured
    /// against.
    Live,
}

/// A `?text=` value named no known mode.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("unknown text mode")]
pub struct UnknownTextMode;

impl TextMode {
    pub fn as_str(self) -> &'static str {
        match self {
            TextMode::Outline => "outline",
            TextMode::Embed => "embed",
            TextMode::Live => "live",
        }
    }
}

impl FromStr for TextMode {
    type Err = UnknownTextMode;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "outline" => Ok(TextMode::Outline),
            "embed" => Ok(TextMode::Embed),
            "live" => Ok(TextMode::Live),
            _ => Err(UnknownTextMode),
        }
    }
}

/// Escapes text for an XML character-data position.
///
/// Tera only auto-escapes templates whose name ends in `.html`, `.htm`, or
/// `.xml`, so an `.svg` template interpolates raw. Rendered text is partly
/// caller-controlled through `?format=`, and the response is served as
/// `image/svg+xml`, which executes script when opened directly rather than
/// through `<img>`. Escaping happens here so no template can forget it.
fn escape_xml(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(c),
        }
    }
    out
}

/// The family name an embedded subset is bound to. Deliberately not a real
/// family name, so it can only match the face carried in the same document.
const EMBEDDED_FAMILY: &str = "tb-embedded";

/// The `<defs>` and `<text>` markup for a shaped run.
pub(crate) struct Markup {
    pub defs: String,
    pub body: String,
}

/// Builds the markup for `shaped` in `mode`.
///
/// `Outline` produces the same markup as `Live` here; it becomes paths in
/// `outline`, which needs a parsed document rather than a string.
pub(crate) fn markup(
    mode: TextMode,
    text: &str,
    shaped: &Shaped,
    font_size: f64,
    x: f64,
    baseline: f64,
) -> Result<Markup, RenderError> {
    let escaped = escape_xml(text);

    let (defs, stack) = match mode {
        TextMode::Embed => {
            let subset = subset(shaped.family(), text)?;
            let encoded = STANDARD.encode(&subset);
            let defs = format!(
                "<defs><style>@font-face{{font-family:\"{EMBEDDED_FAMILY}\";\
                 src:url(data:font/ttf;base64,{encoded}) format(\"truetype\");\
                 font-weight:400;font-style:normal}}</style></defs>"
            );
            (defs, format!("{EMBEDDED_FAMILY}, {}", shaped.css_stack()))
        }
        TextMode::Outline | TextMode::Live => (String::new(), shaped.css_stack()),
    };

    // Optical sizing stays off so measurement, rasterization, and any client
    // all place glyphs from the face's default axis position, and so an
    // embedded subset can be a static font rather than a variable one.
    let body = format!(
        "<text x=\"{x}\" y=\"{baseline}\" text-anchor=\"start\" \
         font-family=\"{}\" font-size=\"{font_size}\" \
         font-optical-sizing=\"none\">{escaped}</text>",
        escape_xml(&stack),
    );

    Ok(Markup { defs, body })
}

/// Variation tables, dropped so an embedded subset ships as a static font at
/// the face's default axis position.
///
/// Every bundled face is a variable font, and a subsetter that cannot pin an
/// axis has to keep the machinery that drives it. For Inter those tables run
/// several times the size of the handful of glyphs in a banner, which is the
/// difference between embedding being competitive and being absurd. Nothing
/// is lost because the emitted text asks for no axis but the default.
const VARIATION_TABLES: [&[u8; 4]; 7] = [
    b"fvar", b"gvar", b"avar", b"cvar", b"HVAR", b"VVAR", b"MVAR",
];

/// Scripts an embedded subset keeps layout rules for. The bundled faces cover
/// Latin, Greek and Cyrillic, and `?locale=` can put any of the three on a
/// banner; every other script's rules are dead weight.
const LAYOUT_SCRIPTS: [&[u8; 4]; 5] = [b"DFLT", b"latn", b"grek", b"cyrl", b"zinh"];

/// Layout features an embedded subset keeps.
///
/// These are the ones shaping applies without being asked, so measurement
/// here and shaping in the client both use them. Dropping one would make a
/// client lay the text out fractionally differently from the canvas measured
/// for it. Retaining every other feature costs nothing in glyph data and
/// several kilobytes in layout rules.
const LAYOUT_FEATURES: [&[u8; 4]; 9] = [
    b"kern", b"ccmp", b"mark", b"mkmk", b"liga", b"clig", b"calt", b"rlig", b"locl",
];

/// Subsets `family` down to the codepoints `text` uses.
///
/// Subsetting by codepoint rather than by glyph id is what keeps the `cmap`
/// intact, and the client needs that to map its own characters onto the
/// glyphs. It shapes the text itself, and never sees our shaping result.
fn subset(family: Family, text: &str) -> Result<Vec<u8>, RenderError> {
    let font = FontRef::new(family.data())
        .map_err(|e| RenderError::encode("failed to read bundled face for subsetting", e))?;

    let mut unicodes = IntSet::empty();
    for c in text.chars() {
        unicodes.insert(c as u32);
    }

    let mut drop_tables = IntSet::empty();
    for tag in VARIATION_TABLES {
        drop_tables.insert(Tag::new(tag));
    }

    let mut layout_scripts = IntSet::empty();
    for tag in LAYOUT_SCRIPTS {
        layout_scripts.insert(Tag::new(tag));
    }

    let mut layout_features = IntSet::empty();
    for tag in LAYOUT_FEATURES {
        layout_features.insert(Tag::new(tag));
    }

    let plan = Plan::new(
        &IntSet::empty(),
        &unicodes,
        &font,
        SubsetFlags::SUBSET_FLAGS_NO_HINTING | SubsetFlags::SUBSET_FLAGS_NOTDEF_OUTLINE,
        &drop_tables,
        &layout_scripts,
        &layout_features,
        &IntSet::empty(),
        &IntSet::empty(),
    );

    subset_font(&font, &plan)
        .map_err(|e| RenderError::encode("failed to subset face for embedding", e))
}

#[cfg(test)]
mod tests {
    use assert2::check;
    use rstest::rstest;

    use super::*;
    use crate::font;

    const SIZE: f64 = 27.0;

    #[rstest]
    #[case("outline", TextMode::Outline)]
    #[case("EMBED", TextMode::Embed)]
    #[case(" live ", TextMode::Live)]
    fn mode_parses_from_a_query_value(#[case] raw: &str, #[case] expected: TextMode) {
        check!(raw.parse::<TextMode>() == Ok(expected));
    }

    #[test]
    fn an_unknown_mode_is_rejected() {
        check!("paths".parse::<TextMode>() == Err(UnknownTextMode));
    }

    #[test]
    fn the_default_mode_is_the_one_that_needs_no_client_font() {
        check!(TextMode::default() == TextMode::Outline);
    }

    /// `?format=` reaches this text, the response is served as
    /// `image/svg+xml`, and opening that URL directly runs whatever script it
    /// contains. Markup in the value has to come out inert.
    #[rstest]
    #[case(TextMode::Live)]
    #[case(TextMode::Embed)]
    fn markup_in_the_text_is_escaped(#[case] mode: TextMode) {
        let text = "</text><script>alert(1)</script>";
        let shaped = font::shape(Family::RobotoMono, text, SIZE);
        let markup = markup(mode, text, &shaped, SIZE, 12.0, 24.0).unwrap();

        check!(!markup.body.contains("<script"));
        check!(!markup.body.contains("</text><"));
        check!(markup.body.contains("&lt;script&gt;"));
    }

    #[rstest]
    fn every_family_subsets(
        #[values(Family::Inter, Family::RobotoMono, Family::Arimo)] family: Family,
    ) {
        let subset = subset(family, "2023-11-14 16:13:20 CST").unwrap();
        check!(subset.len() > 0);
        // A subset is a font in its own right, so it must still parse as one.
        check!(FontRef::new(&subset).is_ok());
    }

    /// The whole point of subsetting: what gets embedded is a small fraction
    /// of the face it came from.
    #[rstest]
    fn a_subset_is_much_smaller_than_the_face(
        #[values(Family::Inter, Family::RobotoMono, Family::Arimo)] family: Family,
    ) {
        let subset = subset(family, "2023-11-14 16:13:20 CST").unwrap();
        check!(subset.len() * 4 < family.data().len());
    }

    #[test]
    fn embedding_carries_a_data_uri_and_binds_the_text_to_it() {
        let text = "2023-11-14 16:13:20 CST";
        let shaped = font::shape(Family::RobotoMono, text, SIZE);
        let markup = markup(TextMode::Embed, text, &shaped, SIZE, 12.0, 24.0).unwrap();

        check!(markup.defs.contains("@font-face"));
        check!(markup.defs.contains("data:font/ttf;base64,"));
        check!(markup.body.contains(EMBEDDED_FAMILY));
        // The real families stay behind it, so a client that ignores the
        // embedded face still has something to try.
        check!(markup.body.contains("Roboto Mono"));
    }

    /// The client never sees our shaping result. It shapes the text itself
    /// against whatever we embedded, so if the subset lays that text out to a
    /// different width than the canvas was measured for, the banner is
    /// clipped or padded on every browser that honors it. Re-shaping the
    /// subset is the only way to catch that.
    #[rstest]
    fn an_embedded_subset_lays_out_to_the_measured_width(
        #[values(Family::Inter, Family::RobotoMono, Family::Arimo)] family: Family,
    ) {
        let text = "2023-11-14 16:13:20 CST";
        let expected = font::measure(family, text, SIZE);

        let subset = subset(family, text).unwrap();
        let actual = font::measure_with(&subset, text, SIZE).expect("subset shapes");

        check!((expected - actual).abs() < 0.01);
    }

    #[test]
    fn live_output_carries_no_font_payload() {
        let text = "now";
        let shaped = font::shape(Family::Inter, text, SIZE);
        let markup = markup(TextMode::Live, text, &shaped, SIZE, 12.0, 24.0).unwrap();

        check!(markup.defs.is_empty());
        check!(markup.body.contains("Inter"));
    }
}
