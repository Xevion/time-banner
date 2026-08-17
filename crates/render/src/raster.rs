use std::sync::Arc;

use resvg::usvg::fontdb;
use resvg::{tiny_skia, usvg};

use crate::error::RenderError;

pub struct Rasterizer {
    font_db: Arc<fontdb::Database>,
}

impl Default for Rasterizer {
    fn default() -> Self {
        Self::new()
    }
}

impl Rasterizer {
    /// Creates a new rasterizer and loads available fonts. Expensive (scans
    /// system font directories); build once and reuse across requests.
    pub fn new() -> Self {
        let mut fontdb = fontdb::Database::new();
        fontdb.load_system_fonts();
        fontdb.load_fonts_dir("fonts");

        Self {
            font_db: Arc::new(fontdb),
        }
    }

    /// Converts SVG data to PNG.
    pub fn render(&self, svg_data: Vec<u8>) -> Result<Vec<u8>, RenderError> {
        let tree = {
            let opt = usvg::Options {
                fontdb: self.font_db.clone(),
                ..Default::default()
            };
            usvg::Tree::from_data(&svg_data, &opt)
                .map_err(|e| RenderError::Rasterize(format!("Failed to parse SVG: {}", e)))?
        };

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

        resvg::render(&tree, render_ts, &mut pixmap.as_mut());

        pixmap
            .encode_png()
            .map_err(|e| RenderError::Rasterize(format!("Failed to encode PNG: {}", e)))
    }
}
