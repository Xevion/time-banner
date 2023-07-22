use std::str::FromStr;
use regex::{Regex, Captures, Match};
use lazy_static::lazy_static;
use crate::separator::Separable;

lazy_static! {
    // First, second, third groups required (date segments)
    static ref ABSOLUTE_TIME: Regex = Regex::new(r"^(\d+)[.,-: ](\d+)[.,-: ](\d+)[.,-: ](?<time>[^\w]*?(?:PM|AM)?)(?<tz>[.,-: ]\w{2,5})?$").unwrap();
}

#[derive(Debug, Copy, Clone, Default)]
pub enum DateSegmentOrder {
    #[default]
    YearMonthDay,
    MonthDayYear,
    DayMonthYear,
}

impl FromStr for DateSegmentOrder {
    type Err = ();

    /*
    Case-sensitive match on the string representation of the enum.
     */
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "YMD" => Ok(DateSegmentOrder::YearMonthDay),
            "MDY" => Ok(DateSegmentOrder::MonthDayYear),
            "DMY" => Ok(DateSegmentOrder::DayMonthYear),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Copy, Clone, Default)]
pub struct ExtractionOptions {
    date_segment_order: DateSegmentOrder,
    strict: bool,
}

#[derive(Debug, PartialEq)]
pub struct ExtractedTime {
    year: u32,
    month: u32,
    day: u32,
    hour: u32,
    minute: u32,
    second: u32,
    timezone: String,
}

trait ExtractAbsolute {
    fn extract_absolute(&self, options: ExtractionOptions) -> Result<ExtractedTime, String>;
}

impl ExtractAbsolute for String {
    fn extract_absolute(&self, options: ExtractionOptions) -> Result<ExtractedTime, String> {
        let capture: Captures = ABSOLUTE_TIME.captures(self).expect(format!("Invalid absolute time format: {}", self).as_str());
        println!("{:?}", capture);
        let (year, month, day): (u32, u32, u32) = {
            fn as_u32(capture: Option<Match>) -> u32 {
                capture.unwrap().as_str().parse().unwrap()
            }
            let (first, second, third) = (as_u32(capture.get(1)), as_u32(capture.get(2)), as_u32(capture.get(3)));
            match options.date_segment_order {
                DateSegmentOrder::YearMonthDay => {
                    (first, second, third)
                }
                DateSegmentOrder::MonthDayYear => {
                    (third, first, second)
                }
                DateSegmentOrder::DayMonthYear => {
                    (third, second, first)
                }
            }
        };

        let (hour, minute, second): (u32, u32, u32) = if capture.name("time").unwrap().len() > 0 {
            // Split the time segment using the separator characters defined in the Separable trait.
            let mut time = capture.name("time").unwrap().as_str().split(|c: char| c.is_separator());
            println!("{:?}", time);
            let (mut hour, mut minute, mut second) = (0, 0, 0);

            // Iterate over the next four segments as available.
            if let Some(next_hour) = time.next() { hour = next_hour.parse().unwrap(); }
            if let Some(next_minute) = time.next() { minute = next_minute.parse().unwrap(); }
            if let Some(next_second) = time.next() { second = next_second.parse().unwrap(); }
            time.next();  // Skip the milliseconds segment

            // Prevent additional segments from being present.
            let remaining = time.count();
            if remaining > 0 {
                return Err("Invalid time format: too many segments".to_string());
            }

            (hour, minute, second)
        } else {
            (0, 0, 0)
        };

        Ok(ExtractedTime {
            year,
            month,
            day,
            hour,
            minute,
            second,
            timezone: "UTC".to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::parse::{ExtractAbsolute, ExtractedTime};

    #[test]
    fn extract_hour() {
        // 3:00 AM UTC
        assert_eq!("2023-06-14-3".to_string().extract_absolute(Default::default()).unwrap(), ExtractedTime {
            year: 2023,
            month: 6,
            day: 14,
            hour: 3,
            minute: 0,
            second: 0,
            timezone: "UTC".to_string(),
        });

        // 3:00 PM CST
        assert_eq!("2023.06.14.15-CST".to_string().extract_absolute(Default::default()), Ok(ExtractedTime {
            year: 2023,
            month: 6,
            day: 14,
            hour: 15,
            minute: 0,
            second: 0,
            timezone: "CST".to_string(),
        }));

        // 3:00 PM CST
        assert_eq!("2023-06-14-3PM-CST".to_string().extract_absolute(Default::default()), Ok(ExtractedTime {
            year: 2023,
            month: 6,
            day: 14,
            hour: 12 + 3,
            minute: 0,
            second: 0,
            timezone: "CST".to_string(),
        }));
    }

    #[test]
    fn extract_minute() {
        // 3:45 AM UTC
        assert_eq!("2023-06-14-3-45".to_string().extract_absolute(Default::default()), Ok(ExtractedTime {
            year: 2023,
            month: 6,
            day: 14,
            hour: 3,
            minute: 45,
            second: 0,
            timezone: "UTC".to_string(),
        }));

        // 3:45 PM CST
        assert_eq!("2023.06.14.15-45-CST".to_string().extract_absolute(Default::default()), Ok(ExtractedTime {
            year: 2023,
            month: 6,
            day: 14,
            hour: 15,
            minute: 45,
            second: 0,
            timezone: "CST".to_string(),
        }));
    }

    #[test]
    fn extract_seconds() {
        // 3:45:30 PM CST
        assert_eq!("2023.06.14.15-45-30-CST".to_string().extract_absolute(Default::default()), Ok(ExtractedTime {
            year: 2023,
            month: 6,
            day: 14,
            hour: 15,
            minute: 45,
            second: 30,
            timezone: "CST".to_string(),
        }));
    }

    #[test]
    fn handle_milliseconds() {
        // Handle comma
        assert_eq!("2023.06.14.15-45-30,123".to_string().extract_absolute(Default::default()), Ok(ExtractedTime {
            year: 2023,
            month: 6,
            day: 14,
            hour: 12 + 3,
            minute: 45,
            second: 30,
            timezone: "UTC".to_string(),
        }));


        // Handle period
        assert_eq!("2023.06.14.15-45-30.456".to_string().extract_absolute(Default::default()), Ok(ExtractedTime {
            year: 2023,
            month: 6,
            day: 14,
            hour: 12 + 3,
            minute: 45,
            second: 30,
            timezone: "UTC".to_string(),
        }));

        // Handle comma and timezone
        assert_eq!("2023.06.14.15-45-30,123-CST".to_string().extract_absolute(Default::default()), Ok(ExtractedTime {
            year: 2023,
            month: 6,
            day: 14,
            hour: 15,
            minute: 45,
            second: 30,
            timezone: "CST".to_string(),
        }));
    }
}