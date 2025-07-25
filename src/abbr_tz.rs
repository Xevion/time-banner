use chrono::FixedOffset;

// Generated by build.rs - timezone abbreviation to UTC offset mapping
include!(concat!(env!("OUT_DIR"), "/timezone_map.rs"));

/// Parse a timezone abbreviation into a UTC offset.
///
/// This uses a pre-generated map of timezone abbreviations to their UTC offsets
/// in seconds. The mapping is based on the Wikipedia reference of timezone
/// abbreviations (as of 2023-07-20).
///
/// Note: Timezone abbreviations are not standardized and can be ambiguous.
/// This implementation uses preferred interpretations for conflicting abbreviations.
///
/// # Arguments
/// * `abbreviation` - The timezone abbreviation (e.g., "CST", "EST", "PST")
///
/// # Returns
/// * `Ok(FixedOffset)` - The UTC offset for the timezone
/// * `Err(String)` - Error message if abbreviation is not found or invalid
///
/// # Examples
/// ```
/// use chrono::FixedOffset;
///
/// let cst = parse_abbreviation("CST").unwrap();
/// assert_eq!(cst, FixedOffset::west_opt(6 * 3600).unwrap());
/// ```
pub fn parse_abbreviation(abbreviation: &str) -> Result<FixedOffset, String> {
    let offset_seconds = TIMEZONE_OFFSETS
        .get(abbreviation)
        .ok_or_else(|| format!("Unknown timezone abbreviation: {}", abbreviation))?;

    // Convert seconds to FixedOffset
    // Positive offsets are east of UTC, negative are west
    let offset = if *offset_seconds >= 0 {
        FixedOffset::east_opt(*offset_seconds)
    } else {
        FixedOffset::west_opt(-*offset_seconds)
    };

    offset.ok_or_else(|| {
        format!(
            "Invalid offset for timezone {}: {} seconds",
            abbreviation, offset_seconds
        )
    })
}

#[cfg(test)]
mod tests {
    use crate::abbr_tz::parse_abbreviation;
    use chrono::FixedOffset;

    #[test]
    fn test_parse_cst() {
        // CST (Central Standard Time) is UTC-6
        let cst = parse_abbreviation("CST").unwrap();
        assert_eq!(cst, FixedOffset::west_opt(6 * 3600).unwrap());
    }

    #[test]
    fn test_parse_est() {
        // EST (Eastern Standard Time) is UTC-5
        let est = parse_abbreviation("EST").unwrap();
        assert_eq!(est, FixedOffset::west_opt(5 * 3600).unwrap());
    }

    #[test]
    fn test_parse_utc() {
        // UTC should be zero offset
        let utc = parse_abbreviation("UTC").unwrap();
        assert_eq!(utc, FixedOffset::east_opt(0).unwrap());
    }

    #[test]
    fn test_parse_unknown() {
        // Unknown abbreviation should return error
        let result = parse_abbreviation("INVALID");
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .contains("Unknown timezone abbreviation"));
    }

    #[test]
    fn test_parse_positive_offset() {
        // JST (Japan Standard Time) is UTC+9
        let jst = parse_abbreviation("JST").unwrap();
        assert_eq!(jst, FixedOffset::east_opt(9 * 3600).unwrap());
    }
}
