use lazy_static::lazy_static;
use regex::Regex;
use std::env;
use std::fmt;
use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::Path;

/// Error types for build script failures
#[derive(Debug)]
enum BuildError {
    Io(std::io::Error),
    Regex(String),
    Parse(String),
    Env(env::VarError),
}

impl fmt::Display for BuildError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BuildError::Io(e) => write!(f, "IO error: {}", e),
            BuildError::Regex(msg) => write!(f, "Regex error: {}", msg),
            BuildError::Parse(msg) => write!(f, "Parse error: {}", msg),
            BuildError::Env(e) => write!(f, "Environment error: {}", e),
        }
    }
}

impl From<std::io::Error> for BuildError {
    fn from(error: std::io::Error) -> Self {
        BuildError::Io(error)
    }
}

impl From<env::VarError> for BuildError {
    fn from(error: env::VarError) -> Self {
        BuildError::Env(error)
    }
}

lazy_static! {
    /// Regex to match timezone lines: "ABBR \t Description \t UTC±HH:MM"
    static ref TIMEZONE_PATTERN: Regex =
        Regex::new(r"([A-Z]+)\s\t.+\s\tUTC([−+±]\d{2}(?::\d{2})?)").unwrap();

    /// Regex to parse UTC offset format: "±HH:MM" or "±HH"
    static ref OFFSET_PATTERN: Regex =
        Regex::new(r"([−+±])(\d{2})(?::(\d{2}))?").unwrap();
}

const SECONDS_PER_HOUR: i32 = 3600;
const SECONDS_PER_MINUTE: i32 = 60;

/// Parse a UTC offset string (e.g., "+05:30", "-08", "±00") into seconds from UTC
fn parse_utc_offset(raw_offset: &str) -> Result<i32, BuildError> {
    let captures = OFFSET_PATTERN.captures(raw_offset).ok_or_else(|| {
        BuildError::Regex(format!("Failed to match offset pattern: {}", raw_offset))
    })?;

    // Handle ± (variable offset) as UTC
    let sign = captures.get(1).unwrap().as_str();
    if sign == "±" {
        return Ok(0);
    }

    let hours_str = captures.get(2).unwrap().as_str();
    let minutes_str = captures.get(3).map(|m| m.as_str()).unwrap_or("0");

    let hours: i32 = hours_str
        .parse()
        .map_err(|e| BuildError::Parse(format!("Invalid hours '{}': {}", hours_str, e)))?;

    let minutes: i32 = minutes_str
        .parse()
        .map_err(|e| BuildError::Parse(format!("Invalid minutes '{}': {}", minutes_str, e)))?;

    // Validate ranges
    if hours > 23 {
        return Err(BuildError::Parse(format!("Hours out of range: {}", hours)));
    }
    if minutes > 59 {
        return Err(BuildError::Parse(format!(
            "Minutes out of range: {}",
            minutes
        )));
    }

    let total_seconds = (hours * SECONDS_PER_HOUR) + (minutes * SECONDS_PER_MINUTE);

    // Apply sign (− is west/negative, + is east/positive)
    Ok(match sign {
        "−" => -total_seconds,
        "+" => total_seconds,
        _ => unreachable!("Regex should only match +, −, or ±"),
    })
}

/// Parse a single timezone line and extract abbreviation and offset
fn parse_timezone_line(line: &str) -> Result<Option<(String, i32)>, BuildError> {
    // Skip comment lines
    if line.trim().starts_with('#') || line.trim().is_empty() {
        return Ok(None);
    }

    let captures = TIMEZONE_PATTERN
        .captures(line)
        .ok_or_else(|| BuildError::Regex(format!("Failed to match timezone pattern: {}", line)))?;

    let abbreviation = match captures.get(1) {
        Some(m) => m.as_str().to_string(),
        None => {
            return Err(BuildError::Regex(format!(
                "Failed to extract abbreviation from line: {}",
                line
            )))
        }
    };
    let raw_offset = match captures.get(2) {
        Some(m) => m.as_str(),
        None => {
            return Err(BuildError::Regex(format!(
                "Failed to extract offset from line: {}",
                line
            )))
        }
    };

    let offset = parse_utc_offset(raw_offset)?;

    Ok(Some((abbreviation, offset)))
}

/// Generate the PHF map code for timezone abbreviations to UTC offsets
fn generate_timezone_map() -> Result<(), BuildError> {
    let out_dir = env::var("OUT_DIR")?;
    let output_path = Path::new(&out_dir).join("timezone_map.rs");

    let tz_path = Path::new("./src/abbr_tz");
    let tz_file = File::open(tz_path)?;
    let reader = BufReader::new(tz_file);

    let mut out_file = BufWriter::new(File::create(&output_path)?);
    let mut builder = phf_codegen::Map::<String>::new();

    let mut processed_count = 0;
    let mut skipped_count = 0;

    for line in reader.lines() {
        let line = line?;

        match parse_timezone_line(&line)? {
            Some((abbreviation, offset)) => {
                builder.entry(abbreviation.clone(), offset.to_string());
                processed_count += 1;
                // println!(
                //     "cargo:warning=Processed timezone: {} -> {} seconds",
                //     abbreviation, offset
                // );
            }
            None => {
                skipped_count += 1;
            }
        }
    }

    // Generate the PHF map
    writeln!(
        &mut out_file,
        "/// Auto-generated timezone abbreviation to UTC offset (in seconds) mapping"
    )?;
    writeln!(
        &mut out_file,
        "/// Generated from {} timezone definitions ({} processed, {} skipped)",
        processed_count + skipped_count,
        processed_count,
        skipped_count
    )?;
    writeln!(
        &mut out_file,
        "pub static TIMEZONE_OFFSETS: phf::Map<&'static str, i32> = {};",
        builder.build()
    )?;

    println!(
        "cargo:warning=Generated timezone map with {} entries",
        processed_count
    );
    Ok(())
}

fn main() {
    if let Err(e) = generate_timezone_map() {
        panic!("Build script failed: {}", e);
    }

    // Tell Cargo to re-run this build script if the timezone file changes
    println!("cargo:rerun-if-changed=src/abbr_tz");
}
