use chrono::Duration;
use lazy_static::lazy_static;
use regex::Regex;

pub trait Months {
    fn months(count: i32) -> Self;
}

impl Months for Duration {
    fn months(count: i32) -> Self {
        Duration::milliseconds(
            (Duration::days(1).num_milliseconds() as f64 * (365.25f64 / 12f64)) as i64,
        ) * count
    }
}

lazy_static! {
    static ref FULL_RELATIVE_PATTERN: Regex = Regex::new(concat!(
        "(?<sign>[-+])?",
        r"(?:(?<year>\d+)\s?(?:y|yrs?|years?))?",
        r"(?:(?<month>\d+)\s?(?:mon|months?))?",
        r"(?:(?<week>\d+)\s?(?:w|wks?|weeks?))?",
        r"(?:(?<day>\d+)\s?(?:d|days?))?",
        r"(?:(?<hour>\d+)\s?(?:h|hrs?|hours?))?",
        r"(?:(?<minute>\d+)\s?(?:m|mins?|minutes?))?",
        r"(?:(?<second>\d+)\s?(?:s|secs?|seconds?))?"
    ))
    .unwrap();
}

pub fn parse_duration(str: &str) -> Result<Duration, String> {
    let capture = FULL_RELATIVE_PATTERN.captures(str).unwrap();
    let mut value = Duration::zero();

    if let Some(raw_year) = capture.name("year") {
        value += match raw_year.as_str().parse::<i64>() {
            Ok(year) => {
                Duration::days(year * 365)
                    + (if year > 0 {
                        Duration::hours(6) * year as i32
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

#[cfg(test)]
mod tests {
    use crate::relative::{parse_duration, Months};
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
            Ok(Duration::hours(365 * 24 + 6)
                + Duration::months(2)
                + Duration::days(3 * 7 + 4)
                + Duration::hours(5)
                + Duration::minutes(6)
                + Duration::seconds(7))
        );
        assert_eq!(
            parse_duration("19year33weeks4d9min"),
            Ok(Duration::hours((365 * 24 + 6) * 19)
                + Duration::days(33 * 7 + 4)
                + Duration::minutes(9))
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
        assert_eq!(0, parse_duration("0mon").unwrap().num_minutes());
        assert_eq!(131490, parse_duration("3mon").unwrap().num_minutes());
        assert_eq!(-613620, parse_duration("-14mon").unwrap().num_minutes());
        assert_eq!(6311520, parse_duration("+144months").unwrap().num_minutes());
    }

    #[test]
    fn parse_week() {
        assert_eq!(parse_duration("0w"), Ok(Duration::zero()));
        assert_eq!(parse_duration("7w"), Ok(Duration::days(7 * 7)));
        assert_eq!(parse_duration("19week"), Ok(Duration::days(7 * 19)));
        assert_eq!(parse_duration("433weeks"), Ok(Duration::days(7 * 433)));
    }

    #[test]
    fn parse_day() {
        assert_eq!(parse_duration("0d"), Ok(Duration::zero()));
        assert_eq!(parse_duration("9d"), Ok(Duration::days(9)));
        assert_eq!(parse_duration("43day"), Ok(Duration::days(43)));
        assert_eq!(parse_duration("969days"), Ok(Duration::days(969)));
    }

    #[test]
    fn parse_hour() {
        assert_eq!(parse_duration("0h"), Ok(Duration::zero()));
        assert_eq!(parse_duration("4h"), Ok(Duration::hours(4)));
        assert_eq!(parse_duration("150hour"), Ok(Duration::hours(150)));
        assert_eq!(parse_duration("777hours"), Ok(Duration::hours(777)));
    }

    #[test]
    fn parse_minute() {
        assert_eq!(parse_duration("0m"), Ok(Duration::zero()));
        assert_eq!(parse_duration("5m"), Ok(Duration::minutes(5)));
        assert_eq!(parse_duration("60min"), Ok(Duration::minutes(60)));
        assert_eq!(parse_duration("999minutes"), Ok(Duration::minutes(999)));
    }

    #[test]
    fn parse_second() {
        assert_eq!(parse_duration("0s"), Ok(Duration::zero()));
        assert_eq!(parse_duration("6s"), Ok(Duration::seconds(6)));
        assert_eq!(parse_duration("60sec"), Ok(Duration::minutes(1)));
        assert_eq!(parse_duration("999seconds"), Ok(Duration::seconds(999)));
    }
}
