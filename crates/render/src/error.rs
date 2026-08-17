/// Errors from the render pipeline: templating, rasterization, or icon encoding.
///
/// Each variant keeps the underlying error as its `source`, so the original
/// cause survives instead of being flattened into a string at the point of
/// failure.
#[derive(Debug, thiserror::Error)]
pub enum RenderError {
    #[error("{context}: {source}")]
    Template {
        context: &'static str,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },
    #[error("{context}: {source}")]
    Rasterize {
        context: &'static str,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },
    #[error("{context}: {source}")]
    Encode {
        context: &'static str,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },
}

impl RenderError {
    pub(crate) fn template(
        context: &'static str,
        source: impl std::error::Error + Send + Sync + 'static,
    ) -> Self {
        Self::Template {
            context,
            source: Box::new(source),
        }
    }

    pub(crate) fn rasterize(
        context: &'static str,
        source: impl std::error::Error + Send + Sync + 'static,
    ) -> Self {
        Self::Rasterize {
            context,
            source: Box::new(source),
        }
    }

    pub(crate) fn encode(
        context: &'static str,
        source: impl std::error::Error + Send + Sync + 'static,
    ) -> Self {
        Self::Encode {
            context,
            source: Box::new(source),
        }
    }
}
