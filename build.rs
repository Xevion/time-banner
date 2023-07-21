use std::env;
use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::Path;
use regex::Regex;
use chrono::{FixedOffset};
use lazy_static::lazy_static;

lazy_static! {
    static ref FULL_PATTERN: Regex = Regex::new(r"([A-Z]+)\s\t.+\s\tUTC([−+±]\d{2}(?::\d{2})?)").unwrap();
    static ref OFFSET_PATTERN: Regex = Regex::new(r"([−+±])(\d{2}(?::\d{2})?)").unwrap();
}

const HOUR: u32 = 3600;

fn parse_offset(raw_offset: &str) -> i32 {
    let capture = OFFSET_PATTERN.captures(raw_offset).expect("RegEx failed to match offset");
    println!("{}: {}", raw_offset, capture.get(1).expect("First group capture failed").as_str());

    let is_west = capture.get(1).unwrap().as_str() == "−";
    let time = capture.get(2).expect("Second group capture failed").as_str();
    let (hours, minutes) = if time.contains(':') {
        let mut split = time.split(':');
        let hours = split.next().unwrap().parse::<u32>().unwrap();
        let minutes = split.next().unwrap().parse::<u32>().unwrap();

        (hours, minutes)
    } else {
        // Minutes not specified, assume 0
        (time.parse::<u32>().unwrap(), 0)
    };

    let value = (hours * HOUR) + (minutes * 60);
    return if is_west { value as i32 * -1 } else { value as i32 };
}

fn main() {
    let path = Path::new(&env::var("OUT_DIR").unwrap()).join("codegen.rs");
    let raw_tz = BufReader::new(File::open("./src/abbr_tz").unwrap());

    let mut file = BufWriter::new(File::create(&path).unwrap());

    let mut builder: phf_codegen::Map<String> = phf_codegen::Map::new();

    for line in raw_tz.lines() {
        let line = line.unwrap();
        if line.starts_with('#') {
            continue;
        }

        let capture = FULL_PATTERN.captures(&line).expect("RegEx failed to match line");

        let abbreviation = capture.get(1).unwrap().as_str();
        let raw_offset = capture.get(2).unwrap().as_str();

        let offset = if !raw_offset.starts_with('±') {
            parse_offset(raw_offset)
        } else {
            0
        };

        builder.entry(String::from(abbreviation), &format!("\"{}\"", offset).to_string());
    }

    write!(
        &mut file,
        "static TIMEZONES: phf::Map<&'static str, &'static str> = {}",
        builder.build()
    )
        .unwrap();
    write!(&mut file, ";\n").unwrap();
}