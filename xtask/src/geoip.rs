//! `cargo xtask geoip` converts a DB-IP City Lite `.mmdb` into the compact
//! `geoip.bin` table `time_banner_core::geoip` memory-maps at runtime.
//!
//! DB-IP City Lite carries no timezone field, only city-centroid
//! latitude/longitude, so each block's IANA zone is derived once here via
//! `utz`'s offline timezone-boundary lookup, then adjacent blocks resolving
//! to the same zone are merged. The on-disk layout this writes must match
//! `crates/core/src/geoip.rs` exactly.
//!
//! Every expensive step is done once per distinct input rather than once
//! per network, and in parallel: the search tree is split into prefix
//! shards walked concurrently, each mmdb data record is decoded once (keyed
//! by its data-section offset), each distinct city centroid is resolved to
//! a zone once, and each shard merges its own ranges. Shards are disjoint
//! and ascending, so their results concatenate without a global sort.
//! Downloads stream straight into the decoder and are cached under the
//! target directory.

use std::fmt::Write as _;
use std::fs;
use std::io::{Read as _, Write as _};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use assert2::assert;
use ipnetwork::{IpNetwork, Ipv4Network, Ipv6Network};
use maxminddb::{Reader, WithinOptions};
use rayon::prelude::*;
use rustc_hash::{FxHashMap, FxHashSet};
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

/// Prefix length the search tree is split on for the parallel walk. Twelve
/// bits gives 4096 shards per family: fine enough that no single subtree
/// serializes the tail of the run on a small CI worker, coarse enough that
/// the fixed cost of entering a shard stays negligible.
///
/// A shard may not start below an aliased IPv4 subtree, or the walk will
/// descend into the alias instead of skipping it and duplicate the whole
/// IPv4 table into the IPv6 output. The shallowest alias is 6to4's
/// `2002::/16`, so 16 bits is the hard floor.
const SHARD_PREFIX: u8 = 12;
#[expect(clippy::disallowed_macros, reason = "assert2's assert! is not const")]
const _: () = core::assert!(SHARD_PREFIX <= 16);

/// Prints one aligned `stage: detail` line of the run summary.
macro_rules! report {
    ($stage:expr, $($detail:tt)*) => {
        println!("  {:<11} {}", $stage, format_args!($($detail)*))
    };
}

#[derive(Deserialize)]
struct Location {
    latitude: Option<f64>,
    longitude: Option<f64>,
}

#[derive(Deserialize)]
struct Record {
    location: Option<Location>,
}

/// A city centroid keyed by exact bit pattern, so distinct records sharing
/// a centroid collapse to one timezone lookup.
type CoordKey = (u64, u64);

/// What one shard's walk produces: its coalesced blocks, the centroid of
/// every data record it touched, and its share of the walk counters.
type ShardWalk = (Vec<Block>, FxHashMap<usize, Option<CoordKey>>, WalkStats);

/// One contiguous address range whose blocks all resolve through the same
/// mmdb data record.
struct Block {
    start: u128,
    end: u128,
    offset: usize,
}

/// One window of the address space handed to a single worker.
struct Shard {
    /// The subtree to walk, or `None` for a window with nothing to read.
    net: Option<IpNetwork>,
    /// Inclusive bounds the window's merged ranges must cover.
    range: (u128, u128),
}

impl Shard {
    fn walked(net: IpNetwork) -> Self {
        Shard {
            net: Some(net),
            range: network_bounds(net),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Family {
    V4,
    V6,
}

impl Family {
    fn label(self) -> &'static str {
        match self {
            Family::V4 => "IPv4",
            Family::V6 => "IPv6",
        }
    }

    fn max(self) -> u128 {
        match self {
            Family::V4 => u32::MAX as u128,
            Family::V6 => u128::MAX,
        }
    }

    /// The IPv6 walk also reaches the IPv4 subtree the mmdb stores at
    /// `::/96`, which reports IPv4 networks; those belong to the v4 table.
    fn matches(self, addr: IpAddr) -> bool {
        matches!(
            (self, addr),
            (Family::V4, IpAddr::V4(_)) | (Family::V6, IpAddr::V6(_))
        )
    }

    /// The family's address space split into disjoint windows in ascending
    /// order, together covering it with no gaps.
    ///
    /// The IPv6 split keeps `::/96` out of the walk: the mmdb's entire IPv4
    /// table hangs there, so walking it during the IPv6 pass would traverse
    /// millions of nodes only to discard every one of them. Its window is
    /// still covered (as unknown), and nothing addressable is lost, since
    /// `::/96` is deprecated IPv4-compatible space.
    fn shards(self) -> Vec<Shard> {
        let count = 1u128 << SHARD_PREFIX;
        match self {
            Family::V4 => (0..count)
                .map(|i| {
                    let addr = Ipv4Addr::from((i as u32) << (32 - SHARD_PREFIX));
                    Shard::walked(IpNetwork::V4(
                        Ipv4Network::new(addr, SHARD_PREFIX).expect("valid IPv4 shard prefix"),
                    ))
                })
                .collect(),
            Family::V6 => {
                let mut shards = vec![Shard {
                    net: None,
                    range: (0, u32::MAX as u128),
                }];
                // `::/SHARD_PREFIX` minus `::/96` is one network per bit
                // between the two prefixes, ascending as the prefix shrinks.
                shards.extend(
                    (SHARD_PREFIX + 1..=96)
                        .rev()
                        .map(|prefix| Shard::walked(v6_network(1u128 << (128 - prefix), prefix))),
                );
                shards.extend(
                    (1..count).map(|i| {
                        Shard::walked(v6_network(i << (128 - SHARD_PREFIX), SHARD_PREFIX))
                    }),
                );
                shards
            }
        }
    }
}

fn v6_network(addr: u128, prefix: u8) -> IpNetwork {
    IpNetwork::V6(Ipv6Network::new(Ipv6Addr::from(addr), prefix).expect("valid IPv6 shard prefix"))
}

fn network_bounds(net: IpNetwork) -> (u128, u128) {
    let (start, bits) = match net.network() {
        IpAddr::V4(addr) => (u32::from(addr) as u128, 32),
        IpAddr::V6(addr) => (u128::from(addr), 128),
    };
    let width = if net.prefix() == 0 {
        u128::MAX >> (128 - bits)
    } else {
        (1u128 << (bits - net.prefix())) - 1
    };
    (start, start.saturating_add(width))
}

enum Source {
    LocalFile(PathBuf),
    Fetch(String),
}

fn resolve_source(args: &[String]) -> Source {
    let flag = |name: &str| {
        args.iter().position(|a| a == name).map(|pos| {
            args.get(pos + 1).cloned().unwrap_or_else(|| {
                eprintln!("{name} requires an argument");
                std::process::exit(1);
            })
        })
    };

    if let Some(path) = flag("--input") {
        return Source::LocalFile(PathBuf::from(path));
    }
    if let Some(month) = flag("--month") {
        return Source::Fetch(month);
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

fn load_mmdb(source: Source, use_cache: bool) -> Vec<u8> {
    match source {
        Source::LocalFile(path) => {
            let start = Instant::now();
            let bytes = fs::read(&path)
                .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));
            let gzipped = path.extension().is_some_and(|e| e == "gz");
            let mmdb = if gzipped { gunzip(&bytes) } else { bytes };
            report!(
                "source",
                "{} -> {} mmdb{} in {}",
                path.display(),
                size(mmdb.len()),
                if gzipped { " (gunzipped)" } else { "" },
                secs(start.elapsed()),
            );
            mmdb
        }
        Source::Fetch(month) => fetch_mmdb(&month, use_cache),
    }
}

fn dbip_file_name(month: &str) -> String {
    format!("dbip-city-lite-{month}.mmdb.gz")
}

fn dbip_url(month: &str) -> String {
    format!("https://download.db-ip.com/free/{}", dbip_file_name(month))
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

/// Mirrors every byte read from the response into the cache file, so the
/// archive lands on disk during the transfer rather than after it. A write
/// failure drops the sink and leaves the download itself unaffected.
struct Tee<R> {
    inner: R,
    sink: Option<std::io::BufWriter<fs::File>>,
    read: u64,
}

impl<R: std::io::Read> std::io::Read for Tee<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.read += n as u64;
        if let Some(sink) = &mut self.sink
            && sink.write_all(&buf[..n]).is_err()
        {
            self.sink = None;
        }
        Ok(n)
    }
}

/// Streams one month's snapshot, inflating as the bytes arrive so the
/// transfer and the decompression overlap instead of running back to back.
/// Returns the mmdb and the compressed size, or `Ok(None)` if that month is
/// not published yet.
fn stream_month(month: &str, cache: Option<&Path>) -> Result<Option<(Vec<u8>, u64)>, String> {
    let mut response = match ureq::get(dbip_url(month))
        .header("User-Agent", DBIP_USER_AGENT)
        .header("Referer", DBIP_REFERER)
        .header(
            "Accept",
            "application/gzip, application/octet-stream;q=0.9, */*;q=0.8",
        )
        .header("Accept-Language", "en-US,en;q=0.9")
        .call()
    {
        Ok(response) => response,
        Err(ureq::Error::StatusCode(404)) => return Ok(None),
        Err(e) => return Err(e.to_string()),
    };

    let compressed_len: usize = response
        .headers()
        .get("Content-Length")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);

    let mut tee = Tee {
        inner: response
            .body_mut()
            .with_config()
            .limit(FETCH_BODY_LIMIT)
            .reader(),
        sink: cache.and_then(|path| fs::File::create(path).ok().map(std::io::BufWriter::new)),
        read: 0,
    };

    // DB-IP compresses about 2.2x, so this reserves the eventual mmdb up
    // front instead of growing through a dozen reallocations.
    let mut mmdb = Vec::with_capacity(compressed_len.saturating_mul(9) / 4);
    flate2::read::GzDecoder::new(&mut tee)
        .read_to_end(&mut mmdb)
        .map_err(|e| e.to_string())?;
    if let Some(sink) = &mut tee.sink {
        sink.flush().map_err(|e| e.to_string())?;
    }
    Ok(Some((mmdb, tee.read)))
}

/// Downloads a snapshot (falling back to earlier months while the current
/// one is unpublished), reusing and populating a cached archive under the
/// target directory so repeat runs skip the transfer entirely.
fn fetch_mmdb(month: &str, use_cache: bool) -> Vec<u8> {
    let start = Instant::now();
    if use_cache && let Ok(gz) = fs::read(cache_dir().join(dbip_file_name(month))) {
        let mmdb = gunzip(&gz);
        report!(
            "source",
            "{} cached, {} -> {} mmdb in {}",
            dbip_file_name(month),
            size(gz.len()),
            size(mmdb.len()),
            secs(start.elapsed()),
        );
        return mmdb;
    }

    let mut candidate = month.to_string();
    for _ in 0..=FETCH_FALLBACK_MONTHS {
        // Stream into the cache through a sibling temp file so an
        // interrupted run can't leave a truncated archive for the next one.
        let cached = cache_dir().join(dbip_file_name(&candidate));
        let temp = cached.with_extension("gz.partial");
        let sink = (use_cache && fs::create_dir_all(cache_dir()).is_ok()).then_some(temp.as_path());

        for attempt in 1..=FETCH_MAX_ATTEMPTS {
            match stream_month(&candidate, sink) {
                Ok(Some((mmdb, compressed))) => {
                    if sink.is_some() && fs::rename(&temp, &cached).is_err() {
                        let _ = fs::remove_file(&temp);
                    }
                    let elapsed = start.elapsed();
                    let rate = compressed as f64 / elapsed.as_secs_f64().max(0.001);
                    report!(
                        "source",
                        "{} downloaded, {} -> {} mmdb in {} ({}/s)",
                        dbip_file_name(&candidate),
                        size(compressed as usize),
                        size(mmdb.len()),
                        secs(elapsed),
                        size(rate as usize),
                    );
                    return mmdb;
                }
                Ok(None) => break,
                Err(e) if attempt < FETCH_MAX_ATTEMPTS => {
                    let backoff = Duration::from_secs(attempt as u64);
                    report!(
                        "retry",
                        "{candidate} attempt {attempt}/{FETCH_MAX_ATTEMPTS} failed ({e}), waiting {backoff:?}"
                    );
                    std::thread::sleep(backoff);
                }
                Err(e) => panic!(
                    "failed to fetch {} after {FETCH_MAX_ATTEMPTS} attempts: {e}",
                    dbip_url(&candidate)
                ),
            }
        }

        let _ = fs::remove_file(&temp);
        report!(
            "source",
            "no snapshot for {candidate}, trying the month before"
        );
        candidate = previous_month(&candidate);
    }

    panic!(
        "no DB-IP City Lite snapshot found for {month} or the {FETCH_FALLBACK_MONTHS} month(s) before it"
    );
}

fn workspace_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask is a workspace member")
}

fn cache_dir() -> PathBuf {
    std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| workspace_root().join("target"))
        .join("xtask-cache")
}

fn gunzip(bytes: &[u8]) -> Vec<u8> {
    // The gzip footer records the uncompressed size mod 2^32; the payload
    // is well under 4 GiB, so this reserves the exact final capacity.
    let hint = bytes
        .len()
        .checked_sub(4)
        .map(|i| u32::from_le_bytes([bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]]) as usize)
        .unwrap_or(0);
    let mut out = Vec::with_capacity(hint);
    flate2::read::GzDecoder::new(bytes)
        .read_to_end(&mut out)
        .unwrap_or_else(|e| panic!("failed to decompress archive: {e}"));
    out
}

#[derive(Default)]
struct WalkStats {
    /// Networks yielded by the search tree walk.
    networks: u64,
    /// Networks dropped because their record carries no city centroid.
    unlocated: u64,
}

/// Walks one shard, coalescing runs of adjacent networks that share a data
/// record and decoding each record at most once.
fn walk_shard<S: AsRef<[u8]>>(reader: &Reader<S>, net: IpNetwork, family: Family) -> ShardWalk {
    let iter = reader
        .within(net, WithinOptions::default())
        .unwrap_or_else(|e| panic!("within({net}) failed: {e}"));

    let mut blocks: Vec<Block> = Vec::new();
    let mut coords: FxHashMap<usize, Option<CoordKey>> = FxHashMap::default();
    let mut stats = WalkStats::default();

    for item in iter {
        let item = item.unwrap_or_else(|e| panic!("within({net}) iteration failed: {e}"));
        let network = item
            .network()
            .unwrap_or_else(|e| panic!("bad network in {net}: {e}"));
        let Some(offset) = item.offset().filter(|_| family.matches(network.network())) else {
            continue;
        };
        stats.networks += 1;

        let coord = *coords.entry(offset).or_insert_with(|| {
            match item
                .decode::<Record>()
                .unwrap_or_else(|e| panic!("failed to decode record: {e}"))
            {
                Some(Record {
                    location:
                        Some(Location {
                            latitude: Some(lat),
                            longitude: Some(lon),
                        }),
                }) => Some((lat.to_bits(), lon.to_bits())),
                _ => None,
            }
        });
        if coord.is_none() {
            stats.unlocated += 1;
            continue;
        }

        let (start, end) = network_bounds(network);
        match blocks.last_mut() {
            Some(last) if last.offset == offset && last.end.checked_add(1) == Some(start) => {
                last.end = end;
            }
            _ => blocks.push(Block { start, end, offset }),
        }
    }

    coords.retain(|_, coord| coord.is_some());
    (blocks, coords, stats)
}

/// Walks every shard of one family in parallel, keeping each shard's blocks
/// separate so the labelling and merging that follow stay parallel too.
fn walk_family<S: AsRef<[u8]> + Sync>(
    reader: &Reader<S>,
    family: Family,
    shards: &[Shard],
) -> (Vec<Vec<Block>>, FxHashMap<usize, CoordKey>, WalkStats) {
    let walked: Vec<ShardWalk> = shards
        .par_iter()
        .map(|shard| match shard.net {
            Some(net) => walk_shard(reader, net, family),
            None => ShardWalk::default(),
        })
        .collect();

    let mut blocks = Vec::with_capacity(walked.len());
    let mut records: FxHashMap<usize, CoordKey> = FxHashMap::default();
    let mut stats = WalkStats::default();
    for (shard_blocks, shard_coords, shard_stats) in walked {
        blocks.push(shard_blocks);
        records.extend(
            shard_coords
                .into_iter()
                .filter_map(|(offset, coord)| Some((offset, coord?))),
        );
        stats.networks += shard_stats.networks;
        stats.unlocated += shard_stats.unlocated;
    }
    (blocks, records, stats)
}

/// Interning state shared across both families: zone names by label id, the
/// reverse index, and every centroid resolved so far. Centroids repeat
/// between IPv4 and IPv6, so the cache keeps each one to a single lookup.
struct Zones {
    names: Vec<String>,
    index: FxHashMap<String, u8>,
    resolved: FxHashMap<CoordKey, u8>,
}

impl Zones {
    fn new() -> Self {
        Zones {
            // Index 0 is the "unknown" sentinel and never names a zone.
            names: vec![String::new()],
            index: FxHashMap::default(),
            resolved: FxHashMap::default(),
        }
    }

    fn label(&self, coord: CoordKey) -> u8 {
        self.resolved.get(&coord).copied().unwrap_or(UNKNOWN_LABEL)
    }

    /// Resolves every centroid not already cached, in parallel, and returns
    /// how many were looked up and how many landed outside every zone.
    /// Coordinates with no zone cache as the unknown sentinel, so a repeat
    /// centroid never costs a second lookup.
    fn resolve(&mut self, coords: &FxHashSet<CoordKey>, finder: &utz::Finder) -> (usize, usize) {
        let mut pending: Vec<CoordKey> = coords
            .iter()
            .copied()
            .filter(|coord| !self.resolved.contains_key(coord))
            .collect();
        // Sorted purely so label ids fall out in a deterministic order.
        pending.sort_unstable();

        let looked_up: Vec<(CoordKey, Option<&str>)> = pending
            .par_iter()
            .map(|&(lat, lon)| {
                let position = utz::Position {
                    lat: f64::from_bits(lat),
                    lon: f64::from_bits(lon),
                };
                ((lat, lon), finder.lookup(position).ok().flatten())
            })
            .collect();

        let mut unmapped = 0;
        self.resolved.reserve(looked_up.len());
        for (coord, tz) in looked_up {
            let label = match tz {
                Some(tz) => self.intern(tz),
                None => {
                    unmapped += 1;
                    UNKNOWN_LABEL
                }
            };
            self.resolved.insert(coord, label);
        }
        (pending.len(), unmapped)
    }

    fn intern(&mut self, tz: &str) -> u8 {
        if let Some(&label) = self.index.get(tz) {
            return label;
        }
        assert!(
            self.names.len() < 256,
            "more than 255 distinct timezones resolved; the u8 label can't address them all"
        );
        let label = self.names.len() as u8;
        self.names.push(tz.to_string());
        self.index.insert(tz.to_string(), label);
        label
    }
}

/// Merges labelled ranges into starts-only entries covering exactly
/// `[lo, hi]`: adjacent same-label ranges collapse and any gap becomes an
/// explicit unknown range, so the result has no implicit holes. Ranges are
/// clamped to the window, which is what makes per-shard results
/// concatenable; `entries` need not be pre-sorted.
fn merge_ranges(mut entries: Vec<(u128, u128, u8)>, (lo, hi): (u128, u128)) -> Vec<(u128, u8)> {
    if !entries.is_sorted_by_key(|&(start, _, _)| start) {
        entries.sort_unstable_by_key(|&(start, _, _)| start);
    }

    let mut out: Vec<(u128, u8)> = Vec::new();
    let push = |out: &mut Vec<(u128, u8)>, start: u128, label: u8| {
        if out.last().is_none_or(|&(_, last)| last != label) {
            out.push((start, label));
        }
    };

    let mut cursor = lo;
    for (start, end, label) in entries {
        let end = end.min(hi);
        // A range wholly behind the cursor was already covered by an
        // enclosing one: a record shorter than the shard prefix is yielded
        // once per shard it spans.
        if end < cursor {
            continue;
        }
        let start = start.max(cursor);
        if start > cursor {
            push(&mut out, cursor, UNKNOWN_LABEL);
        }
        push(&mut out, start, label);
        match end.checked_add(1) {
            Some(next) => cursor = next,
            None => return out, // end == u128::MAX: covered, no trailing gap possible
        }
    }
    if cursor <= hi {
        push(&mut out, cursor, UNKNOWN_LABEL);
    }
    out
}

/// Joins per-shard tables into one, dropping the first entry of a shard
/// when it repeats the label the previous shard ended on.
fn concat_ranges(shards: Vec<Vec<(u128, u8)>>) -> Vec<(u128, u8)> {
    let mut out: Vec<(u128, u8)> = Vec::with_capacity(shards.iter().map(Vec::len).sum());
    for shard in shards {
        let mut entries = shard.into_iter();
        if let Some(first) = entries.next()
            && out.last().is_none_or(|&(_, label)| label != first.1)
        {
            out.push(first);
        }
        out.extend(entries);
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

    let mut buf = Vec::with_capacity(32 + v4.len() * 5 + v6.len() * 17 + label_table.len());
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

/// Address counts per label across a merged table, where each entry runs up
/// to the next entry's start.
fn label_coverage(table: &[(u128, u8)], max: u128) -> FxHashMap<u8, u128> {
    let mut out: FxHashMap<u8, u128> = FxHashMap::default();
    for (i, &(start, label)) in table.iter().enumerate() {
        let end = table.get(i + 1).map_or(max, |&(next, _)| next - 1);
        *out.entry(label).or_default() += end - start + 1;
    }
    out
}

/// The zones covering the most IPv4 addresses, as `Name 12.3%` fragments.
fn top_zones(coverage: &FxHashMap<u8, u128>, names: &[String], take: usize) -> String {
    let space = u32::MAX as f64 + 1.0;
    let mut ranked: Vec<(u8, u128)> = coverage
        .iter()
        .map(|(&label, &count)| (label, count))
        .filter(|&(label, _)| label != UNKNOWN_LABEL)
        .collect();
    ranked.sort_unstable_by_key(|&(_, count)| std::cmp::Reverse(count));

    ranked
        .iter()
        .take(take)
        .fold(String::new(), |mut out, &(label, count)| {
            let separator = if out.is_empty() { "" } else { ", " };
            let share = count as f64 * 100.0 / space;
            let _ = write!(out, "{separator}{} {share:.1}%", names[label as usize]);
            out
        })
}

fn commas(n: u128) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, c) in digits.char_indices() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

fn size(len: usize) -> String {
    const UNITS: [&str; 4] = ["B", "KiB", "MiB", "GiB"];
    let mut value = len as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{len} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

fn secs(elapsed: Duration) -> String {
    format!("{:.2}s", elapsed.as_secs_f64())
}

/// Loads the timezone boundaries in eager mode: preloading trades a one-off
/// decode for lookups that never touch the compressed geometry again, and
/// this run does hundreds of thousands of them.
fn load_finder() -> utz::Finder {
    let start = Instant::now();
    let mut finder = utz::Finder::new().expect("failed to load bundled timezone boundary data");
    finder.preload();
    report!(
        "boundaries",
        "utz {} preloaded in {}",
        finder.tzbb_release(),
        secs(start.elapsed())
    );
    finder
}

/// Walks, labels, and merges one family end to end. Families run one at a
/// time so only one family's blocks, the bulk of the memory, are resident.
fn build_table<S: AsRef<[u8]> + Sync>(
    reader: &Reader<S>,
    family: Family,
    zones: &mut Zones,
    finder: &utz::Finder,
) -> Vec<(u128, u8)> {
    let shards = family.shards();

    let start = Instant::now();
    let (blocks, records, stats) = walk_family(reader, family, &shards);
    let block_count: usize = blocks.iter().map(Vec::len).sum();
    report!(
        family.label(),
        "{} networks, {} records, {} unlocated -> {} blocks ({})",
        commas(stats.networks as u128),
        commas(records.len() as u128),
        commas(stats.unlocated as u128),
        commas(block_count as u128),
        secs(start.elapsed()),
    );

    let start = Instant::now();
    let centroids: FxHashSet<CoordKey> = records.values().copied().collect();
    let (looked_up, unmapped) = zones.resolve(&centroids, finder);
    report!(
        "",
        "{} centroids, {} new lookups, {} outside every zone ({})",
        commas(centroids.len() as u128),
        commas(looked_up as u128),
        commas(unmapped as u128),
        secs(start.elapsed()),
    );

    let start = Instant::now();
    let table = concat_ranges(
        blocks
            .into_par_iter()
            .zip(shards.par_iter())
            .map(|(shard_blocks, shard)| {
                let entries = shard_blocks
                    .into_iter()
                    .map(|block| {
                        let label = records
                            .get(&block.offset)
                            .map_or(UNKNOWN_LABEL, |&coord| zones.label(coord));
                        (block.start, block.end, label)
                    })
                    .collect();
                merge_ranges(entries, shard.range)
            })
            .collect(),
    );
    report!(
        "",
        "merged to {} ranges, {:.1}x fewer ({})",
        commas(table.len() as u128),
        block_count as f64 / table.len().max(1) as f64,
        secs(start.elapsed()),
    );
    table
}

pub fn convert(args: &[String]) {
    let started = Instant::now();
    let use_cache = !args.iter().any(|a| a == "--no-cache");

    println!("geoip: DB-IP City Lite -> {OUTPUT_RELATIVE_PATH}");
    let mmdb = load_mmdb(resolve_source(args), use_cache);
    let reader = Reader::from_source(mmdb).unwrap_or_else(|e| panic!("failed to parse mmdb: {e}"));
    report!(
        "database",
        "{}, {} nodes, {} shards/family over {} threads",
        reader.metadata().database_type,
        commas(reader.metadata().node_count as u128),
        commas(1 << SHARD_PREFIX),
        rayon::current_num_threads(),
    );

    let finder = load_finder();
    let mut zones = Zones::new();
    let v4 = build_table(&reader, Family::V4, &mut zones, &finder);
    let v6 = build_table(&reader, Family::V6, &mut zones, &finder);
    let bytes = encode(&v4, &v6, &zones.names);

    let out_path = workspace_root().join(OUTPUT_RELATIVE_PATH);
    fs::create_dir_all(out_path.parent().expect("output path has a parent"))
        .expect("failed to create output directory");
    fs::write(&out_path, &bytes)
        .unwrap_or_else(|e| panic!("failed to write {}: {e}", out_path.display()));

    let coverage = label_coverage(&v4, Family::V4.max());
    let space = u32::MAX as u128 + 1;
    let mapped = space - coverage.get(&UNKNOWN_LABEL).copied().unwrap_or(0);

    println!(
        "wrote {} in {}",
        out_path.display(),
        secs(started.elapsed())
    );
    report!(
        "output",
        "{} ({} bytes), {} IPv4 + {} IPv6 ranges, {} zones",
        size(bytes.len()),
        commas(bytes.len() as u128),
        commas(v4.len() as u128),
        commas(v6.len() as u128),
        commas(zones.names.len() as u128 - 1),
    );
    report!(
        "coverage",
        "{:.1}% of the IPv4 space mapped ({} addresses)",
        mapped as f64 * 100.0 / space as f64,
        commas(mapped),
    );
    report!("top zones", "{}", top_zones(&coverage, &zones.names, 5));
}

#[cfg(test)]
mod tests {
    use assert2::check;

    use super::*;

    /// `(case, entries, window, expected)` for one merge scenario.
    type MergeCase = (
        &'static str,
        &'static [(u128, u128, u8)],
        (u128, u128),
        &'static [(u128, u8)],
    );

    /// Every branch of the merge: collapsing, gap filling, clamping, and
    /// overlap handling.
    #[rustfmt::skip]
    const MERGE_CASES: &[MergeCase] = &[
        ("adjacent same-label ranges merge",   &[(0, 9, 1), (10, 19, 1)],  (0, 19), &[(0, 1)]),
        ("distinct labels stay separate",      &[(0, 9, 1), (10, 19, 2)],  (0, 19), &[(0, 1), (10, 2)]),
        ("a leading gap becomes unknown",      &[(10, 19, 1)],             (0, 19), &[(0, 0), (10, 1)]),
        ("a trailing gap becomes unknown",     &[(0, 9, 1)],               (0, 19), &[(0, 1), (10, 0)]),
        ("no entries is entirely unknown",     &[],                        (0, 19), &[(0, 0)]),
        ("input need not be sorted",           &[(10, 19, 1), (0, 9, 1)],  (0, 19), &[(0, 1)]),
        ("a gap splits same-label ranges",     &[(0, 9, 1), (20, 29, 1)],  (0, 29), &[(0, 1), (10, 0), (20, 1)]),
        ("duplicate ranges collapse",          &[(0, 19, 1), (0, 19, 1)],  (0, 19), &[(0, 1)]),
        ("ranges are clamped to the window",   &[(0, 99, 1)],              (20, 29), &[(20, 1)]),
        ("an overlap covers only what's left", &[(0, 19, 1), (10, 29, 2)], (0, 29), &[(0, 1), (20, 2)]),
        ("the full domain does not overflow",  &[(0, u128::MAX, 1)],       (0, u128::MAX), &[(0, 1)]),
    ];

    #[test]
    fn merge_ranges_handles_every_shape() {
        for &(case, entries, window, expected) in MERGE_CASES {
            let out = merge_ranges(entries.to_vec(), window);
            check!(out == expected, "{case}");
        }
    }

    #[test]
    fn concat_drops_a_label_repeated_across_a_shard_boundary() {
        let joined = concat_ranges(vec![vec![(0, 1)], vec![(10, 1), (15, 2)], vec![(20, 2)]]);
        check!(joined == vec![(0, 1), (15, 2)]);
    }

    #[test]
    fn shards_tile_each_family_without_gaps() {
        for family in [Family::V4, Family::V6] {
            let shards = family.shards();
            check!(shards[0].range.0 == 0);
            check!(shards[shards.len() - 1].range.1 == family.max());
            check!(shards.windows(2).all(|w| w[0].range.1 + 1 == w[1].range.0));
        }
    }

    #[test]
    fn the_ipv4_mapped_block_is_covered_but_never_walked() {
        let shards = Family::V6.shards();
        let complement = 96 - SHARD_PREFIX as usize;
        check!(shards[0].net.is_none());
        check!(shards[0].range == (0, u32::MAX as u128));
        check!(shards[1..=complement].iter().all(|s| s.net.is_some()));
        check!(shards[complement].range.1 == (1u128 << (128 - SHARD_PREFIX)) - 1);
    }

    #[test]
    fn label_coverage_counts_every_address_once() {
        let coverage = label_coverage(&[(0, 1), (10, 2), (20, UNKNOWN_LABEL)], 29);
        check!(coverage[&1] == 10);
        check!(coverage[&2] == 10);
        check!(coverage[&UNKNOWN_LABEL] == 10);
    }

    #[test]
    fn months_step_backwards_and_across_the_year_boundary() {
        check!(previous_month("2026-08") == "2026-07");
        check!(previous_month("2026-01") == "2025-12");
    }

    #[test]
    fn dbip_url_embeds_the_month() {
        check!(
            dbip_url("2026-08") == "https://download.db-ip.com/free/dbip-city-lite-2026-08.mmdb.gz"
        );
    }

    #[test]
    fn numbers_and_sizes_are_human_readable() {
        check!(commas(0) == "0");
        check!(commas(999) == "999");
        check!(commas(13_205_768) == "13,205,768");
        check!(size(512) == "512 B");
        check!(size(13_205_768) == "12.6 MiB");
    }
}
