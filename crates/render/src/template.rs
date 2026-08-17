use std::sync::LazyLock;

use chrono::{DateTime, Timelike, Utc};
use serde::Serialize;
use tera::{Context, Tera};
use timeago::Formatter;

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
pub enum OutputForm {
    /// Relative display: "2 hours ago", "in 3 days"
    Relative,
    /// Absolute display: "2025-01-17 14:30:00 UTC"  
    Absolute,
    /// Clock display: analog clock with hands showing the time
    Clock,
}

/// Timezone specification formats (currently unused but reserved for future features).
#[allow(dead_code)]
pub enum TzForm {
    Abbreviation(String), // e.g. "CST"
    Iso(String),          // e.g. "America/Chicago"
    Offset(i32),          // e.g. "-0600" as -21600
}

/// Context passed to template renderer containing all necessary data.
pub struct RenderContext {
    pub value: DateTime<Utc>,
    pub output_form: OutputForm,
    #[allow(dead_code)]
    pub output_format: OutputFormat,
    /// Target timezone (not yet implemented - defaults to UTC)
    #[allow(dead_code)]
    pub timezone: Option<TzForm>,
    /// Custom time format string (not yet implemented)
    #[allow(dead_code)]
    pub format: Option<String>,
    /// Reference instant relative values are computed against.
    pub now: DateTime<Utc>,
}

/// Calculates clock hand positions for a given time.
///
/// Returns (hour_x, hour_y, minute_x, minute_y) coordinates for SVG rendering.
/// Clock center is at (16, 16) with appropriate hand lengths for a 32x32 favicon.
fn calculate_clock_hands(time: DateTime<Utc>) -> (f64, f64, f64, f64) {
    let hour = time.hour() as f64;
    let minute = time.minute() as f64;

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

/// Renders a time value using the appropriate template.
///
/// Uses different templates based on output form:
/// - Relative/Absolute: "basic.svg" with text content
/// - Clock: "clock.svg" with calculated hand positions
pub fn render_template(context: RenderContext) -> Result<String, tera::Error> {
    let mut template_context = Context::new();

    match context.output_form {
        OutputForm::Relative => {
            let mut formatter = Formatter::new();
            let text = if context.value > context.now {
                formatter.ago("from now");
                formatter.convert_chrono(context.now, context.value)
            } else {
                formatter.convert_chrono(context.value, context.now)
            };
            insert_basic_text(&mut template_context, &text);
            TEMPLATES.render("basic.svg", &template_context)
        }
        OutputForm::Absolute => {
            insert_basic_text(&mut template_context, &context.value.to_rfc3339());
            TEMPLATES.render("basic.svg", &template_context)
        }
        OutputForm::Clock => {
            let (hour_x, hour_y, minute_x, minute_y) = calculate_clock_hands(context.value);

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
    }
}

/// A single live example shown on the index page.
#[derive(Serialize)]
struct Example {
    label: &'static str,
    path: String,
}

/// Renders the index page, with live example image URLs computed from `now`.
pub fn render_index_page(now: DateTime<Utc>) -> Result<String, tera::Error> {
    let epoch = now.timestamp();

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
    use super::*;

    #[test]
    fn index_page_renders() {
        let html = render_index_page(Utc::now()).expect("index page should render");
        assert!(html.contains("time-banner"));
        assert!(html.contains("/favicon.ico"));
    }

    #[test]
    fn basic_svg_declares_explicit_size() {
        let now = Utc::now();
        let svg = render_template(RenderContext {
            value: now,
            output_form: OutputForm::Absolute,
            output_format: crate::pipeline::OutputFormat::Svg,
            timezone: None,
            format: None,
            now,
        })
        .expect("basic.svg should render");
        assert!(svg.contains("viewBox="));
        assert!(!svg.contains("width=\"0\""));
    }

    #[test]
    fn future_relative_time_does_not_render_unknown() {
        let now = Utc::now();
        let future = now + chrono::Duration::hours(1);
        let svg = render_template(RenderContext {
            value: future,
            output_form: OutputForm::Relative,
            output_format: crate::pipeline::OutputFormat::Svg,
            timezone: None,
            format: None,
            now,
        })
        .expect("basic.svg should render");
        assert!(!svg.contains("???"));
        assert!(svg.contains("from now"));
    }
}
