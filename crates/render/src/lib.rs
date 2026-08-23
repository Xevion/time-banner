//! Templating, rasterization, and encoding pipeline for time-banner.

pub mod error;
pub mod font;
mod format;
pub mod locale;
pub mod pipeline;
pub mod raster;
pub mod svg_text;
pub mod template;

pub use error::RenderError;
pub use font::Family;
pub use pipeline::{OutputFormat, Rendered, convert_png_to_ico, generate_favicon_png_bytes};
pub use svg_text::TextMode;
pub use template::{OutputForm, RenderContext};
