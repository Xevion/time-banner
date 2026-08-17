use std::io::Cursor;
use std::sync::LazyLock;

use chrono::{DateTime, Utc};

use crate::error::RenderError;
use crate::raster::Rasterizer;
use crate::template::{OutputForm, RenderContext, render_template};

/// Shared rasterizer, built once. Loading system fonts is too expensive to
/// repeat per request.
static RASTERIZER: LazyLock<Rasterizer> = LazyLock::new(Rasterizer::new);

/// Output format for rendered time banners.
#[derive(Debug, Clone)]
pub enum OutputFormat {
    Svg,
    Png,
}

impl OutputFormat {
    /// Determines output format from file extension. Defaults to SVG for unknown extensions.
    pub fn from_extension(ext: &str) -> Self {
        match ext {
            "png" => OutputFormat::Png,
            _ => OutputFormat::Svg,
        }
    }

    #[allow(dead_code)]
    pub fn from_mime_type(mime_type: &str) -> Self {
        match mime_type {
            "image/svg+xml" => OutputFormat::Svg,
            "image/png" => OutputFormat::Png,
            _ => OutputFormat::Svg,
        }
    }

    /// Returns the appropriate MIME type for HTTP responses.
    pub fn mime_type(&self) -> &'static str {
        match self {
            OutputFormat::Svg => "image/svg+xml",
            OutputFormat::Png => "image/png",
        }
    }
}

/// Converts SVG to the requested format. PNG requires rasterization.
pub fn handle_rasterize(data: String, format: &OutputFormat) -> Result<Vec<u8>, RenderError> {
    match format {
        OutputFormat::Svg => Ok(data.into_bytes()),
        OutputFormat::Png => RASTERIZER.render(data.into_bytes()),
    }
}

/// Main rendering pipeline: template -> SVG -> optional rasterization -> encoded bytes.
///
/// `now` is the reference instant relative values are computed against.
pub fn render_time(
    time: DateTime<Utc>,
    now: DateTime<Utc>,
    output_form: OutputForm,
    output_format: OutputFormat,
) -> Result<Vec<u8>, RenderError> {
    let context = RenderContext {
        value: time,
        output_form,
        output_format: output_format.clone(),
        timezone: None,
        format: None,
        now,
    };

    let rendered_template = render_template(context)
        .map_err(|e| RenderError::Template(format!("Template rendering failed: {}", e)))?;

    handle_rasterize(rendered_template, &output_format)
}

/// Generates PNG bytes for the favicon clock.
pub fn generate_favicon_png_bytes(time: DateTime<Utc>) -> Result<Vec<u8>, RenderError> {
    let context = RenderContext {
        value: time,
        output_form: OutputForm::Clock,
        output_format: OutputFormat::Png,
        timezone: None,
        format: None,
        now: time,
    };

    let rendered_template = render_template(context)
        .map_err(|e| RenderError::Template(format!("Template rendering failed: {}", e)))?;

    handle_rasterize(rendered_template, &OutputFormat::Png)
}

/// Converts PNG bytes to ICO format using the ico crate.
pub fn convert_png_to_ico(png_bytes: &[u8]) -> Result<Vec<u8>, RenderError> {
    let mut icon_dir = ico::IconDir::new(ico::ResourceType::Icon);

    let cursor = Cursor::new(png_bytes);
    let image = ico::IconImage::read_png(cursor)
        .map_err(|e| RenderError::Encode(format!("Failed to read PNG data: {}", e)))?;

    icon_dir.add_entry(
        ico::IconDirEntry::encode(&image)
            .map_err(|e| RenderError::Encode(format!("Failed to encode icon entry: {}", e)))?,
    );

    let mut ico_buffer = Vec::new();
    icon_dir
        .write(&mut ico_buffer)
        .map_err(|e| RenderError::Encode(format!("Failed to write ICO data: {}", e)))?;

    Ok(ico_buffer)
}
