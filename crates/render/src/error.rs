/// Errors from the render pipeline: templating, rasterization, or icon encoding.
#[derive(Debug, thiserror::Error)]
pub enum RenderError {
    #[error("template rendering failed: {0}")]
    Template(String),
    #[error("rasterization failed: {0}")]
    Rasterize(String),
    #[error("icon encoding failed: {0}")]
    Encode(String),
}
