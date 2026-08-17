//! Timezone resolution: IANA identifiers, abbreviations, fixed offsets, and
//! offsets written against a `UTC` or `GMT` prefix.
//!
//! See `docs/SPEC.md` section 6 for the accepted forms and section 6.1 for
//! how ambiguous abbreviations are decided.

use jiff::tz::{Offset, TimeZone};

use crate::abbr_tz;
use crate::error::ParseError;

/// Longest spec worth examining. The longest real IANA identifier is well
/// under this, so anything longer is garbage and is rejected before any
/// lookup runs.
const MAX_SPEC_LEN: usize = 64;

/// Largest offset jiff will represent, in whole hours.
const MAX_OFFSET_HOURS: i32 = 25;

/// Resolves a timezone spec to a `TimeZone`.
///
/// Forms are tried in order: `auto`, abbreviation, prefixed offset, fixed
/// offset, IANA identifier. Abbreviations are tried before the IANA lookup
/// deliberately: tzdb carries legacy `EST`, `MST`, and `HST` zones that never
/// observe daylight saving, and letting those win would defeat the point of
/// mapping an abbreviation to the region that uses it.
///
/// `~` substitutes for `/` anywhere in the spec, so an identifier survives
/// path position (`America~Chicago`).
pub fn resolve(spec: &str) -> Result<TimeZone, ParseError> {
    let trimmed = spec.trim();
    let unknown = || ParseError::UnknownTimezone(trimmed.chars().take(MAX_SPEC_LEN).collect());

    if trimmed.is_empty() || trimmed.len() > MAX_SPEC_LEN {
        return Err(unknown());
    }

    // `auto` always resolves to UTC here: geolocation needs a client
    // address, which is a request-level concern this pure spec-parser
    // doesn't have. `crate::geoip` does the actual lookup; the server
    // intercepts `auto` before it reaches this function and only falls
    // back to this branch (matching section 6.2's best-effort miss) when no
    // geoip database is configured or the address doesn't resolve.
    if trimmed.eq_ignore_ascii_case("auto") {
        return Ok(TimeZone::UTC);
    }

    let spec = trimmed.replace('~', "/");

    if spec.chars().all(|c| c.is_ascii_alphabetic())
        && let Some(entry) = abbr_tz::lookup(&spec.to_ascii_uppercase())
    {
        return TimeZone::get(entry.zone).map_err(|_| unknown());
    }

    if let Some(body) = strip_offset_prefix(&spec)
        && let Some(offset) = parse_offset(body)
    {
        return Ok(TimeZone::fixed(offset));
    }

    if let Some(offset) = parse_offset(&spec) {
        return Ok(TimeZone::fixed(offset));
    }

    TimeZone::get(&spec).map_err(|_| unknown())
}

/// Strips a `UTC` or `GMT` prefix, returning the offset that followed it.
fn strip_offset_prefix(spec: &str) -> Option<&str> {
    let (prefix, rest) = spec.split_at_checked(3)?;
    (prefix.eq_ignore_ascii_case("UTC") || prefix.eq_ignore_ascii_case("GMT")).then_some(rest)
}

/// Parses a signed offset: `-6`, `-06`, `-0600`, or `-06:00`.
///
/// The sign is read literally, so `UTC-6` is six hours behind UTC. This is
/// the ISO reading, and the opposite of the POSIX `TZ` convention, where the
/// same string means six hours ahead.
fn parse_offset(body: &str) -> Option<Offset> {
    let sign = match body.as_bytes().first()? {
        b'+' => 1,
        b'-' => -1,
        _ => return None,
    };
    let digits = &body[1..];

    let (hours, minutes) = match digits.split_once(':') {
        Some(split) => split,
        None if matches!(digits.len(), 3 | 4) => digits.split_at(digits.len() - 2),
        None => (digits, "0"),
    };

    let hours: i32 = hours.parse().ok()?;
    let minutes: i32 = minutes.parse().ok()?;
    if hours > MAX_OFFSET_HOURS || minutes > 59 {
        return None;
    }

    Offset::from_seconds(sign * (hours * 3600 + minutes * 60)).ok()
}

#[cfg(test)]
mod tests {
    use assert2::check;
    use jiff::Timestamp;
    use rstest::rstest;

    use super::*;

    /// A summer instant, so zones that observe daylight saving are visibly
    /// distinct from their standard offset.
    fn probe() -> Timestamp {
        "2025-07-15T12:00:00Z".parse().unwrap()
    }

    #[rstest]
    #[case::iana("America/Chicago", "America/Chicago")]
    #[case::iana_tilde_substituted("America~Chicago", "America/Chicago")]
    #[case::iana_lowercase("america/chicago", "America/Chicago")]
    #[case::abbreviation("CST", "America/Chicago")]
    #[case::abbreviation_lowercase("cst", "America/Chicago")]
    #[case::utc("UTC", "UTC")]
    #[case::gmt("GMT", "UTC")]
    #[case::auto_without_geolocation("auto", "UTC")]
    #[case::surrounding_whitespace(" Asia/Tokyo ", "Asia/Tokyo")]
    fn resolves_to_a_named_zone(#[case] spec: &str, #[case] expected: &str) {
        check!(resolve(spec).unwrap().iana_name() == Some(expected));
    }

    /// tzdb ships a legacy `EST` zone that is a flat -05:00 with no daylight
    /// saving. The abbreviation must win over it, or `?tz=EST` renders an
    /// hour off all summer.
    #[test]
    fn abbreviation_shadows_the_legacy_tzdb_zone() {
        check!(resolve("EST").unwrap().iana_name() == Some("America/New_York"));
    }

    #[rstest]
    #[case::compact("-0600", -6 * 3600)]
    #[case::extended("-06:00", -6 * 3600)]
    #[case::hours_only("-06", -6 * 3600)]
    #[case::single_digit_hour("+5", 5 * 3600)]
    #[case::half_hour("+05:30", 5 * 3600 + 30 * 60)]
    #[case::prefixed("UTC-6", -6 * 3600)]
    #[case::prefixed_gmt("GMT+5:30", 5 * 3600 + 30 * 60)]
    #[case::prefixed_lowercase("utc-6", -6 * 3600)]
    fn resolves_to_a_fixed_offset(#[case] spec: &str, #[case] expected_seconds: i32) {
        let tz = resolve(spec).unwrap();
        check!(tz.to_offset(probe()).seconds() == expected_seconds);
    }

    #[rstest]
    #[case::empty("")]
    #[case::whitespace("   ")]
    #[case::unknown_identifier("Mars/Olympus")]
    #[case::unknown_abbreviation("XYZ")]
    #[case::bare_digits("0600")]
    #[case::offset_beyond_range("+99")]
    #[case::minutes_beyond_range("+06:99")]
    #[case::overlong("America/Chicago/America/Chicago/America/Chicago/America/Chicago/America")]
    fn rejects_an_unrecognized_spec(#[case] spec: &str) {
        check!(let Err(ParseError::UnknownTimezone(_)) = resolve(spec));
    }

    #[test]
    fn abbreviation_follows_daylight_saving() {
        let tz = resolve("CST").unwrap();
        check!(tz.to_offset(probe()).seconds() == -5 * 3600);
    }
}
