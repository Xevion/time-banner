use crate::error::{get_error_response, TimeBannerError};
use crate::raster::Rasterizer;
use crate::template::{render_template, OutputForm, RenderContext};
use axum::body::Bytes;
use axum::http::{header, StatusCode};
use axum::response::IntoResponse;
use chrono::{DateTime, Utc};

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
            _ => OutputFormat::Svg, // Default to SVG
        }
    }

    pub fn from_mime_type(mime_type: &str) -> Self {
        // TODO: Support mime types dynamically, proper header parsing
        match mime_type {
            "image/svg+xml" => OutputFormat::Svg,
            "image/png" => OutputFormat::Png,
            _ => OutputFormat::Svg, // Default to SVG
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
fn handle_rasterize(data: String, format: &OutputFormat) -> Result<Bytes, TimeBannerError> {
    match format {
        OutputFormat::Svg => Ok(Bytes::from(data)),
        OutputFormat::Png => {
            let renderer = Rasterizer::new();
            let raw_image = renderer.render(data.into_bytes());
            if let Err(err) = raw_image {
                return Err(TimeBannerError::RasterizeError(
                    err.message.unwrap_or_else(|| "Unknown error".to_string()),
                ));
            }
            Ok(Bytes::from(raw_image.unwrap()))
        }
    }
}

/// Main rendering pipeline: template → SVG → optional rasterization → HTTP response.
///
/// Takes a timestamp, display format, and file extension, then:
/// 1. Renders the time using a template
/// 2. Converts to the requested format (SVG or PNG)  
/// 3. Returns an HTTP response with appropriate headers
pub fn render_time_response(
    time: DateTime<Utc>,
    output_form: OutputForm,
    extension: &str,
) -> impl IntoResponse {
    let output_format = OutputFormat::from_extension(extension);

    // Build context for rendering
    let context = RenderContext {
        value: time,
        output_form,
        output_format: output_format.clone(),
        timezone: None, // Default to UTC for now
        format: None,   // Use default format
        now: None,      // Use current time
    };

    // Render template
    let rendered_template = match render_template(context) {
        Ok(template) => template,
        Err(e) => {
            return get_error_response(TimeBannerError::RenderError(format!(
                "Template rendering failed: {}",
                e
            )))
            .into_response()
        }
    };

    // Handle rasterization
    match handle_rasterize(rendered_template, &output_format) {
        Ok(bytes) => (
            StatusCode::OK,
            [(header::CONTENT_TYPE, output_format.mime_type())],
            bytes,
        )
            .into_response(),
        Err(e) => get_error_response(e).into_response(),
    }
}
