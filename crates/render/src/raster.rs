use std::sync::Arc;

use resvg::usvg::fontdb;
use resvg::{tiny_skia, usvg};

use crate::error::RenderError;

const ARIMO: &[u8] = include_bytes!("../fonts/arimo.ttf");
const INTER: &[u8] = include_bytes!("../fonts/inter.ttf");
const ROBOTO_MONO: &[u8] = include_bytes!("../fonts/RobotoMono.ttf");

pub struct Rasterizer {
    font_db: Arc<fontdb::Database>,
}

impl Default for Rasterizer {
    fn default() -> Self {
        Self::new()
    }
}

impl Rasterizer {
    /// Creates a new rasterizer with the bundled fonts. System fonts are
    /// never loaded: rendering must be identical across machines, and the
    /// templates only ever reference these 3 families.
    pub fn new() -> Self {
        let mut fontdb = fontdb::Database::new();
        fontdb.load_font_data(ARIMO.to_vec());
        fontdb.load_font_data(INTER.to_vec());
        fontdb.load_font_data(ROBOTO_MONO.to_vec());

        Self {
            font_db: Arc::new(fontdb),
        }
    }

    /// Parses SVG source into a usvg tree, resolving fonts against the bundle.
    pub fn parse(&self, svg_data: &[u8]) -> Result<usvg::Tree, RenderError> {
        let opt = usvg::Options {
            fontdb: self.font_db.clone(),
            ..Default::default()
        };
        usvg::Tree::from_data(svg_data, &opt)
            .map_err(|e| RenderError::Rasterize(format!("Failed to parse SVG: {}", e)))
    }

    /// Rasterizes a parsed tree, applying the banner's fixed 10% zoom-out.
    pub fn rasterize(&self, tree: &usvg::Tree) -> tiny_skia::Pixmap {
        let pixmap_size = tree.size().to_int_size();
        let mut pixmap = tiny_skia::Pixmap::new(pixmap_size.width(), pixmap_size.height()).unwrap();

        // Calculate center point for scaling
        let center_x = pixmap_size.width() as f32 / 2.0;
        let center_y = pixmap_size.height() as f32 / 2.0;

        // Create transform that scales from center: translate to center, scale, translate back
        let zoom = 0.90; // 10% zoom out from center
        let render_ts = tiny_skia::Transform::from_translate(-center_x, -center_y)
            .post_scale(zoom, zoom)
            .post_translate(center_x, center_y);

        resvg::render(tree, render_ts, &mut pixmap.as_mut());
        pixmap
    }

    /// Converts SVG data to PNG.
    pub fn render(&self, svg_data: Vec<u8>) -> Result<Vec<u8>, RenderError> {
        let tree = self.parse(&svg_data)?;
        let pixmap = self.rasterize(&tree);
        encode_png(pixmap)
    }
}

/// Encodes a pixmap as PNG, favoring encode speed over file size: banners are
/// small, mostly-flat images generated fresh per request, not archived.
pub fn encode_png(pixmap: tiny_skia::Pixmap) -> Result<Vec<u8>, RenderError> {
    let width = pixmap.width();
    let height = pixmap.height();
    let demultiplied = pixmap.take_demultiplied();

    let mut data = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut data, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_compression(png::Compression::Fast);
        let mut writer = encoder
            .write_header()
            .map_err(|e| RenderError::Rasterize(format!("Failed to encode PNG: {}", e)))?;
        writer
            .write_image_data(&demultiplied)
            .map_err(|e| RenderError::Rasterize(format!("Failed to encode PNG: {}", e)))?;
    }

    Ok(data)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use assert2::{assert, check};
    use rstest::{fixture, rstest};

    use super::*;

    const TINY_SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><rect width="10" height="10" fill="red"/></svg>"#;

    /// Parsing 3 embedded fonts into a fontdb is the expensive part of
    /// `Rasterizer::new`; shared once across every test in this module.
    #[fixture]
    #[once]
    fn rasterizer() -> Rasterizer {
        Rasterizer::new()
    }

    #[rstest]
    fn parses_and_rasterizes(rasterizer: &Rasterizer) {
        let tree = rasterizer.parse(TINY_SVG.as_bytes()).unwrap();
        let pixmap = rasterizer.rasterize(&tree);
        check!(pixmap.width() == 10);
        check!(pixmap.height() == 10);
    }

    #[rstest]
    fn encoded_png_has_a_valid_signature(rasterizer: &Rasterizer) {
        let tree = rasterizer.parse(TINY_SVG.as_bytes()).unwrap();
        let pixmap = rasterizer.rasterize(&tree);
        let bytes = encode_png(pixmap).unwrap();
        assert!(bytes.starts_with(&[0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1a, b'\n']));
    }

    /// Every bundled font file must be a font `fontdb` can actually load:
    /// a corrupted or truncated fetch should fail loudly here, not at
    /// first render in production.
    #[rstest]
    fn bundled_font_files_parse(#[files("fonts/*.ttf")] path: PathBuf) {
        let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let mut db = fontdb::Database::new();
        db.load_font_data(bytes);
        assert!(!db.is_empty());
    }
}
