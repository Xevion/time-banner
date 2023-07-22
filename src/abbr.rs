use chrono::{FixedOffset, TimeZone};
use chrono::format::Fixed;
use phf::{Map, phf_map};

const HOUR: i32 = 60 * 60;

// Generated by build.rs, phf_codegen
include!(concat!(env!("OUT_DIR"), "/codegen.rs"));

/*
    Parse an abbreviation of a timezone into a UTC offset.
    Note: This is not standardized at all and is simply built on a reference of Time Zone abbreviations
    from Wikipedia (as of 2023-7-20).
 */
pub fn parse_abbreviation(abbreviation: &str) -> Result<FixedOffset, String> {
    let offset_integer_string = TIMEZONES.get(abbreviation);
    if offset_integer_string.is_none() {
        return Err("Failed to find abbreviation".to_string());
    }

    let offset = FixedOffset::east_opt(offset_integer_string.unwrap().parse().expect("Failed to parse stored offset"));
    return offset.ok_or("Failed to parse offset".to_string());
}


#[cfg(test)]
mod tests {
    use chrono::FixedOffset;
    use crate::abbr::parse_abbreviation;

    #[test]
    fn parse_offset() {
        assert_eq!(parse_abbreviation("CST").unwrap(), FixedOffset::west_opt(6 * 3600).unwrap());
    }
}