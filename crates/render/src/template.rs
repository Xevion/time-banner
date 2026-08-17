use std::sync::LazyLock;

use jiff::{Timestamp, fmt::strtime, tz::TimeZone};
use serde::Serialize;
use tera::{Context, Tera};
use timeago::Formatter;

use crate::error::RenderError;
use crate::pipeline::OutputFormat;

/// Global Tera template engine instance. Templates are compiled into the
/// binary, so there is no filesystem path to resolve at startup.
static TEMPLATES: LazyLock<Tera> = LazyLock::new(|| {
    let mut tera = Tera::default();
    let sources = [
        ("basic.svg", include_str!("templates/basic.svg")),
        ("clock.svg", include_str!("templates/clock.svg")),
        ("index.html", include_str!("templates/index.html")),
    ];
    if let Err(e) = tera.add_raw_templates(sources) {
        panic!("Template parsing error(s): {}", e);
    }

    let names: Vec<&str> = tera.get_template_names().collect();
    tracing::info!("{} templates found ([{}]).", names.len(), names.join(", "));
    tera
});

/// Display format for time values.
#[derive(Debug, Clone, Copy)]
pub enum OutputForm {
    /// Relative display: "2 hours ago", "in 3 days"
    Relative,
    /// Absolute display: "2025-01-17 14:30:00 UTC"
    Absolute,
    /// Clock display: analog clock with hands showing the time
    Clock,
}

/// Context passed to template renderer containing all necessary data.
pub struct RenderContext {
    pub value: Timestamp,
    pub output_form: OutputForm,
    pub output_format: OutputFormat,
    /// Zone the value is drawn in.
    pub tz: TimeZone,
    /// Reference instant relative values are computed against.
    pub now: Timestamp,
}

/// Calculates clock hand positions for a given time.
///
/// Returns (hour_x, hour_y, minute_x, minute_y) coordinates for SVG rendering.
/// Clock center is at (16, 16) with appropriate hand lengths for a 32x32 favicon.
fn calculate_clock_hands(time: Timestamp, tz: &TimeZone) -> (f64, f64, f64, f64) {
    let zoned = time.to_zoned(tz.clone());
    let hour = zoned.hour() as f64;
    let minute = zoned.minute() as f64;

    // Calculate angles (12 o'clock = 0°, clockwise)
    let hour_angle = ((hour % 12.0) + minute / 60.0) * 30.0; // 30° per hour
    let minute_angle = minute * 6.0; // 6° per minute

    // Convert to radians and adjust for SVG coordinate system (0° at top)
    let hour_rad = (hour_angle - 90.0).to_radians();
    let minute_rad = (minute_angle - 90.0).to_radians();

    // Clock center and hand lengths
    let center_x = 16.0;
    let center_y = 16.0;
    let hour_length = 7.0; // Shorter hour hand
    let minute_length = 11.0; // Longer minute hand

    // Calculate end positions
    let hour_x = center_x + hour_length * hour_rad.cos();
    let hour_y = center_y + hour_length * hour_rad.sin();
    let minute_x = center_x + minute_length * minute_rad.cos();
    let minute_y = center_y + minute_length * minute_rad.sin();

    (hour_x, hour_y, minute_x, minute_y)
}

/// Font size (px) used by "basic.svg", and its approximate monospace
/// character-advance ratio, used to size the SVG canvas to fit the text.
const BASIC_FONT_SIZE: f64 = 27.0;
const BASIC_CHAR_WIDTH_RATIO: f64 = 0.6;
const BASIC_PADDING_X: f64 = 12.0;
const BASIC_HEIGHT: f64 = 34.0;

/// Estimates the pixel width needed to render `text` in "basic.svg"'s monospace font.
fn estimate_basic_width(text: &str) -> f64 {
    let char_width = BASIC_FONT_SIZE * BASIC_CHAR_WIDTH_RATIO;
    text.chars().count() as f64 * char_width + BASIC_PADDING_X * 2.0
}

fn insert_basic_text(template_context: &mut Context, text: &str) {
    template_context.insert("text", text);
    template_context.insert("width", &format!("{:.0}", estimate_basic_width(text)));
    template_context.insert("height", &format!("{:.0}", BASIC_HEIGHT));
}

/// `strftime` pattern absolute output is drawn with. `?format=` will make
/// this the default rather than the only option.
const ABSOLUTE_FORMAT: &str = "%Y-%m-%d %H:%M:%S %Z";

/// Renders a time value using the appropriate template.
///
/// Uses different templates based on output form:
/// - Relative/Absolute: "basic.svg" with text content
/// - Clock: "clock.svg" with calculated hand positions
pub fn render_template(context: RenderContext) -> Result<String, RenderError> {
    let mut template_context = Context::new();

    let rendered = match context.output_form {
        OutputForm::Relative => {
            let elapsed = context.value.duration_since(context.now).unsigned_abs();
            let mut formatter = Formatter::new();
            if context.value > context.now {
                formatter.ago("from now");
            }
            let text = formatter.convert(elapsed);
            insert_basic_text(&mut template_context, &text);
            TEMPLATES.render("basic.svg", &template_context)
        }
        OutputForm::Absolute => {
            let zoned = context.value.to_zoned(context.tz.clone());
            let text = strtime::format(ABSOLUTE_FORMAT, &zoned)
                .map_err(|e| RenderError::Template(format!("time formatting failed: {}", e)))?;
            insert_basic_text(&mut template_context, &text);
            TEMPLATES.render("basic.svg", &template_context)
        }
        OutputForm::Clock => {
            let (hour_x, hour_y, minute_x, minute_y) =
                calculate_clock_hands(context.value, &context.tz);

            // Format to 2 decimal places to avoid precision issues
            let hour_x_str = format!("{:.2}", hour_x);
            let hour_y_str = format!("{:.2}", hour_y);
            let minute_x_str = format!("{:.2}", minute_x);
            let minute_y_str = format!("{:.2}", minute_y);

            template_context.insert("hour_x", &hour_x_str);
            template_context.insert("hour_y", &hour_y_str);
            template_context.insert("minute_x", &minute_x_str);
            template_context.insert("minute_y", &minute_y_str);

            TEMPLATES.render("clock.svg", &template_context)
        }
    };

    rendered.map_err(|e| RenderError::Template(e.to_string()))
}

/// A single live example shown on the index page.
#[derive(Serialize)]
struct Example {
    label: &'static str,
    path: String,
}

/// Renders the index page, with live example image URLs computed from `now`.
pub fn render_index_page(now: Timestamp) -> Result<String, tera::Error> {
    let epoch = now.as_second();

    let examples = vec![
        Example {
            label: "Absolute time",
            path: format!("/absolute/{epoch}"),
        },
        Example {
            label: "Relative time, past",
            path: format!("/relative/{}", epoch - 3600),
        },
        Example {
            label: "Relative time, future",
            path: "/relative/+3600".to_string(),
        },
        Example {
            label: "Absolute time, in a timezone",
            path: format!("/absolute/{epoch}?tz=America/Chicago"),
        },
        Example {
            label: "PNG output",
            path: format!("/relative/{}.png", epoch - 3600),
        },
        Example {
            label: "Analog clock favicon",
            path: "/favicon.ico".to_string(),
        },
    ];

    let mut context = Context::new();
    context.insert("examples", &examples);
    TEMPLATES.render("index.html", &context)
}

#[cfg(test)]
mod tests {
    use assert2::assert;
    use jiff::ToSpan;

    use super::*;

    #[test]
    fn index_page_renders() {
        let html = render_index_page(Timestamp::now()).expect("index page should render");
        assert!(html.contains("time-banner"));
        assert!(html.contains("/favicon.ico"));
    }

    /// A context in UTC, for tests where the zone is not what's under test.
    fn context(value: Timestamp, now: Timestamp, output_form: OutputForm) -> RenderContext {
        RenderContext {
            value,
            output_form,
            output_format: crate::pipeline::OutputFormat::Svg,
            tz: TimeZone::UTC,
            now,
        }
    }

    #[test]
    fn basic_svg_declares_explicit_size() {
        let now = Timestamp::now();
        let svg = render_template(context(now, now, OutputForm::Absolute))
            .expect("basic.svg should render");
        assert!(svg.contains("viewBox="));
        assert!(!svg.contains("width=\"0\""));
    }

    #[test]
    fn future_relative_time_does_not_render_unknown() {
        let now = Timestamp::now();
        let future = now.checked_add(1.hour()).unwrap();
        let svg = render_template(context(future, now, OutputForm::Relative))
            .expect("basic.svg should render");
        assert!(!svg.contains("???"));
        assert!(svg.contains("from now"));
    }

    /// 2023-11-14T22:13:20Z, chosen so the Chicago rendering lands on a
    /// different day part and a different date is not in play.
    fn fixed_instant() -> Timestamp {
        Timestamp::from_second(1_700_000_000).unwrap()
    }

    #[test]
    fn absolute_renders_wall_time_in_the_resolved_zone() {
        let value = fixed_instant();
        let chicago = TimeZone::get("America/Chicago").unwrap();

        let svg = render_template(RenderContext {
            value,
            output_form: OutputForm::Absolute,
            output_format: crate::pipeline::OutputFormat::Svg,
            tz: chicago,
            now: value,
        })
        .expect("basic.svg should render");

        assert!(svg.contains("2023-11-14 16:13:20 CST"));
    }

    #[test]
    fn absolute_renders_the_offset_when_the_zone_has_no_abbreviation() {
        let value = fixed_instant();

        let svg = render_template(RenderContext {
            value,
            output_form: OutputForm::Absolute,
            output_format: crate::pipeline::OutputFormat::Svg,
            tz: TimeZone::fixed(jiff::tz::offset(-6)),
            now: value,
        })
        .expect("basic.svg should render");

        assert!(svg.contains("2023-11-14 16:13:20 -06"));
    }

    #[test]
    fn clock_hands_follow_the_resolved_zone() {
        let value = fixed_instant();
        let chicago = TimeZone::get("America/Chicago").unwrap();

        assert!(
            calculate_clock_hands(value, &TimeZone::UTC) != calculate_clock_hands(value, &chicago)
        );
    }
}
