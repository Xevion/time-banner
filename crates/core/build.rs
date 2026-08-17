use anyhow::{Context, Result, bail};
use regex::Regex;
use std::env;
use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::Path;
use std::sync::LazyLock;

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

/// One parsed row: abbreviation, offset in seconds, and representative IANA zone.
struct Row {
    abbreviation: String,
    offset_seconds: i32,
    zone: String,
}

/// Parse a single tab-separated row: abbreviation, description, UTC offset, IANA zone.
///
/// Comments and blank lines yield `None`. A row that is present but malformed
/// is an error, so a mistyped column fails the build instead of silently
/// dropping an abbreviation.
fn parse_timezone_line(line: &str) -> Result<Option<Row>> {
    if line.trim().starts_with('#') || line.trim().is_empty() {
        return Ok(None);
    }

    let fields: Vec<&str> = line.split('\t').map(str::trim).collect();
    let [abbreviation, _description, raw_offset, zone] = fields.as_slice() else {
        bail!(
            "Expected 4 tab-separated fields, found {}: {}",
            fields.len(),
            line
        );
    };

    if !abbreviation.chars().all(|c| c.is_ascii_uppercase()) {
        bail!("Abbreviation must be ASCII uppercase: {}", abbreviation);
    }
    if !zone.contains('/') && *zone != "UTC" {
        bail!("Zone must be an IANA identifier or UTC: {}", zone);
    }

    Ok(Some(Row {
        abbreviation: abbreviation.to_string(),
        offset_seconds: parse_utc_offset(raw_offset)?,
        zone: zone.to_string(),
    }))
}

/// Generate the PHF map code for timezone abbreviations to their entries
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
            Some(row) => {
                builder.entry(
                    row.abbreviation,
                    format!(
                        r#"TzEntry {{ zone: "{}", offset_seconds: {} }}"#,
                        row.zone, row.offset_seconds
                    ),
                );
                processed_count += 1;
            }
            None => {
                skipped_count += 1;
            }
        }
    }

    writeln!(
        &mut out_file,
        "/// Auto-generated timezone abbreviation to IANA zone mapping"
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
        "pub static TIMEZONE_ABBREVIATIONS: phf::Map<&'static str, TzEntry> = {};",
        builder.build()
    )?;

    Ok(())
}

fn main() {
    if let Err(e) = generate_timezone_map() {
        panic!("Build script failed: {:#}", e);
    }

    // Tell Cargo to re-run this build script if the timezone file changes
    println!("cargo:rerun-if-changed=src/abbr_tz");
}
