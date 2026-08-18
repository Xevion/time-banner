//! `cargo xtask geoip` converts a DB-IP City Lite `.mmdb` into the compact
//! `geoip.bin` table `time_banner_core::geoip` memory-maps at runtime.
//!
//! DB-IP City Lite carries no timezone field, only city-centroid
//! latitude/longitude, so each block's IANA zone is derived once here via
//! `utz`'s offline timezone-boundary lookup, then adjacent blocks resolving
//! to the same zone are merged. The on-disk layout this writes must match
//! `crates/core/src/geoip.rs` exactly.

use std::collections::BTreeMap;
use std::fs;
use std::io::Read as _;
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::time::Duration;

use assert2::assert;
use maxminddb::{Reader, WithinOptions};
use serde::Deserialize;

const MAGIC: &[u8; 8] = b"TBGEOIP\0";
const FORMAT_VERSION: u32 = 1;
const OUTPUT_RELATIVE_PATH: &str = "crates/core/geoip/geoip.bin";
const UNKNOWN_LABEL: u8 = 0;

const DBIP_USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
     (KHTML, like Gecko) Chrome/128.0.0.0 Safari/537.36";
const DBIP_REFERER: &str = "https://db-ip.com/db/download/ip-to-city-lite";

const FETCH_MAX_ATTEMPTS: u32 = 4;
const FETCH_BODY_LIMIT: u64 = 200 * 1024 * 1024;
// Snapshots publish early each month; fall back to earlier ones on 404.
const FETCH_FALLBACK_MONTHS: u32 = 2;

#[derive(Deserialize)]
struct Location {
    latitude: Option<f64>,
    longitude: Option<f64>,
}

#[derive(Deserialize)]
struct Record {
    location: Option<Location>,
}

/// One resolved source block: an address range and the IANA zone its
/// centroid falls in.
struct ResolvedRange {
    start: u128,
    end: u128,
    tz: String,
}

enum Source {
    LocalFile(PathBuf),
    Fetch(String),
}

fn resolve_source(args: &[String]) -> Source {
    if let Some(pos) = args.iter().position(|a| a == "--input") {
        return match args.get(pos + 1) {
            Some(path) => Source::LocalFile(PathBuf::from(path)),
            None => {
                eprintln!("--input requires a path argument");
                std::process::exit(1);
            }
        };
    }
    if let Some(pos) = args.iter().position(|a| a == "--month") {
        return match args.get(pos + 1) {
            Some(month) => Source::Fetch(month.clone()),
            None => {
                eprintln!("--month requires a YYYY-MM argument");
                std::process::exit(1);
            }
        };
    }
    if let Ok(path) = std::env::var("DBIP_MMDB_PATH") {
        return Source::LocalFile(PathBuf::from(path));
    }
    if let Ok(month) = std::env::var("DBIP_MONTH") {
        return Source::Fetch(month);
    }

    eprintln!(
        "no source given: pass --input <path>, --month <YYYY-MM>, or set DBIP_MMDB_PATH / DBIP_MONTH"
    );
    std::process::exit(1);
}

fn load_mmdb_bytes(source: Source) -> Vec<u8> {
    match source {
        Source::LocalFile(path) => {
            println!("reading {}", path.display());
            fs::read(&path).unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()))
        }
        Source::Fetch(month) => gunzip(&fetch_mmdb_gz(&month)),
    }
}

fn dbip_url(month: &str) -> String {
    format!("https://download.db-ip.com/free/dbip-city-lite-{month}.mmdb.gz")
}

fn previous_month(month: &str) -> String {
    let (year, mon) = month
        .split_once('-')
        .and_then(|(y, m)| Some((y.parse::<u32>().ok()?, m.parse::<u32>().ok()?)))
        .unwrap_or_else(|| panic!("malformed month {month:?}, expected YYYY-MM"));
    let (year, mon) = if mon <= 1 {
        (year - 1, 12)
    } else {
        (year, mon - 1)
    };
    format!("{year:04}-{mon:02}")
}

fn fetch_month(month: &str) -> Option<Vec<u8>> {
    let url = dbip_url(month);
    for attempt in 1..=FETCH_MAX_ATTEMPTS {
        let outcome = ureq::get(&url)
            .header("User-Agent", DBIP_USER_AGENT)
            .header("Referer", DBIP_REFERER)
            .header(
                "Accept",
                "application/gzip, application/octet-stream;q=0.9, */*;q=0.8",
            )
            .header("Accept-Language", "en-US,en;q=0.9")
            .call();

        match outcome {
            Ok(mut response) => {
                return Some(
                    response
                        .body_mut()
                        .with_config()
                        .limit(FETCH_BODY_LIMIT)
                        .read_to_vec()
                        .unwrap_or_else(|e| {
                            panic!("failed to read response body for {month}: {e}")
                        }),
                );
            }
            Err(ureq::Error::StatusCode(404)) => return None,
            Err(e) if attempt < FETCH_MAX_ATTEMPTS => {
                let backoff = Duration::from_secs(attempt as u64);
                println!(
                    "{month} attempt {attempt}/{FETCH_MAX_ATTEMPTS} failed ({e}), retrying in {backoff:?}"
                );
                std::thread::sleep(backoff);
            }
            Err(e) => panic!("failed to fetch {url} after {FETCH_MAX_ATTEMPTS} attempts: {e}"),
        }
    }
    unreachable!()
}

fn fetch_mmdb_gz(month: &str) -> Vec<u8> {
    let mut candidate = month.to_string();
    for _ in 0..=FETCH_FALLBACK_MONTHS {
        println!("fetching DB-IP City Lite for {candidate}");
        if let Some(bytes) = fetch_month(&candidate) {
            return bytes;
        }
        candidate = previous_month(&candidate);
    }
    panic!(
        "no DB-IP City Lite snapshot found for {month} or the {FETCH_FALLBACK_MONTHS} month(s) before it"
    );
}

fn gunzip(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    flate2::read::GzDecoder::new(bytes)
        .read_to_end(&mut out)
        .unwrap_or_else(|e| panic!("failed to decompress downloaded archive: {e}"));
    out
}

fn output_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask is a workspace member")
        .join(OUTPUT_RELATIVE_PATH)
}

/// Walks every network in `cidr` (`0.0.0.0/0` or `::/0`), resolving each
/// block's centroid to an IANA zone. `keep` filters out the address family
/// the caller isn't walking (the `::/0` pass otherwise also yields
/// IPv4-mapped aliases).
fn resolve_network<S: AsRef<[u8]>>(
    reader: &Reader<S>,
    finder: &utz::Finder,
    cidr: &str,
    keep: impl Fn(IpAddr) -> bool,
) -> Vec<ResolvedRange> {
    let net: ipnetwork::IpNetwork = cidr.parse().expect("valid CIDR literal");
    let iter = reader
        .within(net, WithinOptions::default())
        .unwrap_or_else(|e| panic!("within({cidr}) failed: {e}"));

    let mut out = Vec::new();
    for item in iter {
        let item = item.unwrap_or_else(|e| panic!("within({cidr}) iteration failed: {e}"));
        let network = item
            .network()
            .unwrap_or_else(|e| panic!("bad network in {cidr}: {e}"));
        if !keep(network.network()) {
            continue;
        }

        let record = item
            .decode::<Record>()
            .unwrap_or_else(|e| panic!("failed to decode record: {e}"));
        let Some(Record {
            location:
                Some(Location {
                    latitude: Some(lat),
                    longitude: Some(lon),
                }),
        }) = record
        else {
            continue;
        };

        let tz = finder
            .lookup(utz::Position { lat, lon })
            .unwrap_or_else(|e| panic!("timezone lookup failed for ({lat}, {lon}): {e}"));
        let Some(tz) = tz else { continue };

        let (start, end) = network_bounds(network);
        out.push(ResolvedRange {
            start,
            end,
            tz: tz.to_string(),
        });
    }
    out
}

fn network_bounds(net: ipnetwork::IpNetwork) -> (u128, u128) {
    match net.network() {
        IpAddr::V4(addr) => {
            let start = u32::from(addr) as u128;
            let prefix = net.prefix();
            let size = if prefix == 0 {
                u32::MAX as u128
            } else {
                (1u128 << (32 - prefix)) - 1
            };
            (start, start + size)
        }
        IpAddr::V6(addr) => {
            let start = u128::from(addr);
            let prefix = net.prefix();
            let size = if prefix == 0 {
                u128::MAX
            } else {
                (1u128 << (128 - prefix)) - 1
            };
            (start, start.saturating_add(size))
        }
    }
}

/// Assigns a stable `u8` label to each distinct timezone name seen so far,
/// interning into `labels`/`label_index` (index `0` is reserved for the
/// "unknown" sentinel, pre-seeded by the caller).
fn to_labeled_entries(
    ranges: Vec<ResolvedRange>,
    labels: &mut Vec<String>,
    label_index: &mut BTreeMap<String, u8>,
) -> Vec<(u128, u128, u8)> {
    ranges
        .into_iter()
        .map(|r| {
            let label = *label_index.entry(r.tz.clone()).or_insert_with(|| {
                assert!(
                    labels.len() < 256,
                    "more than 255 distinct timezones resolved; the u8 label can't address them all"
                );
                let id = labels.len() as u8;
                labels.push(r.tz.clone());
                id
            });
            (r.start, r.end, label)
        })
        .collect()
}

/// Merges adjacent same-label ranges and fills any gaps with the unknown
/// label, so the output covers `[0, max]` with no implicit holes. `entries`
/// need not be pre-sorted.
fn merge_ranges(entries: &[(u128, u128, u8)], max: u128) -> Vec<(u128, u8)> {
    let mut sorted = entries.to_vec();
    sorted.sort_by_key(|&(start, _, _)| start);

    let mut out: Vec<(u128, u8)> = Vec::new();
    let push = |out: &mut Vec<(u128, u8)>, start: u128, label: u8| {
        if out
            .last()
            .is_some_and(|&(_, last_label)| last_label == label)
        {
            return;
        }
        out.push((start, label));
    };

    let mut cursor: u128 = 0;
    for (start, end, label) in sorted {
        if start > cursor {
            push(&mut out, cursor, UNKNOWN_LABEL);
        }
        push(&mut out, start, label);
        match end.checked_add(1) {
            Some(next) => cursor = next,
            None => return out, // end == u128::MAX: fully covered, no trailing gap possible
        }
    }
    if cursor <= max {
        push(&mut out, cursor, UNKNOWN_LABEL);
    }
    out
}

fn encode(v4: &[(u128, u8)], v6: &[(u128, u8)], labels: &[String]) -> Vec<u8> {
    let label_table: Vec<u8> = labels
        .iter()
        .flat_map(|s| {
            assert!(s.len() <= u8::MAX as usize, "timezone name too long: {s}");
            let mut entry = vec![s.len() as u8];
            entry.extend_from_slice(s.as_bytes());
            entry
        })
        .collect();

    let mut buf = Vec::new();
    buf.extend_from_slice(MAGIC);
    buf.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
    buf.extend_from_slice(&(v4.len() as u32).to_le_bytes());
    buf.extend_from_slice(&(v6.len() as u32).to_le_bytes());
    buf.extend_from_slice(&(labels.len() as u16).to_le_bytes());
    buf.extend_from_slice(&0u16.to_le_bytes());
    buf.extend_from_slice(&(label_table.len() as u32).to_le_bytes());
    buf.extend_from_slice(&0u32.to_le_bytes());
    for &(start, label) in v4 {
        buf.extend_from_slice(&(start as u32).to_le_bytes());
        buf.push(label);
    }
    for &(start, label) in v6 {
        buf.extend_from_slice(&start.to_le_bytes());
        buf.push(label);
    }
    buf.extend_from_slice(&label_table);
    buf
}

pub fn convert(args: &[String]) {
    let bytes = load_mmdb_bytes(resolve_source(args));
    let reader = Reader::from_source(bytes).unwrap_or_else(|e| panic!("failed to parse mmdb: {e}"));

    let finder = utz::Finder::new().expect("failed to load bundled timezone boundary data");

    println!("resolving IPv4 blocks (this walks the full source tree, may take a minute)...");
    let v4_raw = resolve_network(&reader, &finder, "0.0.0.0/0", |addr| {
        matches!(addr, IpAddr::V4(_))
    });
    println!("resolving IPv6 blocks...");
    let v6_raw = resolve_network(&reader, &finder, "::/0", |addr| {
        matches!(addr, IpAddr::V6(_))
    });

    let mut labels: Vec<String> = vec![String::new()];
    let mut label_index: BTreeMap<String, u8> = BTreeMap::new();

    let v4_entries = to_labeled_entries(v4_raw, &mut labels, &mut label_index);
    let v6_entries = to_labeled_entries(v6_raw, &mut labels, &mut label_index);

    let v4_table = merge_ranges(&v4_entries, u32::MAX as u128);
    let v6_table = merge_ranges(&v6_entries, u128::MAX);

    let bytes = encode(&v4_table, &v6_table, &labels);

    let out_path = output_path();
    fs::create_dir_all(out_path.parent().expect("output path has a parent"))
        .expect("failed to create output directory");
    fs::write(&out_path, &bytes)
        .unwrap_or_else(|e| panic!("failed to write {}: {e}", out_path.display()));

    println!(
        "wrote {} ({} bytes, {} v4 ranges, {} v6 ranges, {} timezones)",
        out_path.display(),
        bytes.len(),
        v4_table.len(),
        v6_table.len(),
        labels.len() - 1,
    );
}

#[cfg(test)]
mod tests {
    use assert2::check;

    use super::*;

    #[test]
    fn merges_adjacent_same_label_ranges() {
        let out = merge_ranges(&[(0, 9, 1), (10, 19, 1)], 19);
        check!(out == vec![(0, 1)]);
    }

    #[test]
    fn keeps_distinct_labels_separate() {
        let out = merge_ranges(&[(0, 9, 1), (10, 19, 2)], 19);
        check!(out == vec![(0, 1), (10, 2)]);
    }

    #[test]
    fn fills_a_leading_gap_with_the_unknown_label() {
        let out = merge_ranges(&[(10, 19, 1)], 19);
        check!(out == vec![(0, UNKNOWN_LABEL), (10, 1)]);
    }

    #[test]
    fn fills_a_trailing_gap_with_the_unknown_label() {
        let out = merge_ranges(&[(0, 9, 1)], 19);
        check!(out == vec![(0, 1), (10, UNKNOWN_LABEL)]);
    }

    #[test]
    fn empty_input_is_entirely_unknown() {
        let out = merge_ranges(&[], 19);
        check!(out == vec![(0, UNKNOWN_LABEL)]);
    }

    #[test]
    fn input_need_not_be_pre_sorted() {
        let out = merge_ranges(&[(10, 19, 1), (0, 9, 1)], 19);
        check!(out == vec![(0, 1)]);
    }

    #[test]
    fn a_range_spanning_the_full_u128_domain_does_not_overflow() {
        let out = merge_ranges(&[(0, u128::MAX, 1)], u128::MAX);
        check!(out == vec![(0, 1)]);
    }

    #[test]
    fn a_gap_between_same_label_ranges_still_splits_them() {
        // not adjacent (there's an unknown block between them), so this is
        // two separate same-label ranges, not one merged range.
        let out = merge_ranges(&[(0, 9, 1), (20, 29, 1)], 29);
        check!(out == vec![(0, 1), (10, UNKNOWN_LABEL), (20, 1)]);
    }

    #[test]
    fn previous_month_steps_back_within_a_year() {
        check!(previous_month("2026-08") == "2026-07");
    }

    #[test]
    fn previous_month_rolls_back_across_a_year_boundary() {
        check!(previous_month("2026-01") == "2025-12");
    }

    #[test]
    fn dbip_url_embeds_the_month() {
        check!(
            dbip_url("2026-08") == "https://download.db-ip.com/free/dbip-city-lite-2026-08.mmdb.gz"
        );
    }
}
