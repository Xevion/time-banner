//! The time-banner value grammar: epoch seconds, ISO 8601 instants, dates,
//! durations, and intervals, plus the human-friendly shorthand layered on
//! top (`now`, compact dates, `+1y2d3h`).
//!
//! See `docs/SPEC.md` section 3 for the full grammar and section 3.1 for the
//! digit-count rule that disambiguates compact dates from epoch seconds.

use std::sync::LazyLock;

use jiff::{
    Span, Timestamp, ToSpan,
    civil::{Date, Time},
    tz::TimeZone,
};
use regex::Regex;

use crate::error::ParseError;

/// Regex pattern matching human-readable duration strings with flexible
/// ordering and abbreviations.
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
/// Anchored so any leftover, unmatched text (garbage units, out-of-order
/// components) fails the whole match rather than being silently ignored.
static FULL_RELATIVE_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(concat!(
        "^",
        "(?<sign>[-+])?",
        r"(?:(?<year>\d+)\s?(?:years?|yrs?|y)\s*)?",
        r"(?:(?<month>\d+)\s?(?:months?|mon)\s*)?",
        r"(?:(?<week>\d+)\s?(?:weeks?|wks?|w)\s*)?",
        r"(?:(?<day>\d+)\s?(?:days?|d)\s*)?",
        r"(?:(?<hour>\d+)\s?(?:hours?|hrs?|h)\s*)?",
        r"(?:(?<minute>\d+)\s?(?:minutes?|mins?|m)\s*)?",
        r"(?:(?<second>\d+)\s?(?:seconds?|secs?|s)\s*)?",
        "$"
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
/// `parse_time_value`. Empty (or whitespace-only) strings return a zero span.
pub fn parse_duration(str: &str) -> Result<Span, ParseError> {
    let trimmed = str.trim();
    let capture = FULL_RELATIVE_PATTERN
        .captures(trimmed)
        .ok_or_else(|| ParseError::UnrecognizedValue(str.to_string()))?;

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

/// Exactly 8 ASCII digits, e.g. `"20270101"`. Digit count alone decides this
/// is a compact date rather than epoch seconds (section 3.1).
fn is_compact_date(s: &str) -> bool {
    s.len() == 8 && s.bytes().all(|b| b.is_ascii_digit())
}

/// Parses an 8-digit compact date (`YYYYMMDD`) into a civil `Date`.
///
/// jiff's ISO 8601 parser only accepts the extended, hyphenated form, so the
/// basic form has to be split and validated by hand.
fn parse_compact_date(digits: &str) -> Result<Date, ParseError> {
    let invalid = || ParseError::UnrecognizedValue(digits.to_string());
    let year = digits[0..4].parse::<i16>().map_err(|_| invalid())?;
    let month = digits[4..6].parse::<i8>().map_err(|_| invalid())?;
    let day = digits[6..8].parse::<i8>().map_err(|_| invalid())?;
    Date::new(year, month, day).map_err(|_| invalid())
}

/// Resolves a civil date to the `Timestamp` at midnight in `tz`.
fn date_to_timestamp(date: Date, tz: &TimeZone) -> Result<Timestamp, ParseError> {
    date.to_zoned(tz.clone())
        .map(|zoned| zoned.timestamp())
        .map_err(|_| ParseError::OutOfRange)
}

/// Parses `date@time` (e.g. `"2027-01-01@14:30"`) into a `Timestamp`.
///
/// The date half accepts either the ISO extended or compact form; the time
/// half is a civil `Time` (`HH:MM` or `HH:MM:SS`, optionally with fractional
/// seconds).
fn parse_date_and_time(
    date_part: &str,
    time_part: &str,
    raw: &str,
    tz: &TimeZone,
) -> Result<Timestamp, ParseError> {
    let invalid = || ParseError::UnrecognizedValue(raw.to_string());

    let date = if is_compact_date(date_part) {
        parse_compact_date(date_part)?
    } else {
        date_part.parse::<Date>().map_err(|_| invalid())?
    };
    let time: Time = time_part.parse().map_err(|_| invalid())?;

    date.to_datetime(time)
        .to_zoned(tz.clone())
        .map(|zoned| zoned.timestamp())
        .map_err(|_| ParseError::OutOfRange)
}

/// Adds `span` to `reference`, resolving calendar units (years, months, ...)
/// against `tz` since they need a civil date to make sense of.
fn apply_span(span: Span, reference: Timestamp, tz: &TimeZone) -> Result<Timestamp, ParseError> {
    let zoned = reference.to_zoned(tz.clone());
    zoned
        .checked_add(span)
        .map(|z| z.timestamp())
        .map_err(|_| ParseError::OutOfRange)
}

/// Parses a value relative to `now`: signed seconds (`"+3600"`), a human
/// duration (`"+1y2d3h"`), or a signed ISO 8601 duration (`"+P1Y2D"`).
fn parse_relative_value(
    trimmed: &str,
    now: Timestamp,
    tz: &TimeZone,
) -> Result<Timestamp, ParseError> {
    if let Ok(offset_seconds) = trimmed.parse::<i64>() {
        return apply_span(offset_seconds.seconds(), now, tz);
    }

    let is_negative = trimmed.starts_with('-');
    let body = &trimmed[1..];

    if body.starts_with('P') || body.starts_with('p') {
        let mut span: Span = body
            .parse()
            .map_err(|_| ParseError::UnrecognizedValue(trimmed.to_string()))?;
        if is_negative {
            span = span.negate();
        }
        return apply_span(span, now, tz);
    }

    let span = parse_duration(trimmed)?;
    apply_span(span, now, tz)
}

/// Parses a time value against the full grammar (section 3):
///
/// - `"now"`: the reference instant
/// - Signed seconds, human durations, or signed ISO durations, relative to `now`
/// - Epoch seconds (any digit count other than exactly 8)
/// - A compact date (exactly 8 digits, `YYYYMMDD`)
/// - `date@time` (ISO or compact date, `@`, civil time)
/// - An ISO 8601 instant (`2027-01-01T00:00:00Z`)
/// - An ISO 8601 date (`2027-01-01`), midnight in `tz`
///
/// `now` is the reference instant relative values are computed against, and
/// `tz` the zone that gives civil dates and calendar spans their meaning.
pub fn parse_time_value(
    raw_time: &str,
    now: Timestamp,
    tz: &TimeZone,
) -> Result<Timestamp, ParseError> {
    let trimmed = raw_time.trim();

    if trimmed == "now" {
        return Ok(now);
    }

    if trimmed.starts_with('+') || trimmed.starts_with('-') {
        return parse_relative_value(trimmed, now, tz);
    }

    if is_compact_date(trimmed) {
        return date_to_timestamp(parse_compact_date(trimmed)?, tz);
    }

    if let Some((date_part, time_part)) = trimmed.split_once('@') {
        return parse_date_and_time(date_part, time_part, raw_time, tz);
    }

    if let Ok(epoch) = trimmed.parse::<i64>() {
        return parse_epoch_into_timestamp(epoch).ok_or(ParseError::OutOfRange);
    }

    if let Ok(timestamp) = trimmed.parse::<Timestamp>() {
        return Ok(timestamp);
    }

    if let Ok(date) = trimmed.parse::<Date>() {
        return date_to_timestamp(date, tz);
    }

    Err(ParseError::UnrecognizedValue(raw_time.to_string()))
}

/// Parses an interval (`progress` only, section 3): `start/end` or
/// `start/P1Y` (start plus an unsigned ISO 8601 duration).
pub fn parse_interval(
    raw: &str,
    now: Timestamp,
    tz: &TimeZone,
) -> Result<(Timestamp, Timestamp), ParseError> {
    let invalid = || ParseError::UnrecognizedValue(raw.to_string());
    let (start_part, end_part) = raw.split_once('/').ok_or_else(invalid)?;

    let start = parse_time_value(start_part, now, tz)?;

    let end = if end_part.starts_with('P') || end_part.starts_with('p') {
        let span: Span = end_part.parse().map_err(|_| invalid())?;
        apply_span(span, start, tz)?
    } else {
        parse_time_value(end_part, now, tz)?
    };

    Ok((start, end))
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
    #[case::out_of_order("1d2y")]
    #[case::garbage("banana")]
    #[case::signed_garbage("+banana")]
    fn rejects_malformed_duration_strings(#[case] input: &str) {
        check!(let Err(ParseError::UnrecognizedValue(_)) = parse_duration(input));
    }

    #[rstest]
    #[case::epoch_seconds("1752170474", 0, 1752170474)]
    #[case::signed_seconds_forward("+3600", 1_000_000, 1_003_600)]
    #[case::signed_seconds_backward("-1800", 1_000_000, 998_200)]
    #[case::human_duration("+1d", 0, 86_400)]
    #[case::now_literal("now", 1_700_000_000, 1_700_000_000)]
    #[case::iso_duration_forward("+P1DT2H", 0, 93_600)]
    #[case::iso_duration_backward("-P1D", 100_000, 13_600)]
    #[case::iso_instant_utc("2027-01-01T00:00:00Z", 0, 1_798_761_600)]
    #[case::iso_instant_with_offset("2027-01-01T00:00:00-05:00", 0, 1_798_779_600)]
    #[case::iso_date("2027-01-01", 0, 1_798_761_600)]
    #[case::compact_date("20270101", 0, 1_798_761_600)]
    #[case::date_and_time("2027-01-01@14:30", 0, 1_798_813_800)]
    #[case::compact_date_and_time("20270101@14:30", 0, 1_798_813_800)]
    #[case::nine_digit_string_is_epoch_not_compact_date("999999999", 0, 999_999_999)]
    fn parses_a_recognized_value(
        #[case] raw: &str,
        #[case] now_epoch: i64,
        #[case] expected_epoch: i64,
    ) {
        let now = Timestamp::from_second(now_epoch).unwrap();
        check!(
            parse_time_value(raw, now, &TimeZone::UTC)
                .unwrap()
                .as_second()
                == expected_epoch
        );
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
    #[case::invalid_compact_date(
        "20271301",
        ParseError::UnrecognizedValue("20271301".to_string())
    )]
    fn rejects_an_unparseable_value(#[case] raw: &str, #[case] expected: ParseError) {
        let now = Timestamp::UNIX_EPOCH;
        check!(parse_time_value(raw, now, &TimeZone::UTC).unwrap_err() == expected);
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

    #[rstest]
    #[case::instant_pair(
        "2026-01-01T00:00:00Z/2027-01-01T00:00:00Z",
        1_767_225_600,
        1_798_761_600
    )]
    #[case::date_pair("2026-01-01/2027-01-01", 1_767_225_600, 1_798_761_600)]
    #[case::start_and_duration("2026-01-01/P1Y", 1_767_225_600, 1_798_761_600)]
    fn parses_an_interval(
        #[case] raw: &str,
        #[case] expected_start_epoch: i64,
        #[case] expected_end_epoch: i64,
    ) {
        let now = Timestamp::UNIX_EPOCH;
        let (start, end) = parse_interval(raw, now, &TimeZone::UTC).unwrap();
        check!(start.as_second() == expected_start_epoch);
        check!(end.as_second() == expected_end_epoch);
    }

    #[rstest]
    #[case::iso_date("2027-01-01")]
    #[case::compact_date("20270101")]
    fn a_civil_date_is_midnight_in_the_given_zone(#[case] raw: &str) {
        let chicago = TimeZone::get("America/Chicago").unwrap();
        let parsed = parse_time_value(raw, Timestamp::UNIX_EPOCH, &chicago).unwrap();

        // Midnight in Chicago is 06:00 UTC in January.
        check!(parsed.as_second() == 1_798_761_600 + 6 * 3600);
    }

    /// A calendar day is a civil unit, so crossing a spring-forward
    /// transition advances 23 hours of absolute time, not 24.
    #[test]
    fn a_calendar_span_resolves_against_local_civil_dates() {
        let chicago = TimeZone::get("America/Chicago").unwrap();
        let midnight_before_transition: Timestamp = "2025-03-09T06:00:00Z".parse().unwrap();

        let parsed = parse_time_value("+1d", midnight_before_transition, &chicago).unwrap();

        check!(parsed.as_second() - midnight_before_transition.as_second() == 23 * 3600);
    }

    #[test]
    fn interval_without_a_separator_is_unrecognized() {
        let now = Timestamp::UNIX_EPOCH;
        check!(let Err(ParseError::UnrecognizedValue(_)) = parse_interval("2026-01-01", now, &TimeZone::UTC));
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
            let _ = parse_time_value(&s, Timestamp::UNIX_EPOCH, &TimeZone::UTC);
        }

        #[test]
        fn parse_duration_never_panics(s in "[-+]?[0-9]{0,6}[a-zA-Z]{0,8}[0-9]{0,6}[a-zA-Z]{0,8}") {
            let _ = parse_duration(&s);
        }

        #[test]
        fn parse_interval_never_panics(s in ".{0,64}") {
            let _ = parse_interval(&s, Timestamp::UNIX_EPOCH, &TimeZone::UTC);
        }

        /// Every epoch second within the representable range round-trips
        /// through the value grammar unchanged, provided it isn't exactly 8
        /// digits (which the grammar reads as a compact date instead).
        #[test]
        fn epoch_round_trips(epoch in -100_000_000_000i64..100_000_000_000i64) {
            prop_assume!(epoch.to_string().trim_start_matches('-').len() != 8);
            let now = Timestamp::UNIX_EPOCH;
            let parsed = parse_time_value(&epoch.to_string(), now, &TimeZone::UTC).unwrap();
            prop_assert_eq!(parsed.as_second(), epoch);
        }

        /// Signed-second offsets are relative to `now` and round-trip through
        /// plain addition, independent of the duration grammar.
        #[test]
        fn signed_seconds_round_trip(offset in -1_000_000i64..1_000_000i64) {
            let now = Timestamp::from_second(1_700_000_000).unwrap();
            let sign = if offset < 0 { "" } else { "+" };
            let raw = format!("{sign}{offset}");
            let parsed = parse_time_value(&raw, now, &TimeZone::UTC).unwrap();
            prop_assert_eq!(parsed.as_second(), now.as_second() + offset);
        }
    }
}
