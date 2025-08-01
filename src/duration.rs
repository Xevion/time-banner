//! Human-readable duration parsing with support for mixed time units.
//!
//! Parses strings like "1y2mon3w4d5h6m7s", "+1year", or "-3h30m" into chrono Duration objects.
//! Time units can appear in any order and use various abbreviations.

use chrono::{DateTime, Duration, Utc};
use lazy_static::lazy_static;
use regex::Regex;

use crate::error::TimeBannerError;

/// Extends chrono::Duration with month support using approximate calendar math.
pub trait Months {
    fn months(count: i32) -> Self;
}

impl Months for Duration {
    /// Creates a duration representing the given number of months.
    /// Uses 365.25/12 ≈ 30.44 days per month for approximation.
    fn months(count: i32) -> Self {
        Duration::milliseconds(
            (Duration::days(1).num_milliseconds() as f64 * (365.25f64 / 12f64)) as i64,
        ) * count
    }
}

lazy_static! {
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
    static ref FULL_RELATIVE_PATTERN: Regex = Regex::new(concat!(
        "(?<sign>[-+])?",
        r"(?:(?<year>\d+)\s?(?:years?|yrs?|y)\s*)?",
        r"(?:(?<month>\d+)\s?(?:months?|mon)\s*)?",
        r"(?:(?<week>\d+)\s?(?:weeks?|wks?|w)\s*)?",
        r"(?:(?<day>\d+)\s?(?:days?|d)\s*)?",
        r"(?:(?<hour>\d+)\s?(?:hours?|hrs?|h)\s*)?",
        r"(?:(?<minute>\d+)\s?(?:minutes?|mins?|m)\s*)?",
        r"(?:(?<second>\d+)\s?(?:seconds?|secs?|s)\s*)?"
    ))
    .unwrap();
}

/// Parses a human-readable duration string into a chrono Duration.
///
/// Examples:
/// - `"1y2d"` → 1 year + 2 days  
/// - `"+3h30m"` → +3.5 hours
/// - `"-1week"` → -7 days
/// - `"2months4days"` → ~2.03 months
///
/// Years include leap year compensation (+6 hours per year).
/// Empty strings return zero duration.
pub fn parse_duration(str: &str) -> Result<Duration, String> {
    let capture = FULL_RELATIVE_PATTERN.captures(str).unwrap();
    let mut value = Duration::zero();

    if let Some(raw_year) = capture.name("year") {
        value += match raw_year.as_str().parse::<i64>() {
            Ok(year) => {
                Duration::days(year * 365)
                    + (if year > 0 {
                        Duration::hours(6) * year as i32 // Leap year compensation
                    } else {
                        Duration::zero()
                    })
            }
            Err(e) => {
                return Err(format!(
                    "Could not parse year from {} ({})",
                    raw_year.as_str(),
                    e
                ))
            }
        };
    }

    if let Some(raw_month) = capture.name("month") {
        value += match raw_month.as_str().parse::<i32>() {
            Ok(month) => Duration::months(month),
            Err(e) => {
                return Err(format!(
                    "Could not parse month from {} ({})",
                    raw_month.as_str(),
                    e
                ))
            }
        };
    }

    if let Some(raw_week) = capture.name("week") {
        value += match raw_week.as_str().parse::<i64>() {
            Ok(week) => Duration::days(7) * week as i32,
            Err(e) => {
                return Err(format!(
                    "Could not parse week from {} ({})",
                    raw_week.as_str(),
                    e
                ))
            }
        };
    }

    if let Some(raw_day) = capture.name("day") {
        value += match raw_day.as_str().parse::<i64>() {
            Ok(day) => Duration::days(day),
            Err(e) => {
                return Err(format!(
                    "Could not parse day from {} ({})",
                    raw_day.as_str(),
                    e
                ))
            }
        };
    }

    if let Some(raw_hour) = capture.name("hour") {
        value += match raw_hour.as_str().parse::<i64>() {
            Ok(hour) => Duration::hours(hour),
            Err(e) => {
                return Err(format!(
                    "Could not parse hour from {} ({})",
                    raw_hour.as_str(),
                    e
                ))
            }
        };
    }

    if let Some(raw_minute) = capture.name("minute") {
        value += match raw_minute.as_str().parse::<i64>() {
            Ok(minute) => Duration::minutes(minute),
            Err(e) => {
                return Err(format!(
                    "Could not parse minute from {} ({})",
                    raw_minute.as_str(),
                    e
                ))
            }
        };
    }

    if let Some(raw_second) = capture.name("second") {
        value += match raw_second.as_str().parse::<i64>() {
            Ok(second) => Duration::seconds(second),
            Err(e) => {
                return Err(format!(
                    "Could not parse second from {} ({})",
                    raw_second.as_str(),
                    e
                ))
            }
        };
    }

    if let Some(raw_sign) = capture.name("sign") {
        match raw_sign.as_str() {
            "-" => value = -value,
            "+" => (),
            _ => return Err(format!("Could not parse sign from {}", raw_sign.as_str())),
        };
    }

    Ok(value)
}

/// Converts Unix epoch timestamp to UTC DateTime.
pub fn parse_epoch_into_datetime(epoch: i64) -> Option<DateTime<Utc>> {
    DateTime::from_timestamp(epoch, 0)
}

/// Parses various time value formats into a UTC datetime.
///
/// Supports:
/// - Relative offsets: "+3600", "-1800" (seconds from now)
/// - Duration strings: "+1y2d", "-3h30m" (using duration parser)
/// - Epoch timestamps: "1752170474" (Unix timestamp)
pub fn parse_time_value(raw_time: &str) -> Result<DateTime<Utc>, TimeBannerError> {
    // Handle relative time values (starting with + or -, or duration strings like "1y2d")
    if raw_time.starts_with('+') || raw_time.starts_with('-') {
        let now = Utc::now();

        // Try parsing as simple offset seconds first
        if let Ok(offset_seconds) = raw_time.parse::<i64>() {
            return Ok(now + Duration::seconds(offset_seconds));
        }

        // Try parsing as duration string (e.g., "+1y2d", "-3h30m")
        if let Ok(duration) = parse_duration(raw_time) {
            return Ok(now + duration);
        }

        return Err(TimeBannerError::ParseError(format!(
            "Could not parse relative time: {}",
            raw_time
        )));
    }

    // Try to parse as epoch timestamp
    if let Ok(epoch) = raw_time.parse::<i64>() {
        return parse_epoch_into_datetime(epoch)
            .ok_or_else(|| TimeBannerError::ParseError("Invalid timestamp".to_string()));
    }

    Err(TimeBannerError::ParseError(format!(
        "Could not parse time value: {}",
        raw_time
    )))
}

#[cfg(test)]
mod tests {
    use crate::duration::{parse_duration, Months};
    use chrono::Duration;

    #[test]
    fn parse_empty() {
        assert_eq!(parse_duration(""), Ok(Duration::zero()));
        assert_eq!(parse_duration(" "), Ok(Duration::zero()));
        assert_eq!(parse_duration("  "), Ok(Duration::zero()));
    }

    #[test]
    fn parse_composite() {
        assert_eq!(
            parse_duration("1y2mon3w4d5h6m7s"),
            Ok(Duration::days(365)
                + Duration::hours(6) // leap year compensation
                + Duration::months(2)
                + Duration::weeks(3)
                + Duration::days(4)
                + Duration::hours(5)
                + Duration::minutes(6)
                + Duration::seconds(7)),
            "1y2mon3w4d5h6m7s"
        );
        assert_eq!(
            parse_duration("19year33weeks4d9min"),
            Ok(Duration::days(365 * 19)
                + Duration::hours(6 * 19)
                + Duration::days(33 * 7 + 4)
                + Duration::minutes(9)),
            "19year33weeks4d9min"
        );
    }

    #[test]
    fn parse_year() {
        assert_eq!(
            parse_duration("1y"),
            Ok(Duration::days(365) + Duration::hours(6))
        );
        assert_eq!(
            parse_duration("2year"),
            Ok(Duration::days(365 * 2) + Duration::hours(6 * 2))
        );
        assert_eq!(
            parse_duration("144years"),
            Ok(Duration::days(365 * 144) + Duration::hours(6 * 144))
        );
    }

    #[test]
    fn parse_month() {
        assert_eq!(Duration::zero(), parse_duration("0mon").unwrap());
        assert_eq!(Duration::months(3), parse_duration("3mon").unwrap());
        assert_eq!(Duration::months(-14), parse_duration("-14mon").unwrap());
        assert_eq!(Duration::months(144), parse_duration("+144months").unwrap());
    }

    #[test]
    fn parse_week() {
        assert_eq!(Duration::zero(), parse_duration("0w").unwrap());
        assert_eq!(Duration::weeks(7), parse_duration("7w").unwrap());
        assert_eq!(Duration::weeks(19), parse_duration("19week").unwrap());
        assert_eq!(Duration::weeks(433), parse_duration("433weeks").unwrap());
    }

    #[test]
    fn parse_day() {
        assert_eq!(Duration::zero(), parse_duration("0d").unwrap());
        assert_eq!(Duration::days(9), parse_duration("9d").unwrap());
        assert_eq!(Duration::days(43), parse_duration("43day").unwrap());
        assert_eq!(Duration::days(969), parse_duration("969days").unwrap());
    }

    #[test]
    fn parse_hour() {
        assert_eq!(Duration::zero(), parse_duration("0h").unwrap());
        assert_eq!(Duration::hours(4), parse_duration("4h").unwrap());
        assert_eq!(Duration::hours(150), parse_duration("150hour").unwrap());
        assert_eq!(Duration::hours(777), parse_duration("777hours").unwrap());
    }

    #[test]
    fn parse_minute() {
        assert_eq!(Duration::zero(), parse_duration("0m").unwrap());
        assert_eq!(Duration::minutes(5), parse_duration("5m").unwrap());
        assert_eq!(Duration::minutes(60), parse_duration("60min").unwrap());
        assert_eq!(
            Duration::minutes(999),
            parse_duration("999minutes").unwrap()
        );
    }

    #[test]
    fn parse_second() {
        assert_eq!(Duration::zero(), parse_duration("0s").unwrap());
        assert_eq!(Duration::seconds(6), parse_duration("6s").unwrap());
        assert_eq!(Duration::minutes(1), parse_duration("60sec").unwrap());
        assert_eq!(
            Duration::seconds(999),
            parse_duration("999seconds").unwrap()
        );
    }
}
