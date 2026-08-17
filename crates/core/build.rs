use anyhow::{Context, Result, bail};
use regex::Regex;
use std::env;
use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::Path;
use std::sync::LazyLock;

/// Regex to match timezone lines: "ABBR \t Description \t UTC+-HH:MM"
static TIMEZONE_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"([A-Z]+)\s\t.+\s\tUTC([−+±]\d{2}(?::\d{2})?)").unwrap());

/// Regex to parse UTC offset format: "+-HH:MM" or "+-HH"
static OFFSET_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"([−+±])(\d{2})(?::(\d{2}))?").unwrap());

const SECONDS_PER_HOUR: i32 = 3600;
const SECONDS_PER_MINUTE: i32 = 60;

/// Parse a UTC offset string (e.g., "+05:30", "-08", "+-00") into seconds from UTC
fn parse_utc_offset(raw_offset: &str) -> Result<i32> {
    let captures = OFFSET_PATTERN
        .captures(raw_offset)
        .with_context(|| format!("Failed to match offset pattern: {}", raw_offset))?;

    // Handle +- (variable offset) as UTC
    let sign = captures.get(1).unwrap().as_str();
    if sign == "±" {
        return Ok(0);
    }

    let hours_str = captures.get(2).unwrap().as_str();
    let minutes_str = captures.get(3).map(|m| m.as_str()).unwrap_or("0");

    let hours: i32 = hours_str
        .parse()
        .with_context(|| format!("Invalid hours '{}'", hours_str))?;

    let minutes: i32 = minutes_str
        .parse()
        .with_context(|| format!("Invalid minutes '{}'", minutes_str))?;

    if hours > 23 {
        bail!("Hours out of range: {}", hours);
    }
    if minutes > 59 {
        bail!("Minutes out of range: {}", minutes);
    }

    let total_seconds = (hours * SECONDS_PER_HOUR) + (minutes * SECONDS_PER_MINUTE);

    Ok(match sign {
        "−" => -total_seconds,
        "+" => total_seconds,
        _ => unreachable!("Regex should only match +, -, or +-"),
    })
}

/// Parse a single timezone line and extract abbreviation and offset
fn parse_timezone_line(line: &str) -> Result<Option<(String, i32)>> {
    // Skip comment lines
    if line.trim().starts_with('#') || line.trim().is_empty() {
        return Ok(None);
    }

    let captures = TIMEZONE_PATTERN
        .captures(line)
        .with_context(|| format!("Failed to match timezone pattern: {}", line))?;

    let abbreviation = captures
        .get(1)
        .with_context(|| format!("Failed to extract abbreviation from line: {}", line))?
        .as_str()
        .to_string();

    let raw_offset = captures
        .get(2)
        .with_context(|| format!("Failed to extract offset from line: {}", line))?
        .as_str();

    let offset = parse_utc_offset(raw_offset)?;

    Ok(Some((abbreviation, offset)))
}

/// Generate the PHF map code for timezone abbreviations to UTC offsets
fn generate_timezone_map() -> Result<()> {
    let out_dir = env::var("OUT_DIR").context("OUT_DIR not set")?;
    let output_path = Path::new(&out_dir).join("timezone_map.rs");

    let tz_path = Path::new("./src/abbr_tz");
    let tz_file = File::open(tz_path).context("Failed to open timezone data file")?;
    let reader = BufReader::new(tz_file);

    let mut out_file =
        BufWriter::new(File::create(&output_path).context("Failed to create output file")?);
    let mut builder = phf_codegen::Map::<String>::new();

    let mut processed_count = 0;
    let mut skipped_count = 0;

    for line in reader.lines() {
        let line = line.context("Failed to read line")?;

        match parse_timezone_line(&line)? {
            Some((abbreviation, offset)) => {
                builder.entry(abbreviation.clone(), offset.to_string());
                processed_count += 1;
            }
            None => {
                skipped_count += 1;
            }
        }
    }

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
        panic!("Build script failed: {:#}", e);
    }

    // Tell Cargo to re-run this build script if the timezone file changes
    println!("cargo:rerun-if-changed=src/abbr_tz");
}
