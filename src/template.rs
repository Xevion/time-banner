use chrono::{DateTime, Utc};
use lazy_static::lazy_static;
use tera::{Context, Tera};
use timeago::Formatter;

use crate::render::OutputFormat;

lazy_static! {
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

pub enum OutputForm {
    Relative,
    Absolute,
}

pub enum TzForm {
    Abbreviation(String), // e.g. "CST"
    Iso(String),          // e.g. "America/Chicago"
    Offset(i32),          // e.g. "-0600" as -21600
}

pub struct RenderContext {
    pub value: DateTime<Utc>,
    pub output_form: OutputForm,
    pub output_format: OutputFormat,
    pub timezone: Option<TzForm>,
    pub format: Option<String>,
    pub now: Option<i64>,
}

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
