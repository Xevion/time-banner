//! Templating, rasterization, and encoding pipeline for time-banner.

pub mod error;
pub mod pipeline;
pub mod raster;
pub mod template;

pub use error::RenderError;
pub use pipeline::{OutputFormat, convert_png_to_ico, generate_favicon_png_bytes};
pub use template::{OutputForm, RenderContext};
