use std::io::Cursor;
use std::sync::LazyLock;

use jiff::{Timestamp, tz::TimeZone};

use crate::error::RenderError;
use crate::raster::Rasterizer;
use crate::template::{OutputForm, RenderContext, render_template};

/// Shared rasterizer, built once and reused across requests.
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
pub fn render_time(context: RenderContext) -> Result<Vec<u8>, RenderError> {
    let output_format = context.output_format.clone();
    let rendered_template = render_template(context)?;

    handle_rasterize(rendered_template, &output_format)
}

/// Generates PNG bytes for the favicon clock, with hands in `tz`.
pub fn generate_favicon_png_bytes(time: Timestamp, tz: TimeZone) -> Result<Vec<u8>, RenderError> {
    render_time(RenderContext {
        value: time,
        output_form: OutputForm::Clock,
        output_format: OutputFormat::Png,
        tz,
        now: time,
    })
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

#[cfg(test)]
mod tests {
    use assert2::assert;
    use rstest::rstest;

    use super::*;

    const PNG_SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1a, b'\n'];

    /// Every (mode, format) pairing that exists today must round-trip
    /// through the full pipeline into recognizable bytes for that format,
    /// not just "some bytes" (a smoke test the benchmark doesn't provide,
    /// since criterion never asserts on its input).
    #[rstest]
    fn render_time_produces_valid_output(
        #[values(OutputForm::Absolute, OutputForm::Relative)] form: OutputForm,
        #[values(OutputFormat::Svg, OutputFormat::Png)] format: OutputFormat,
    ) {
        let now = Timestamp::from_second(1_700_000_000).unwrap();
        let bytes = render_time(RenderContext {
            value: now,
            output_form: form,
            output_format: format.clone(),
            tz: TimeZone::UTC,
            now,
        })
        .unwrap();

        match format {
            OutputFormat::Svg => {
                let svg = String::from_utf8(bytes).expect("SVG output must be valid UTF-8");
                assert!(svg.contains("<svg"));
            }
            OutputFormat::Png => {
                assert!(bytes.starts_with(&PNG_SIGNATURE));
            }
        }
    }
}
