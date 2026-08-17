//! Human-readable duration parsing with support for mixed time units.
//!
//! Parses strings like "1y2mon3w4d5h6m7s", "+1year", or "-3h30m" into a
//! `jiff::Span`. Time units can appear in any order and use various
//! abbreviations.

use std::sync::LazyLock;

use jiff::{Span, Timestamp, ToSpan, tz::TimeZone};
use regex::Regex;

use crate::error::ParseError;

/// Regex pattern matching duration strings with flexible ordering and abbreviations.
///
/// Supports:
/// - Optional +/- sign
/// - Years: y, yr, yrs, year, years
/// - Months: mon, month, months
/// - Weeks: w, wk, wks, week, weeks
/// - Days: d, day, days
/// - Hours: h, hr, hrs, hour, hours
/// - Minutes: m, min, mins, minute, minutes
/// - Seconds: s, sec, secs, second, seconds
///
/// Time units must appear in descending order of magnitude, e.g. "1y2d" is valid, "1d2y" is not.
static FULL_RELATIVE_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(concat!(
        "(?<sign>[-+])?",
        r"(?:(?<year>\d+)\s?(?:years?|yrs?|y)\s*)?",
        r"(?:(?<month>\d+)\s?(?:months?|mon)\s*)?",
        r"(?:(?<week>\d+)\s?(?:weeks?|wks?|w)\s*)?",
        r"(?:(?<day>\d+)\s?(?:days?|d)\s*)?",
        r"(?:(?<hour>\d+)\s?(?:hours?|hrs?|h)\s*)?",
        r"(?:(?<minute>\d+)\s?(?:minutes?|mins?|m)\s*)?",
        r"(?:(?<second>\d+)\s?(?:seconds?|secs?|s)\s*)?"
    ))
    .unwrap()
});

/// Parses a human-readable duration string into a `Span`.
///
/// Examples:
/// - `"1y2d"` → 1 year + 2 days
/// - `"+3h30m"` → +3.5 hours
/// - `"-1week"` → -7 days
/// - `"2months4days"` → 2 months + 4 days
///
/// Units are kept symbolic (a "year" is not resolved to a fixed number of
/// seconds); resolving them against a reference instant happens in
/// `parse_time_value`. Empty strings return a zero span.
pub fn parse_duration(str: &str) -> Result<Span, ParseError> {
    let capture = FULL_RELATIVE_PATTERN.captures(str).unwrap();

    // The regex only ever captures '+' or '-' here, so no other value is reachable.
    let sign: i64 = if capture.name("sign").map(|m| m.as_str()) == Some("-") {
        -1
    } else {
        1
    };

    // A duration component's digits can fit an i64 yet still exceed the
    // range a `Span` accepts for that unit (e.g. years beyond +-19998), so
    // the fallible `try_*` setters are used instead of the panicking ones.
    let mut span = Span::new();
    span = apply_unit(span, "year", capture.name("year"), sign, Span::try_years)?;
    span = apply_unit(span, "month", capture.name("month"), sign, Span::try_months)?;
    span = apply_unit(span, "week", capture.name("week"), sign, Span::try_weeks)?;
    span = apply_unit(span, "day", capture.name("day"), sign, Span::try_days)?;
    span = apply_unit(span, "hour", capture.name("hour"), sign, Span::try_hours)?;
    span = apply_unit(
        span,
        "minute",
        capture.name("minute"),
        sign,
        Span::try_minutes,
    )?;
    span = apply_unit(
        span,
        "second",
        capture.name("second"),
        sign,
        Span::try_seconds,
    )?;
    Ok(span)
}

/// Applies one matched duration component to `span` via a checked setter,
/// reporting the offending component and its raw text on failure.
fn apply_unit(
    span: Span,
    component: &'static str,
    matched: Option<regex::Match>,
    sign: i64,
    setter: impl Fn(Span, i64) -> Result<Span, jiff::Error>,
) -> Result<Span, ParseError> {
    let Some(m) = matched else {
        return Ok(span);
    };
    let out_of_range = || ParseError::ComponentOutOfRange {
        component,
        input: m.as_str().to_string(),
    };
    let n = m.as_str().parse::<i64>().map_err(|_| out_of_range())?;
    setter(span, n * sign).map_err(|_| out_of_range())
}

/// Converts a Unix epoch timestamp to a `Timestamp`.
pub fn parse_epoch_into_timestamp(epoch: i64) -> Option<Timestamp> {
    Timestamp::from_second(epoch).ok()
}

/// Parses various time value formats into a `Timestamp`.
///
/// Supports:
/// - Relative offsets: "+3600", "-1800" (seconds from `now`)
/// - Duration strings: "+1y2d", "-3h30m" (using duration parser, relative to `now`)
/// - Epoch timestamps: "1752170474" (Unix timestamp)
///
/// `now` is the reference instant relative values are computed against.
pub fn parse_time_value(raw_time: &str, now: Timestamp) -> Result<Timestamp, ParseError> {
    // Handle relative time values (starting with + or -, or duration strings like "1y2d")
    if raw_time.starts_with('+') || raw_time.starts_with('-') {
        let span = if let Ok(offset_seconds) = raw_time.parse::<i64>() {
            offset_seconds.seconds()
        } else {
            parse_duration(raw_time)?
        };

        // Calendar units (years, months, ...) need a reference date to
        // resolve, so route through a UTC-zoned view of `now`.
        let zoned = now.to_zoned(TimeZone::UTC);
        let result = zoned
            .checked_add(span)
            .map_err(|_| ParseError::OutOfRange)?;
        return Ok(result.timestamp());
    }

    // Try to parse as epoch timestamp
    if let Ok(epoch) = raw_time.parse::<i64>() {
        return parse_epoch_into_timestamp(epoch).ok_or(ParseError::OutOfRange);
    }

    Err(ParseError::UnrecognizedValue(raw_time.to_string()))
}

#[cfg(test)]
mod tests {
    use assert2::check;
    use jiff::{Span, ToSpan};
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case::empty("")]
    #[case::single_space(" ")]
    #[case::double_space("  ")]
    fn empty_input_is_a_zero_span(#[case] input: &str) {
        check!(parse_duration(input).unwrap().fieldwise() == Span::new());
    }

    #[rstest]
    #[case::every_unit_in_order(
        "1y2mon3w4d5h6m7s",
        1.years().months(2).weeks(3).days(4).hours(5).minutes(6).seconds(7)
    )]
    #[case::sparse_units(
        "19year33weeks4d9min",
        19.years().weeks(33).days(4).minutes(9)
    )]
    fn composite_duration_combines_every_matched_unit(#[case] input: &str, #[case] expected: Span) {
        check!(parse_duration(input).unwrap().fieldwise() == expected.fieldwise());
    }

    #[rstest]
    #[case::bare("1y", 1)]
    #[case::full_word("2year", 2)]
    #[case::plural("144years", 144)]
    fn parses_years(#[case] input: &str, #[case] n: i64) {
        check!(parse_duration(input).unwrap().fieldwise() == n.years());
    }

    #[rstest]
    #[case::zero("0mon", 0)]
    #[case::bare("3mon", 3)]
    #[case::negative("-14mon", -14)]
    #[case::plural_with_sign("+144months", 144)]
    fn parses_months(#[case] input: &str, #[case] n: i64) {
        check!(parse_duration(input).unwrap().fieldwise() == n.months());
    }

    #[rstest]
    #[case::zero("0w", 0)]
    #[case::bare("7w", 7)]
    #[case::full_word("19week", 19)]
    #[case::plural("433weeks", 433)]
    fn parses_weeks(#[case] input: &str, #[case] n: i64) {
        check!(parse_duration(input).unwrap().fieldwise() == n.weeks());
    }

    #[rstest]
    #[case::zero("0d", 0)]
    #[case::bare("9d", 9)]
    #[case::full_word("43day", 43)]
    #[case::plural("969days", 969)]
    fn parses_days(#[case] input: &str, #[case] n: i64) {
        check!(parse_duration(input).unwrap().fieldwise() == n.days());
    }

    #[rstest]
    #[case::zero("0h", 0)]
    #[case::bare("4h", 4)]
    #[case::full_word("150hour", 150)]
    #[case::plural("777hours", 777)]
    fn parses_hours(#[case] input: &str, #[case] n: i64) {
        check!(parse_duration(input).unwrap().fieldwise() == n.hours());
    }

    #[rstest]
    #[case::zero("0m", 0)]
    #[case::bare("5m", 5)]
    #[case::abbreviated("60min", 60)]
    #[case::plural("999minutes", 999)]
    fn parses_minutes(#[case] input: &str, #[case] n: i64) {
        check!(parse_duration(input).unwrap().fieldwise() == n.minutes());
    }

    #[rstest]
    #[case::zero("0s", 0)]
    #[case::bare("6s", 6)]
    #[case::abbreviated("60sec", 60)]
    #[case::plural("999seconds", 999)]
    fn parses_seconds(#[case] input: &str, #[case] n: i64) {
        check!(parse_duration(input).unwrap().fieldwise() == n.seconds());
    }

    #[rstest]
    #[case::epoch_seconds("1752170474", 0, 1752170474)]
    #[case::signed_seconds_forward("+3600", 1_000_000, 1_003_600)]
    #[case::signed_seconds_backward("-1800", 1_000_000, 998_200)]
    #[case::human_duration("+1d", 0, 86_400)]
    fn parses_a_recognized_value(
        #[case] raw: &str,
        #[case] now_epoch: i64,
        #[case] expected_epoch: i64,
    ) {
        let now = Timestamp::from_second(now_epoch).unwrap();
        check!(parse_time_value(raw, now).unwrap().as_second() == expected_epoch);
    }

    #[rstest]
    #[case::unrecognized_value(
        "not-a-time",
        ParseError::UnrecognizedValue("not-a-time".to_string())
    )]
    #[case::epoch_beyond_timestamp_range(
        &i64::MAX.to_string(),
        ParseError::OutOfRange
    )]
    fn rejects_an_unparseable_value(#[case] raw: &str, #[case] expected: ParseError) {
        let now = Timestamp::UNIX_EPOCH;
        check!(parse_time_value(raw, now).unwrap_err() == expected);
    }

    #[rstest]
    // Digits alone overflow i64, failing before the `Span` setter runs.
    #[case::digits_exceed_i64(
        "999999999999999999999y",
        ParseError::ComponentOutOfRange {
            component: "year",
            input: "999999999999999999999".to_string(),
        }
    )]
    // Digits fit i64 but exceed the `Span` type's per-unit bound.
    #[case::digits_exceed_span_bound(
        "20000y",
        ParseError::ComponentOutOfRange {
            component: "year",
            input: "20000".to_string(),
        }
    )]
    fn component_out_of_range_reports_the_offending_unit(
        #[case] input: &str,
        #[case] expected: ParseError,
    ) {
        check!(parse_duration(input).unwrap_err() == expected);
    }
}

#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        /// The parser is a request-facing entry point over untrusted input;
        /// it must reject cleanly rather than panic, no matter what arrives.
        #[test]
        fn parse_time_value_never_panics(s in ".{0,64}") {
            let _ = parse_time_value(&s, Timestamp::UNIX_EPOCH);
        }

        #[test]
        fn parse_duration_never_panics(s in "[-+]?[0-9]{0,6}[a-zA-Z]{0,8}[0-9]{0,6}[a-zA-Z]{0,8}") {
            let _ = parse_duration(&s);
        }

        /// Every epoch second within the representable range round-trips
        /// through the value grammar unchanged.
        #[test]
        fn epoch_round_trips(epoch in -100_000_000_000i64..100_000_000_000i64) {
            let now = Timestamp::UNIX_EPOCH;
            let parsed = parse_time_value(&epoch.to_string(), now).unwrap();
            prop_assert_eq!(parsed.as_second(), epoch);
        }

        /// Signed-second offsets are relative to `now` and round-trip through
        /// plain addition, independent of the duration grammar.
        #[test]
        fn signed_seconds_round_trip(offset in -1_000_000i64..1_000_000i64) {
            let now = Timestamp::from_second(1_700_000_000).unwrap();
            let sign = if offset < 0 { "" } else { "+" };
            let raw = format!("{sign}{offset}");
            let parsed = parse_time_value(&raw, now).unwrap();
            prop_assert_eq!(parsed.as_second(), now.as_second() + offset);
        }
    }
}
