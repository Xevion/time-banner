use resvg::usvg::{fontdb, TreeParsing, TreeTextToPath};
use resvg::{tiny_skia, usvg};

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
    pub fn new() -> Self {
        let mut fontdb = fontdb::Database::new();
        fontdb.load_system_fonts();
        fontdb.load_fonts_dir("./fonts");

        Self { font_db: fontdb }
    }

    pub fn render(&self, svg_data: Vec<u8>) -> Result<Vec<u8>, RenderError> {
        let tree = {
            let opt = usvg::Options::default();
            let mut tree_result = usvg::Tree::from_data(&*svg_data, &opt);
            if tree_result.is_err() {
                return Err(RenderError {
                    message: Some("Failed to parse".to_string()),
                });
            }

            let tree = tree_result.as_mut().unwrap();
            tree.convert_text(&self.font_db);

            resvg::Tree::from_usvg(&tree)
        };

        let pixmap_size = tree.size.to_int_size();
        let mut pixmap = tiny_skia::Pixmap::new(pixmap_size.width(), pixmap_size.height()).unwrap();
        tree.render(tiny_skia::Transform::default(), &mut pixmap.as_mut());

        pixmap.encode_png().map_err(|_| RenderError {
            message: Some("Failed to encode".to_string()),
        })
    }
}
