use png::EncodingError;
use resvg::{tiny_skia, usvg};
use resvg::usvg::{fontdb, TreeParsing, TreeTextToPath};

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

pub fn get() -> Result<Vec<u8>, RenderError> {
    let rtree = {
        let mut opt = usvg::Options::default();
        // Get file's absolute directory.
        opt.resources_dir = std::fs::canonicalize("")
            .ok()
            .and_then(|p| p.parent().map(|p| p.to_path_buf()));

        let mut fontdb = fontdb::Database::new();
        fontdb.load_system_fonts();
        fontdb.load_fonts_dir("./fonts");

        let svg_data = include_bytes!("../test.svg");

        let mut tree_result = usvg::Tree::from_data(svg_data, &opt);
        if tree_result.is_err() { return Err(RenderError { message: Some("Failed to parse".to_string()) }); }


        let tree = tree_result.as_mut().unwrap();
        tree.convert_text(&fontdb);

        resvg::Tree::from_usvg(&tree)
    };

    let pixmap_size = rtree.size.to_int_size();
    let mut pixmap = tiny_skia::Pixmap::new(pixmap_size.width(), pixmap_size.height()).unwrap();
    rtree.render(tiny_skia::Transform::default(), &mut pixmap.as_mut());

    pixmap.encode_png().map_err(|_| RenderError { message: Some("Failed to encode".to_string()) })
}