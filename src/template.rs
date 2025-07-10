use chrono::{DateTime, Utc};
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
}

/// Timezone specification formats.
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

/// Renders a time value using the appropriate template.
pub fn render_template(context: RenderContext) -> Result<String, tera::Error> {
    let mut template_context = Context::new();
    let formatter = Formatter::new();

    template_context.insert(
        "text",
        match context.output_form {
            OutputForm::Relative => formatter.convert_chrono(context.value, Utc::now()),
            OutputForm::Absolute => context.value.to_rfc3339(),
        }
        .as_str(),
    );

    TEMPLATES.render("basic.svg", &template_context)
}
