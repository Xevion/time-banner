/// A time value could not be parsed.
#[derive(Debug, Clone, thiserror::Error)]
#[error("{0}")]
pub struct ParseError(pub String);
