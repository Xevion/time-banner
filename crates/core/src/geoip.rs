//! IP address to IANA timezone lookup against a `geoip.bin` table generated
//! by `cargo xtask geoip` from a DB-IP City Lite database. See
//! `xtask/src/geoip.rs` for the conversion pipeline.
//!
//! On-disk format, little-endian, no compression (the file is intended to be
//! memory-mapped and queried directly, not decoded into memory first):
//!
//! ```text
//! Header (32 bytes):
//!   0   8   magic = b"TBGEOIP\0"
//!   8   4   format_version: u32 = 1
//!   12  4   v4_count: u32
//!   16  4   v6_count: u32
//!   20  2   label_count: u16
//!   22  2   reserved
//!   24  4   label_table_len: u32 (bytes)
//!   28  4   reserved
//!
//! v4_table    (v4_count * 5 bytes):  [start: u32 LE, label: u8], sorted by start
//! v6_table    (v6_count * 17 bytes): [start: u128 LE, label: u8], sorted by start
//! label_table (label_table_len bytes): repeated [len: u8, ascii bytes]
//! ```
//!
//! Label `0` is the empty-string "unknown" sentinel. Ranges are expected to
//! be gap-filled so every address has a predecessor; a file that violates
//! that (or is otherwise adversarial) must not cause a panic, only a miss.

use std::fs::File;
use std::net::IpAddr;
use std::path::Path;

const MAGIC: &[u8; 8] = b"TBGEOIP\0";
const FORMAT_VERSION: u32 = 1;
const HEADER_LEN: usize = 32;
const V4_ENTRY_LEN: usize = 5;
const V6_ENTRY_LEN: usize = 17;

/// A `geoip.bin` failed to load or parse.
#[derive(Debug, thiserror::Error)]
pub enum GeoipError {
    #[error("failed to read geoip database")]
    Io(#[from] std::io::Error),
    #[error("not a geoip database (bad magic)")]
    BadMagic,
    #[error("unsupported geoip database format version {0}")]
    UnsupportedVersion(u32),
    #[error("geoip database is truncated or corrupt")]
    Truncated,
}

/// A parsed `geoip.bin`, generic over its byte backing so tests can use a
/// plain `Vec<u8>` and production can use a memory-mapped file.
pub struct GeoipDb<S: AsRef<[u8]>> {
    data: S,
    v4_count: u32,
    v6_count: u32,
    v6_offset: usize,
    label_offset: usize,
    label_count: u16,
}

impl<S: AsRef<[u8]>> GeoipDb<S> {
    /// Parses a `geoip.bin` buffer, validating its header and section
    /// lengths against the actual byte count. Never panics on malformed
    /// input.
    pub fn from_bytes(data: S) -> Result<Self, GeoipError> {
        let bytes = data.as_ref();
        if bytes.len() < HEADER_LEN {
            return Err(GeoipError::Truncated);
        }
        if &bytes[0..8] != MAGIC {
            return Err(GeoipError::BadMagic);
        }

        let format_version = u32::from_le_bytes(bytes[8..12].try_into().unwrap());
        if format_version != FORMAT_VERSION {
            return Err(GeoipError::UnsupportedVersion(format_version));
        }

        let v4_count = u32::from_le_bytes(bytes[12..16].try_into().unwrap());
        let v6_count = u32::from_le_bytes(bytes[16..20].try_into().unwrap());
        let label_count = u16::from_le_bytes(bytes[20..22].try_into().unwrap());
        let label_table_len = u32::from_le_bytes(bytes[24..28].try_into().unwrap());

        let v4_bytes = (v4_count as usize).saturating_mul(V4_ENTRY_LEN);
        let v6_bytes = (v6_count as usize).saturating_mul(V6_ENTRY_LEN);
        let v6_offset = HEADER_LEN + v4_bytes;
        let label_offset = v6_offset + v6_bytes;
        let expected_len = label_offset + label_table_len as usize;
        if bytes.len() < expected_len {
            return Err(GeoipError::Truncated);
        }

        Ok(Self {
            data,
            v4_count,
            v6_count,
            v6_offset,
            label_offset,
            label_count,
        })
    }

    /// Resolves `ip` to an IANA timezone name, or `None` if the address
    /// falls in an unallocated/reserved range or outside the table entirely.
    pub fn lookup(&self, ip: IpAddr) -> Option<&str> {
        let bytes = self.data.as_ref();
        let label = match ip {
            IpAddr::V4(addr) => {
                let target = u32::from(addr) as u128;
                let idx = predecessor_index(self.v4_count as usize, target, |i| {
                    let off = HEADER_LEN + i * V4_ENTRY_LEN;
                    u32::from_le_bytes(bytes[off..off + 4].try_into().unwrap()) as u128
                })?;
                bytes[HEADER_LEN + idx * V4_ENTRY_LEN + 4]
            }
            IpAddr::V6(addr) => {
                let target = u128::from(addr);
                let idx = predecessor_index(self.v6_count as usize, target, |i| {
                    let off = self.v6_offset + i * V6_ENTRY_LEN;
                    u128::from_le_bytes(bytes[off..off + 16].try_into().unwrap())
                })?;
                bytes[self.v6_offset + idx * V6_ENTRY_LEN + 16]
            }
        };

        if label == 0 {
            return None;
        }
        self.label_str(label)
    }

    /// Walks the label table to find entry `label`. `label_count` is always
    /// small (the source data resolves to a few dozen distinct timezones),
    /// so a linear scan costs nothing next to the binary search and mmap
    /// page faults that dominate a lookup.
    fn label_str(&self, label: u8) -> Option<&str> {
        let bytes = self.data.as_ref();
        let mut offset = self.label_offset;
        for i in 0..self.label_count {
            let len = *bytes.get(offset)? as usize;
            let start = offset + 1;
            let end = start.checked_add(len)?;
            if i as u8 == label {
                return std::str::from_utf8(bytes.get(start..end)?).ok();
            }
            offset = end;
        }
        None
    }
}

impl GeoipDb<memmap2::Mmap> {
    /// Memory-maps `path` and parses it as a `geoip.bin`. The file is
    /// refreshable independently of the binary: replacing it on disk and
    /// reopening picks up a new DB-IP release without a rebuild.
    pub fn open(path: &Path) -> Result<Self, GeoipError> {
        let file = File::open(path)?;
        // SAFETY: the file may be modified by another process while mapped,
        // which risks reading torn data on a concurrent update. The deploy
        // model replaces `geoip.bin` via rename (atomic on the same
        // filesystem), so any given mapping observes only one file version.
        let mmap = unsafe { memmap2::Mmap::map(&file)? };
        Self::from_bytes(mmap)
    }
}

/// Binary search for the index of the largest element `<= target` among
/// `count` keys produced by `key_at`. `None` if `target` is smaller than
/// every key (no predecessor).
fn predecessor_index(count: usize, target: u128, key_at: impl Fn(usize) -> u128) -> Option<usize> {
    let mut lo = 0usize;
    let mut hi = count;
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        if key_at(mid) <= target {
            lo = mid + 1;
        } else {
            hi = mid;
        }
    }
    lo.checked_sub(1)
}

#[cfg(test)]
mod tests {
    use std::net::Ipv4Addr;

    use assert2::check;
    use proptest::prelude::*;

    use super::*;

    /// Serializes a `geoip.bin` buffer directly from entry lists, for
    /// building test fixtures without running the real conversion.
    fn build_test_db(v4: &[(u32, u8)], v6: &[(u128, u8)], labels: &[&str]) -> Vec<u8> {
        let label_table: Vec<u8> = labels
            .iter()
            .flat_map(|s| {
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
        for (start, label) in v4 {
            buf.extend_from_slice(&start.to_le_bytes());
            buf.push(*label);
        }
        for (start, label) in v6 {
            buf.extend_from_slice(&start.to_le_bytes());
            buf.push(*label);
        }
        buf.extend_from_slice(&label_table);
        buf
    }

    #[test]
    fn looks_up_a_v4_address_in_range() {
        let db = GeoipDb::from_bytes(build_test_db(
            &[(0, 0), (0x0A00_0000, 1), (0x0B00_0000, 0)],
            &[],
            &["", "America/Chicago"],
        ))
        .unwrap();
        check!(db.lookup("10.0.0.5".parse().unwrap()) == Some("America/Chicago"));
    }

    #[test]
    fn unknown_label_returns_none() {
        let db = GeoipDb::from_bytes(build_test_db(&[(0, 0)], &[], &[""])).unwrap();
        check!(db.lookup("1.2.3.4".parse().unwrap()).is_none());
    }

    #[test]
    fn looks_up_a_v6_address_in_range() {
        let db = GeoipDb::from_bytes(build_test_db(
            &[],
            &[(0, 0), (0x2001_0db8_0000_0000_0000_0000_0000_0000, 1)],
            &["", "Europe/London"],
        ))
        .unwrap();
        check!(db.lookup("2001:db8::1".parse().unwrap()) == Some("Europe/London"));
    }

    #[test]
    fn address_before_the_first_range_has_no_predecessor() {
        let db = GeoipDb::from_bytes(build_test_db(&[(100, 1)], &[], &["", "X"])).unwrap();
        check!(db.lookup("0.0.0.1".parse().unwrap()).is_none());
    }

    #[test]
    fn boundary_addresses_resolve() {
        let db = GeoipDb::from_bytes(build_test_db(
            &[(0, 1), (u32::MAX, 0)],
            &[],
            &["", "Etc/UTC"],
        ))
        .unwrap();
        check!(db.lookup("0.0.0.0".parse().unwrap()) == Some("Etc/UTC"));
        check!(db.lookup("255.255.255.255".parse().unwrap()).is_none());
        check!(db.lookup("255.255.255.254".parse().unwrap()) == Some("Etc/UTC"));
    }

    #[test]
    fn rejects_bad_magic() {
        let mut buf = build_test_db(&[], &[], &[""]);
        buf[0] = b'X';
        check!(let Err(GeoipError::BadMagic) = GeoipDb::from_bytes(buf));
    }

    #[test]
    fn rejects_unsupported_version() {
        let mut buf = build_test_db(&[], &[], &[""]);
        buf[8..12].copy_from_slice(&99u32.to_le_bytes());
        check!(let Err(GeoipError::UnsupportedVersion(99)) = GeoipDb::from_bytes(buf));
    }

    #[test]
    fn rejects_truncated_header() {
        check!(let Err(GeoipError::Truncated) = GeoipDb::from_bytes(vec![0u8; 10]));
    }

    #[test]
    fn rejects_truncated_body() {
        let mut buf = build_test_db(&[(0, 1)], &[], &["", "X"]);
        buf.truncate(buf.len() - 1);
        check!(let Err(GeoipError::Truncated) = GeoipDb::from_bytes(buf));
    }

    proptest! {
        /// The binary search must agree with a naive linear scan for the
        /// predecessor of an arbitrary query address, across arbitrary
        /// (deduped, zero-anchored) entry lists.
        #[test]
        fn matches_naive_linear_scan(
            entries in prop::collection::vec((any::<u32>(), 1u8..=5u8), 1..30),
            query in any::<u32>(),
        ) {
            let mut map: std::collections::BTreeMap<u32, u8> = entries.into_iter().collect();
            map.entry(0).or_insert(0);
            let v4: Vec<(u32, u8)> = map.into_iter().collect();

            let max_label = *v4.iter().map(|(_, l)| l).max().unwrap();
            let labels: Vec<String> = (0..=max_label)
                .map(|i| if i == 0 { String::new() } else { format!("Zone/{i}") })
                .collect();
            let label_refs: Vec<&str> = labels.iter().map(String::as_str).collect();

            let db = GeoipDb::from_bytes(build_test_db(&v4, &[], &label_refs)).unwrap();

            let expected_label = v4.iter().rev().find(|(start, _)| *start <= query).map(|(_, l)| *l).unwrap();
            let expected: Option<&str> = if expected_label == 0 { None } else { Some(label_refs[expected_label as usize]) };

            prop_assert_eq!(db.lookup(IpAddr::V4(Ipv4Addr::from(query))), expected);
        }

        /// The parser must reject, never panic on, arbitrary bytes.
        #[test]
        fn from_bytes_never_panics_on_arbitrary_input(bytes in prop::collection::vec(any::<u8>(), 0..200)) {
            let _ = GeoipDb::from_bytes(bytes);
        }
    }
}
