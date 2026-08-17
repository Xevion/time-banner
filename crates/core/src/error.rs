/// A time value or duration component could not be parsed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ParseError {
    /// The input matched none of the recognized value grammars (epoch,
    /// signed seconds, or duration).
    #[error("unrecognized time value: {0}")]
    UnrecognizedValue(String),

    /// A duration component's digits did not fit the numeric range.
    #[error("{component} value out of range in {input}")]
    ComponentOutOfRange {
        component: &'static str,
        input: String,
    },

    /// The computed instant fell outside the representable timestamp range.
    #[error("time value out of representable range")]
    OutOfRange,
}

impl ParseError {
    /// Stable machine-readable code for this error, independent of the
    /// human-readable message.
    pub fn code(&self) -> &'static str {
        match self {
            ParseError::UnrecognizedValue(_) => "unrecognized_value",
            ParseError::ComponentOutOfRange { .. } => "component_out_of_range",
            ParseError::OutOfRange => "out_of_range",
        }
    }
}
