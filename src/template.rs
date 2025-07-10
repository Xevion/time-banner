use chrono::{DateTime, FixedOffset, Utc};
use lazy_static::lazy_static;
use tera::{Context, Tera};
use timeago::Formatter;

lazy_static! {
    static ref TEMPLATES: Tera = {
        let mut _tera = match Tera::new("templates/**/*.svg") {
            Ok(t) => {
                let names: Vec<&str> = t.get_template_names().collect();
                println!("{} templates found ([{}]).", names.len(), names.join(", "));
                t
            }
            Err(e) => {
                println!("Parsing error(s): {}", e);
                ::std::process::exit(1);
            }
        };
        _tera
    };
}

pub enum OutputForm {
    Relative,
    Absolute,
}

pub struct RenderContext<'a> {
    pub output_form: OutputForm,
    pub value: DateTime<Utc>,
    pub tz_offset: FixedOffset,
    pub tz_name: &'a str,
    pub view: &'a str,
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
