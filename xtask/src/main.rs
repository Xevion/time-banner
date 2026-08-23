//! Asset pipeline tasks that don't belong in the compile graph: slow,
//! network-dependent, or otherwise unsuited to running on every `cargo check`.
//!
//! `cargo xtask fonts` rebuilds the subsetted face bundle `crates/render`
//! embeds, fetching the upstream faces first if they aren't already on disk;
//! `--verify` reports drift instead of writing. See `fonts.rs`.
//!
//! `cargo xtask geoip` converts a DB-IP City Lite `.mmdb` into the compact
//! `geoip.bin` table `crates/core` memory-maps at runtime; see `geoip.rs`.

mod fonts;
mod geoip;

fn main() {
    let mut args = std::env::args().skip(1);
    let task = args.next();
    let rest: Vec<String> = args.collect();

    match task.as_deref() {
        Some("fonts") => fonts::run(&rest),
        Some("geoip") => geoip::convert(&rest),
        Some(other) => {
            eprintln!("unknown xtask: {other}\nAvailable: fonts, geoip");
            std::process::exit(1);
        }
        None => {
            eprintln!("usage: cargo xtask <task>\nAvailable: fonts, geoip");
            std::process::exit(1);
        }
    }
}
