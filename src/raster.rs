use resvg::usvg::fontdb;
use resvg::{tiny_skia, usvg};

/// Errors that can occur during SVG rasterization.
#[derive(Debug, Clone)]
pub struct RenderError {
    pub message: Option<String>,
}

impl std::fmt::Display for RenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        if self.message.is_none() {
            write!(f, "RenderError")
        } else {
            write!(f, "RenderError: {}", self.message.as_ref().unwrap())
        }
    }
}

pub struct Rasterizer {
    font_db: fontdb::Database,
}

impl Rasterizer {
    /// Creates a new rasterizer and loads available fonts.
    pub fn new() -> Self {
        let mut fontdb = fontdb::Database::new();
        fontdb.load_system_fonts();
        fontdb.load_fonts_dir(if cfg!(debug_assertions) {
            "src/fonts"
        } else {
            "fonts"
        });

        Self { font_db: fontdb }
    }

    /// Converts SVG data to PNG.
    pub fn render(&self, svg_data: Vec<u8>) -> Result<Vec<u8>, RenderError> {
        let tree = {
            let opt = usvg::Options {
                fontdb: std::sync::Arc::new(self.font_db.clone()),
                ..Default::default()
            };
            let tree_result = usvg::Tree::from_data(&svg_data, &opt);
            if tree_result.is_err() {
                return Err(RenderError {
                    message: Some("Failed to parse".to_string()),
                });
            }

            tree_result.unwrap()
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

        pixmap.encode_png().map_err(|_| RenderError {
            message: Some("Failed to encode".to_string()),
        })
    }
}
