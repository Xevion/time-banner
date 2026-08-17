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
    use super::*;
    use jiff::{Span, ToSpan};

    #[test]
    fn parse_empty() {
        assert_eq!(parse_duration("").unwrap().fieldwise(), Span::new());
        assert_eq!(parse_duration(" ").unwrap().fieldwise(), Span::new());
        assert_eq!(parse_duration("  ").unwrap().fieldwise(), Span::new());
    }

    #[test]
    fn parse_composite() {
        assert_eq!(
            parse_duration("1y2mon3w4d5h6m7s").unwrap().fieldwise(),
            1.years()
                .months(2)
                .weeks(3)
                .days(4)
                .hours(5)
                .minutes(6)
                .seconds(7)
        );
        assert_eq!(
            parse_duration("19year33weeks4d9min").unwrap().fieldwise(),
            19.years().weeks(33).days(4).minutes(9)
        );
    }

    #[test]
    fn parse_year() {
        assert_eq!(parse_duration("1y").unwrap().fieldwise(), 1.years());
        assert_eq!(parse_duration("2year").unwrap().fieldwise(), 2.years());
        assert_eq!(parse_duration("144years").unwrap().fieldwise(), 144.years());
    }

    #[test]
    fn parse_month() {
        assert_eq!(parse_duration("0mon").unwrap().fieldwise(), 0.months());
        assert_eq!(parse_duration("3mon").unwrap().fieldwise(), 3.months());
        assert_eq!(
            parse_duration("-14mon").unwrap().fieldwise(),
            (-14).months()
        );
        assert_eq!(
            parse_duration("+144months").unwrap().fieldwise(),
            144.months()
        );
    }

    #[test]
    fn parse_week() {
        assert_eq!(parse_duration("0w").unwrap().fieldwise(), 0.weeks());
        assert_eq!(parse_duration("7w").unwrap().fieldwise(), 7.weeks());
        assert_eq!(parse_duration("19week").unwrap().fieldwise(), 19.weeks());
        assert_eq!(parse_duration("433weeks").unwrap().fieldwise(), 433.weeks());
    }

    #[test]
    fn parse_day() {
        assert_eq!(parse_duration("0d").unwrap().fieldwise(), 0.days());
        assert_eq!(parse_duration("9d").unwrap().fieldwise(), 9.days());
        assert_eq!(parse_duration("43day").unwrap().fieldwise(), 43.days());
        assert_eq!(parse_duration("969days").unwrap().fieldwise(), 969.days());
    }

    #[test]
    fn parse_hour() {
        assert_eq!(parse_duration("0h").unwrap().fieldwise(), 0.hours());
        assert_eq!(parse_duration("4h").unwrap().fieldwise(), 4.hours());
        assert_eq!(parse_duration("150hour").unwrap().fieldwise(), 150.hours());
        assert_eq!(parse_duration("777hours").unwrap().fieldwise(), 777.hours());
    }

    #[test]
    fn parse_minute() {
        assert_eq!(parse_duration("0m").unwrap().fieldwise(), 0.minutes());
        assert_eq!(parse_duration("5m").unwrap().fieldwise(), 5.minutes());
        assert_eq!(parse_duration("60min").unwrap().fieldwise(), 60.minutes());
        assert_eq!(
            parse_duration("999minutes").unwrap().fieldwise(),
            999.minutes()
        );
    }

    #[test]
    fn parse_second() {
        assert_eq!(parse_duration("0s").unwrap().fieldwise(), 0.seconds());
        assert_eq!(parse_duration("6s").unwrap().fieldwise(), 6.seconds());
        assert_eq!(parse_duration("60sec").unwrap().fieldwise(), 60.seconds());
        assert_eq!(
            parse_duration("999seconds").unwrap().fieldwise(),
            999.seconds()
        );
    }

    #[test]
    fn value_epoch_seconds() {
        let now = Timestamp::UNIX_EPOCH;
        let parsed = parse_time_value("1752170474", now).unwrap();
        assert_eq!(parsed.as_second(), 1752170474);
    }

    #[test]
    fn value_signed_seconds() {
        let now = Timestamp::from_second(1_000_000).unwrap();
        assert_eq!(
            parse_time_value("+3600", now).unwrap().as_second(),
            1_003_600
        );
        assert_eq!(parse_time_value("-1800", now).unwrap().as_second(), 998_200);
    }

    #[test]
    fn value_human_duration() {
        let now = Timestamp::from_second(0).unwrap();
        let parsed = parse_time_value("+1d", now).unwrap();
        assert_eq!(parsed.as_second(), 86_400);
    }

    #[test]
    fn value_unrecognized() {
        let now = Timestamp::UNIX_EPOCH;
        assert_eq!(
            parse_time_value("not-a-time", now).unwrap_err(),
            ParseError::UnrecognizedValue("not-a-time".to_string())
        );
    }

    #[test]
    fn value_epoch_out_of_range() {
        let now = Timestamp::UNIX_EPOCH;
        assert_eq!(
            parse_time_value(&i64::MAX.to_string(), now).unwrap_err(),
            ParseError::OutOfRange
        );
    }

    #[test]
    fn component_out_of_range_reports_the_offending_unit() {
        let err = parse_duration("999999999999999999999y").unwrap_err();
        assert_eq!(
            err,
            ParseError::ComponentOutOfRange {
                component: "year",
                input: "999999999999999999999".to_string(),
            }
        );
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
