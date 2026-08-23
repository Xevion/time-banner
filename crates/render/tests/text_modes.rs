//! End-to-end checks that the canvas fits the glyphs drawn inside it, and
//! that each text mode delivers those glyphs in the way it claims to.

use assert2::check;
use jiff::{Timestamp, tz::TimeZone};
use rstest::rstest;
use time_banner_render::font::{self, Family};
use time_banner_render::raster::Rasterizer;
use time_banner_render::{OutputForm, OutputFormat, RenderContext, TextMode};

const FONT_SIZE: f64 = 27.0;
const PADDING_X: f64 = 12.0;

/// The fixed zoom-out `Rasterizer::rasterize` applies about the canvas centre.
const ZOOM: f64 = 0.90;

fn render(
    form: OutputForm,
    format: OutputFormat,
    mode: TextMode,
    family: Option<Family>,
) -> Vec<u8> {
    let now = Timestamp::from_second(1_700_000_000).unwrap();
    RenderContext {
        value: now,
        output_form: form,
        output_format: format,
        tz: TimeZone::UTC,
        now,
        format: None,
        locale: None,
        font: family,
        text_mode: mode,
    }
    .render()
    .expect("render succeeds")
    .bytes
}

fn svg(mode: TextMode, family: Option<Family>) -> String {
    String::from_utf8(render(
        OutputForm::Absolute,
        OutputFormat::Svg,
        mode,
        family,
    ))
    .expect("SVG output is UTF-8")
}

/// Reads the `width` attribute off the root element.
fn svg_width(svg: &str) -> u32 {
    let start = svg.find("width=\"").expect("root carries a width") + 7;
    let end = start + svg[start..].find('"').expect("width is terminated");
    svg[start..end].parse().expect("width is an integer")
}

/// Width from the PNG's IHDR, which is the first thing after the signature.
fn png_width(png: &[u8]) -> u32 {
    u32::from_be_bytes(png[16..20].try_into().expect("IHDR carries a width"))
}

/// Horizontal extent of everything non-transparent in a rasterized banner.
fn ink_columns(svg: &str) -> (u32, u32) {
    let rasterizer = Rasterizer::new();
    let tree = rasterizer.parse(svg.as_bytes()).expect("SVG parses");
    let pixmap = rasterizer.rasterize(&tree);

    let (width, height) = (pixmap.width(), pixmap.height());
    let pixels = pixmap.data();

    let mut left = width;
    let mut right = 0;
    for y in 0..height {
        for x in 0..width {
            let alpha = pixels[((y * width + x) * 4 + 3) as usize];
            if alpha > 0 {
                left = left.min(x);
                right = right.max(x);
            }
        }
    }
    (left, right)
}

#[rstest]
fn every_mode_renders_a_well_formed_document(
    #[values(TextMode::Outline, TextMode::Embed, TextMode::Live)] mode: TextMode,
) {
    let svg = svg(mode, None);
    check!(svg.starts_with("<svg"));
    check!(svg.trim_end().ends_with("</svg>"));
    check!(svg.contains("viewBox="));
}

/// The three modes differ only in how the glyphs travel, so they must agree on
/// how much room those glyphs need.
#[test]
fn every_mode_agrees_on_the_canvas() {
    let outline = svg_width(&svg(TextMode::Outline, None));
    let embed = svg_width(&svg(TextMode::Embed, None));
    let live = svg_width(&svg(TextMode::Live, None));

    check!(outline == live);
    check!(embed == live);
}

/// The headline guarantee: a caller who swaps `.svg` for `.png` gets the same
/// banner, not a differently-sized one.
#[rstest]
fn svg_and_png_agree_on_the_canvas(
    #[values(TextMode::Outline, TextMode::Embed, TextMode::Live)] mode: TextMode,
) {
    let svg_bytes = svg(mode, None);
    let png = render(OutputForm::Absolute, OutputFormat::Png, mode, None);
    check!(svg_width(&svg_bytes) == png_width(&png));
}

/// What the estimated-advance guess could never promise: the drawn glyphs
/// actually fit, and the canvas is not padded out well beyond them.
///
/// Rasterization zooms out by a fixed factor about the centre, so the ink
/// spans `ZOOM` times the shaped advance rather than all of it.
#[rstest]
fn the_canvas_fits_the_glyphs_it_was_measured_for(
    #[values(Family::Inter, Family::RobotoMono, Family::Arimo)] family: Family,
) {
    let svg = svg(TextMode::Live, Some(family));
    let width = svg_width(&svg);
    let (left, right) = ink_columns(&svg);

    check!(left > 0);
    check!(right < width - 1);

    let text = "2023-11-14 22:13:20 UTC";
    let expected = font::measure(family, text, FONT_SIZE) * ZOOM;
    let measured = f64::from(right - left + 1);

    // A glyph's ink stops short of its advance by its side bearings, so the
    // painted extent is a little narrower than the advance it was measured
    // from. Anything beyond a few pixels means the two have drifted apart.
    check!((expected - measured).abs() < 6.0);
}

#[test]
fn outlined_output_carries_no_text_and_names_no_font() {
    let svg = svg(TextMode::Outline, None);
    check!(!svg.contains("<text"));
    check!(!svg.contains("font-family"));
    check!(svg.contains("<path"));
}

#[test]
fn embedded_output_keeps_the_text_selectable() {
    let svg = svg(TextMode::Embed, None);
    check!(svg.contains("<text"));
    check!(svg.contains("2023-11-14 22:13:20 UTC"));
    check!(svg.contains("@font-face"));
}

/// An embedded document has to survive a round trip through a real SVG
/// consumer, not merely look right as a string.
#[test]
fn embedded_output_parses_back_as_svg() {
    let svg = svg(TextMode::Embed, None);
    let rasterizer = Rasterizer::new();
    let tree = rasterizer
        .parse(svg.as_bytes())
        .expect("embedded SVG parses");
    check!(tree.size().width() as u32 == svg_width(&svg));
}

/// `?format=` reaches the drawn string, and the response is served as
/// `image/svg+xml`, so markup in that value must not survive as markup.
#[rstest]
fn a_format_string_cannot_inject_markup(
    #[values(TextMode::Outline, TextMode::Embed, TextMode::Live)] mode: TextMode,
) {
    let now = Timestamp::from_second(1_700_000_000).unwrap();
    let svg = String::from_utf8(
        RenderContext {
            value: now,
            output_form: OutputForm::Absolute,
            output_format: OutputFormat::Svg,
            tz: TimeZone::UTC,
            now,
            format: Some("</text><script>alert(1)</script>".to_string()),
            locale: None,
            font: None,
            text_mode: mode,
        }
        .render()
        .expect("render succeeds")
        .bytes,
    )
    .expect("SVG output is UTF-8");

    check!(!svg.contains("<script"));
}

#[test]
fn the_requested_face_is_the_one_that_draws() {
    let now = Timestamp::from_second(1_700_000_000).unwrap();
    let rendered = RenderContext {
        value: now,
        output_form: OutputForm::Absolute,
        output_format: OutputFormat::Svg,
        tz: TimeZone::UTC,
        now,
        format: None,
        locale: None,
        font: Some(Family::Arimo),
        text_mode: TextMode::Live,
    }
    .render()
    .expect("render succeeds");

    check!(rendered.font.as_deref() == Some("Arimo"));
}

/// Absolute output is digits and wants a stable width; relative output is
/// words and reads better proportional.
#[rstest]
#[case(OutputForm::Absolute, "Roboto Mono")]
#[case(OutputForm::Relative, "Inter")]
fn each_mode_has_its_own_default_face(#[case] form: OutputForm, #[case] expected: &str) {
    let now = Timestamp::from_second(1_700_000_000).unwrap();
    let rendered = RenderContext {
        value: now,
        output_form: form,
        output_format: OutputFormat::Svg,
        tz: TimeZone::UTC,
        now,
        format: None,
        locale: None,
        font: None,
        text_mode: TextMode::Live,
    }
    .render()
    .expect("render succeeds");

    check!(rendered.font.as_deref() == Some(expected));
}

/// The clock draws no text, so there is no face to report and nothing to
/// outline.
#[test]
fn the_clock_reports_no_face() {
    let now = Timestamp::from_second(1_700_000_000).unwrap();
    let rendered = RenderContext {
        value: now,
        output_form: OutputForm::Clock,
        output_format: OutputFormat::Svg,
        tz: TimeZone::UTC,
        now,
        format: None,
        locale: None,
        font: None,
        text_mode: TextMode::Outline,
    }
    .render()
    .expect("render succeeds");

    check!(rendered.font == None);
}

/// `PADDING_X` on each side is the whole difference between the shaped
/// advance and the canvas, so a longer string widens the canvas by exactly
/// what its extra glyphs advance.
#[test]
fn the_canvas_is_the_advance_plus_its_padding() {
    let text = "2023-11-14 22:13:20 UTC";
    let advance = font::measure(Family::RobotoMono, text, FONT_SIZE);
    let expected = (advance + PADDING_X * 2.0).round() as u32;
    check!(svg_width(&svg(TextMode::Live, Some(Family::RobotoMono))) == expected);
}
