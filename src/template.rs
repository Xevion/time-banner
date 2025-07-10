use chrono::{DateTime, Timelike, Utc};
use lazy_static::lazy_static;
use tera::{Context, Tera};
use timeago::Formatter;

use crate::render::OutputFormat;

lazy_static! {
    /// Global Tera template engine instance.
    static ref TEMPLATES: Tera = {
        let template_pattern = if cfg!(debug_assertions) {
            // Development: templates are in src/templates
            "src/templates/**/*.svg"
        } else {
            // Production: templates are in /usr/src/app/templates (relative to working dir)
            "templates/**/*.svg"
        };

        match Tera::new(template_pattern) {
            Ok(t) => {
                let names: Vec<&str> = t.get_template_names().collect();
                println!("{} templates found ([{}]).", names.len(), names.join(", "));
                t
            }
            Err(e) => {
                println!("Parsing error(s): {}", e);
                ::std::process::exit(1);
            }
        }
    };
}

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
pub enum TzForm {
    Abbreviation(String), // e.g. "CST"
    Iso(String),          // e.g. "America/Chicago"
    Offset(i32),          // e.g. "-0600" as -21600
}

/// Context passed to template renderer containing all necessary data.
pub struct RenderContext {
    pub value: DateTime<Utc>,
    pub output_form: OutputForm,
    pub output_format: OutputFormat,
    /// Target timezone (not yet implemented - defaults to UTC)
    pub timezone: Option<TzForm>,
    /// Custom time format string (not yet implemented)
    pub format: Option<String>,
    /// Reference time for relative calculations (not yet implemented - uses current time)
    pub now: Option<i64>,
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

/// Renders a time value using the appropriate template.
///
/// Uses different templates based on output form:
/// - Relative/Absolute: "basic.svg" with text content
/// - Clock: "clock.svg" with calculated hand positions
pub fn render_template(context: RenderContext) -> Result<String, tera::Error> {
    let mut template_context = Context::new();

    match context.output_form {
        OutputForm::Relative => {
            let formatter = Formatter::new();
            template_context.insert("text", &formatter.convert_chrono(context.value, Utc::now()));
            TEMPLATES.render("basic.svg", &template_context)
        }
        OutputForm::Absolute => {
            template_context.insert("text", &context.value.to_rfc3339());
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
