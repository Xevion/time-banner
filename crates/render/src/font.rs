//! Bundled face registry: shaping, measurement, and the fallback chain.
//!
//! Measurement mirrors what `usvg` does when it lays the same string out, since
//! both run `harfrust` over the same bytes: the `wght` axis is pinned to the
//! CSS font weight, and every other axis stays at the face's default.
//! Diverging from that makes the canvas disagree with the glyphs drawn inside
//! it.
//!
//! Optical sizing is deliberately left off. `usvg` and browsers would
//! otherwise pin `opsz` to the font size, which is a real improvement for a
//! face that has the axis. It also means a face can only be embedded as a
//! variable font, and carrying `gvar` for one string costs several times what
//! the glyphs do. Banners draw at a single size, where the refinement is
//! invisible, so the templates ask for `font-optical-sizing: none` and
//! measurement matches by leaving the axis alone.

use std::str::FromStr;
use std::sync::LazyLock;

use harfrust::{FontRef, ShapeOptions, ShaperData, ShaperInstance, Tag, UnicodeBuffer, Variation};

pub(crate) const ARIMO: &[u8] = include_bytes!("../fonts/arimo.ttf");
pub(crate) const INTER: &[u8] = include_bytes!("../fonts/inter.ttf");
pub(crate) const ROBOTO_MONO: &[u8] = include_bytes!("../fonts/RobotoMono.ttf");

const WGHT: Tag = Tag::from_be_bytes(*b"wght");

/// Weight the templates draw at, mapped onto the `wght` axis the way `usvg`
/// maps CSS `font-weight`.
const REGULAR_WEIGHT: f32 = 400.0;

/// `.notdef`, which every face uses for a codepoint it cannot draw. Its
/// presence after shaping is the only reliable signal that a face lacks
/// coverage.
const NOTDEF: u32 = 0;

/// A bundled font family.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Family {
    Inter,
    RobotoMono,
    Arimo,
}

/// A `?font=` value named no bundled family.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("unknown font family")]
pub struct UnknownFamily;

impl Family {
    /// The name SVG `font-family` and `fontdb` resolve against.
    pub fn css_name(self) -> &'static str {
        match self {
            Family::Inter => "Inter",
            Family::RobotoMono => "Roboto Mono",
            Family::Arimo => "Arimo",
        }
    }

    /// The `?font=` spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Family::Inter => "inter",
            Family::RobotoMono => "roboto-mono",
            Family::Arimo => "arimo",
        }
    }

    pub(crate) fn data(self) -> &'static [u8] {
        match self {
            Family::Inter => INTER,
            Family::RobotoMono => ROBOTO_MONO,
            Family::Arimo => ARIMO,
        }
    }
}

impl FromStr for Family {
    type Err = UnknownFamily;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "inter" => Ok(Family::Inter),
            "roboto-mono" | "robotomono" | "roboto mono" => Ok(Family::RobotoMono),
            "arimo" => Ok(Family::Arimo),
            _ => Err(UnknownFamily),
        }
    }
}

/// Faces consulted after the requested one, broadest coverage first.
const FALLBACK_ORDER: [Family; 3] = [Family::Inter, Family::Arimo, Family::RobotoMono];

/// A face parsed once at startup. `ShaperData` holds the table caches, which
/// is the expensive half of shaping; the per-call `ShaperInstance` only
/// normalizes axis coordinates.
struct Face {
    font: FontRef<'static>,
    shaper_data: ShaperData,
}

impl Face {
    fn new(family: Family) -> Self {
        let font = FontRef::from_index(family.data(), 0)
            .unwrap_or_else(|e| panic!("bundled face {} failed to parse: {e}", family.as_str()));
        let shaper_data = ShaperData::new(&font);
        Self { font, shaper_data }
    }

    /// Shapes `text`, returning glyph ids and the total advance in font units.
    /// Font units are size-independent, so the caller scales.
    fn shape(&self, text: &str) -> (Vec<u32>, i32, u16) {
        let variations = [Variation {
            tag: WGHT,
            value: REGULAR_WEIGHT,
        }];

        let instance = ShaperInstance::from_variations(&self.font, variations);
        let shaper = self
            .shaper_data
            .shaper(&self.font)
            .instance(Some(&instance))
            .build();

        let mut buffer = UnicodeBuffer::new();
        buffer.push_str(text);
        buffer.guess_segment_properties();
        let output = shaper.shape(buffer, ShapeOptions::new());

        let glyphs = output
            .glyph_infos()
            .iter()
            .map(|info| info.glyph_id)
            .collect();
        let advance = output
            .glyph_positions()
            .iter()
            .map(|pos| pos.x_advance)
            .sum();

        (glyphs, advance, shaper.units_per_em() as u16)
    }
}

struct Registry {
    inter: Face,
    roboto_mono: Face,
    arimo: Face,
}

impl Registry {
    fn face(&self, family: Family) -> &Face {
        match family {
            Family::Inter => &self.inter,
            Family::RobotoMono => &self.roboto_mono,
            Family::Arimo => &self.arimo,
        }
    }
}

static REGISTRY: LazyLock<Registry> = LazyLock::new(|| Registry {
    inter: Face::new(Family::Inter),
    roboto_mono: Face::new(Family::RobotoMono),
    arimo: Face::new(Family::Arimo),
});

/// A shaped run and the faces it took to produce one.
#[derive(Debug, Clone)]
pub struct Shaped {
    /// Faces consulted, in the order tried. The last entry drew the run, so
    /// more than one entry means a substitution happened.
    pub chain: Vec<Family>,
    /// Whether the drawing face still left an uncoverable codepoint, which
    /// happens when no bundled face covers the script at all.
    pub incomplete: bool,
    advance: i32,
    units_per_em: u16,
    font_size: f64,
}

impl Shaped {
    /// The face that actually drew the run.
    pub fn family(&self) -> Family {
        *self.chain.last().expect("a chain always ends with a face")
    }

    /// Advance width in pixels, at the size this run was shaped for.
    pub fn advance_px(&self) -> f64 {
        f64::from(self.advance) / f64::from(self.units_per_em) * self.font_size
    }

    /// The `font-family` list to emit, so `usvg` and any client walk the same
    /// chain this run was measured against.
    pub fn css_stack(&self) -> String {
        self.chain
            .iter()
            .map(|family| family.css_name())
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// The `Font` response header value.
    ///
    /// The chain alone says which faces were tried, but not whether the last
    /// one succeeded. A run that no bundled face covers still draws, as boxes,
    /// and a caller staring at those boxes has no other way to learn that the
    /// glyphs were missing rather than the styling wrong.
    pub fn header_value(&self) -> String {
        match self.incomplete {
            true => format!("{}; coverage=partial", self.css_stack()),
            false => self.css_stack(),
        }
    }
}

/// Shapes `text` at `font_size`, walking the fallback chain until a face
/// covers every codepoint.
///
/// Fallback is whole-string rather than per-cluster: a run that mixes scripts
/// no single face covers settles on the requested face and reports itself
/// `incomplete`, rather than stitching several faces into one line. Every
/// string this service draws today comes from one script, and the honest
/// report is more useful than a partial repair.
pub fn shape(family: Family, text: &str, font_size: f64) -> Shaped {
    let mut chain = Vec::with_capacity(1 + FALLBACK_ORDER.len());

    for candidate in std::iter::once(family).chain(FALLBACK_ORDER) {
        if chain.contains(&candidate) {
            continue;
        }
        chain.push(candidate);

        let (glyphs, advance, units_per_em) = REGISTRY.face(candidate).shape(text);
        if !glyphs.contains(&NOTDEF) {
            return Shaped {
                chain,
                incomplete: false,
                advance,
                units_per_em,
                font_size,
            };
        }
    }

    // Nothing covered it. Draw with what was asked for, boxes and all, and say
    // so rather than substituting a face that is no better.
    let (_, advance, units_per_em) = REGISTRY.face(family).shape(text);
    Shaped {
        chain: vec![family],
        incomplete: true,
        advance,
        units_per_em,
        font_size,
    }
}

/// Advance width of `text` in pixels, ignoring which face drew it.
pub fn measure(family: Family, text: &str, font_size: f64) -> f64 {
    shape(family, text, font_size).advance_px()
}

/// Advance width of `text` in an arbitrary face, for checking that a face
/// built from a bundled one still lays text out the same way.
///
/// Returns `None` if `font_data` is not a font this can shape.
pub fn measure_with(font_data: &[u8], text: &str, font_size: f64) -> Option<f64> {
    let font = FontRef::from_index(font_data, 0).ok()?;
    let variations = [Variation {
        tag: WGHT,
        value: REGULAR_WEIGHT,
    }];
    let instance = ShaperInstance::from_variations(&font, variations);
    let shaper_data = ShaperData::new(&font);
    let shaper = shaper_data.shaper(&font).instance(Some(&instance)).build();

    let mut buffer = UnicodeBuffer::new();
    buffer.push_str(text);
    buffer.guess_segment_properties();
    let output = shaper.shape(buffer, ShapeOptions::new());

    let advance: i32 = output
        .glyph_positions()
        .iter()
        .map(|pos| pos.x_advance)
        .sum();

    Some(f64::from(advance) / f64::from(shaper.units_per_em() as u16) * font_size)
}

#[cfg(test)]
mod tests {
    use assert2::check;
    use rstest::rstest;

    use super::*;

    const SIZE: f64 = 27.0;

    #[rstest]
    fn every_family_parses(
        #[values(Family::Inter, Family::RobotoMono, Family::Arimo)] family: Family,
    ) {
        check!(measure(family, "0", SIZE) > 0.0);
    }

    #[rstest]
    #[case("inter", Family::Inter)]
    #[case("INTER", Family::Inter)]
    #[case("roboto-mono", Family::RobotoMono)]
    #[case(" arimo ", Family::Arimo)]
    fn family_parses_from_a_query_value(#[case] raw: &str, #[case] expected: Family) {
        check!(raw.parse::<Family>() == Ok(expected));
    }

    #[test]
    fn an_unknown_family_is_rejected() {
        check!("nonesuch".parse::<Family>() == Err(UnknownFamily));
    }

    /// The guess this replaces was a flat 0.6 ratio per character. A
    /// proportional face has to disagree with that, or nothing was gained.
    #[test]
    fn a_proportional_face_is_not_a_fixed_ratio() {
        let narrow = measure(Family::Inter, "iiii", SIZE);
        let wide = measure(Family::Inter, "MMMM", SIZE);
        check!(narrow < wide);
    }

    #[test]
    fn a_monospace_face_advances_uniformly() {
        let narrow = measure(Family::RobotoMono, "iiii", SIZE);
        let wide = measure(Family::RobotoMono, "MMMM", SIZE);
        check!((narrow - wide).abs() < 0.01);
    }

    #[rstest]
    fn measurement_grows_with_text(
        #[values(Family::Inter, Family::RobotoMono, Family::Arimo)] family: Family,
    ) {
        let mut previous = 0.0;
        for text in ["1", "12", "123", "2023-11-14 16:13:20 CST"] {
            let width = measure(family, text, SIZE);
            check!(width.is_finite());
            check!(width > previous);
            previous = width;
        }
    }

    /// With optical sizing off, every face has size-independent proportions,
    /// Inter's `opsz` axis included. A face that stopped scaling linearly
    /// would mean an axis is being varied that the emitted SVG does not ask
    /// for, and the canvas would no longer match the glyphs.
    #[rstest]
    fn measurement_scales_linearly_with_font_size(
        #[values(Family::Inter, Family::RobotoMono, Family::Arimo)] family: Family,
    ) {
        let single = measure(family, "now", 10.0);
        let double = measure(family, "now", 20.0);
        check!((double - single * 2.0).abs() < 0.01);
    }

    #[test]
    fn an_empty_string_measures_zero() {
        check!(measure(Family::Inter, "", SIZE) == 0.0);
    }

    /// Latin text is covered by the face asked for, so nothing substitutes and
    /// the reported chain stays a single entry.
    #[test]
    fn a_covered_string_reports_no_substitution() {
        let shaped = shape(Family::RobotoMono, "2023-11-14 16:13:20 CST", SIZE);
        check!(shaped.chain == vec![Family::RobotoMono]);
        check!(shaped.header_value() == "Roboto Mono");
        check!(!shaped.incomplete);
    }

    /// No bundled face covers CJK, so this reports itself incomplete rather
    /// than pretending a fallback helped.
    #[test]
    fn an_uncoverable_script_is_reported_rather_than_hidden() {
        let shaped = shape(Family::Inter, "\u{4eca}\u{65e5}", SIZE);
        check!(shaped.incomplete);
        check!(shaped.family() == Family::Inter);
        check!(shaped.header_value() == "Inter; coverage=partial");
        // The SVG still names a real family, so a client has something to
        // resolve even where the glyphs will be boxes.
        check!(shaped.css_stack() == "Inter");
    }

    #[test]
    fn a_chain_starts_at_the_requested_face_and_ends_at_the_drawing_one() {
        let shaped = shape(Family::RobotoMono, "hello", SIZE);
        check!(shaped.chain.first() == Some(&Family::RobotoMono));
        check!(shaped.chain.last() == Some(&shaped.family()));
    }
}
