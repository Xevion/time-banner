//! Bounded `strftime`-style expansion for `?format=`.
//!
//! `jiff::Error` deliberately doesn't distinguish "the format string was
//! invalid" from "the writer rejected the output" (its own docs call
//! introspection limited), so the bound is enforced by a writer that tracks
//! its own overflow rather than by inspecting the error jiff returns.

use jiff::Zoned;
use jiff::fmt::Write;
use jiff::fmt::strtime::BrokenDownTime;

use crate::error::RenderError;

/// Longest format string worth attempting. Real `strftime` patterns are a
/// handful of directives; anything past this is rejected before formatting
/// even starts.
const MAX_FORMAT_INPUT_LEN: usize = 64;

/// Accumulates formatted output, refusing to grow past a byte cap.
struct BoundedWriter<'a> {
    buf: &'a mut String,
    max: usize,
    exceeded: bool,
}

impl Write for BoundedWriter<'_> {
    fn write_str(&mut self, s: &str) -> Result<(), jiff::Error> {
        if self.buf.len() + s.len() > self.max {
            self.exceeded = true;
            return Err(jiff::Error::from_args(format_args!(
                "output exceeded {} bytes",
                self.max
            )));
        }
        self.buf.push_str(s);
        Ok(())
    }
}

/// Formats `zoned` with a user-supplied `strftime` pattern, bounded on both
/// the input pattern's length and the expanded output's length.
pub(crate) fn format_absolute(
    zoned: &Zoned,
    format: &str,
    max_output_bytes: usize,
) -> Result<String, RenderError> {
    if format.len() > MAX_FORMAT_INPUT_LEN {
        return Err(RenderError::format_too_large(MAX_FORMAT_INPUT_LEN));
    }

    let tm = BrokenDownTime::from(zoned);
    let mut buf = String::new();
    let mut writer = BoundedWriter {
        buf: &mut buf,
        max: max_output_bytes,
        exceeded: false,
    };

    match tm.format(format, &mut writer) {
        Ok(()) => Ok(buf),
        Err(_) if writer.exceeded => Err(RenderError::format_too_large(max_output_bytes)),
        Err(e) => Err(RenderError::invalid_format(e)),
    }
}

#[cfg(test)]
mod tests {
    use assert2::{assert, check};
    use jiff::Timestamp;
    use jiff::tz::TimeZone;

    use super::*;

    fn zoned() -> Zoned {
        Timestamp::from_second(1_700_000_000)
            .unwrap()
            .to_zoned(TimeZone::UTC)
    }

    #[test]
    fn renders_a_custom_pattern() {
        let text = format_absolute(&zoned(), "%Y", 512).unwrap();
        check!(text == "2023");
    }

    #[test]
    fn rejects_an_unrecognized_directive() {
        assert!(let Err(RenderError::InvalidFormat { .. }) = format_absolute(&zoned(), "%K", 512));
    }

    #[test]
    fn rejects_an_overlong_input_before_formatting() {
        let format = "%Y".repeat(MAX_FORMAT_INPUT_LEN);
        assert!(
            let Err(RenderError::FormatTooLarge { limit }) = format_absolute(&zoned(), &format, 512)
        );
        check!(limit == MAX_FORMAT_INPUT_LEN);
    }

    #[test]
    fn rejects_expansion_past_the_output_cap() {
        // "%Y-%m-%d" alone is 10 bytes; a cap of 4 trips mid-expansion
        // without needing an oversized input string.
        assert!(
            let Err(RenderError::FormatTooLarge { limit }) = format_absolute(&zoned(), "%Y-%m-%d", 4)
        );
        check!(limit == 4);
    }
}
