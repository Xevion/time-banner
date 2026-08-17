use std::io::Cursor;
use std::sync::LazyLock;

use jiff::{Timestamp, tz::TimeZone};

use crate::error::RenderError;
use crate::raster::Rasterizer;
use crate::template::{OutputForm, RenderContext};

/// Shared rasterizer, built once and reused across requests.
static RASTERIZER: LazyLock<Rasterizer> = LazyLock::new(Rasterizer::new);

/// Output format for rendered time banners.
#[derive(Debug, Clone)]
pub enum OutputFormat {
    Svg,
    Png,
}

impl From<&str> for OutputFormat {
    /// Determines output format from a file extension. Defaults to SVG for
    /// unknown extensions.
    fn from(ext: &str) -> Self {
        match ext {
            "png" => OutputFormat::Png,
            _ => OutputFormat::Svg,
        }
    }
}

impl OutputFormat {
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

    /// Encodes rendered SVG source into this format. PNG requires rasterization.
    pub fn encode(&self, svg: String) -> Result<Vec<u8>, RenderError> {
        match self {
            OutputFormat::Svg => Ok(svg.into_bytes()),
            OutputFormat::Png => RASTERIZER.render(svg.into_bytes()),
        }
    }
}

impl RenderContext {
    /// Main rendering pipeline: template -> SVG -> optional rasterization -> encoded bytes.
    pub fn render(self) -> Result<Vec<u8>, RenderError> {
        let format = self.output_format.clone();
        let svg = self.render_svg()?;
        format.encode(svg)
    }
}

/// Generates PNG bytes for the favicon clock, with hands in `tz`.
pub fn generate_favicon_png_bytes(time: Timestamp, tz: TimeZone) -> Result<Vec<u8>, RenderError> {
    RenderContext {
        value: time,
        output_form: OutputForm::Clock,
        output_format: OutputFormat::Png,
        tz,
        now: time,
    }
    .render()
}

/// Converts PNG bytes to ICO format using the ico crate.
pub fn convert_png_to_ico(png_bytes: &[u8]) -> Result<Vec<u8>, RenderError> {
    let mut icon_dir = ico::IconDir::new(ico::ResourceType::Icon);

    let cursor = Cursor::new(png_bytes);
    let image = ico::IconImage::read_png(cursor)
        .map_err(|e| RenderError::encode("failed to read PNG data", e))?;

    icon_dir.add_entry(
        ico::IconDirEntry::encode(&image)
            .map_err(|e| RenderError::encode("failed to encode icon entry", e))?,
    );

    let mut ico_buffer = Vec::new();
    icon_dir
        .write(&mut ico_buffer)
        .map_err(|e| RenderError::encode("failed to write ICO data", e))?;

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
        let bytes = RenderContext {
            value: now,
            output_form: form,
            output_format: format.clone(),
            tz: TimeZone::UTC,
            now,
        }
        .render()
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
