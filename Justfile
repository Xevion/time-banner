set dotenv-load

alias c := check
alias t := test
alias f := format
alias fmt := format
alias l := lint

default:
    just --list

# Run all quality checks
check: format-check lint test machete

# Auto-format code
format:
    cargo fmt --all

# Check formatting without modifying
format-check:
    cargo fmt --all -- --check

# Lint with clippy (warnings are errors)
lint: fonts
    cargo clippy --all-targets --all-features -- -D warnings

# Run tests
test: fonts
    cargo nextest run

# Build the subsetted face bundle render embeds (fetches upstream faces first)
fonts:
    cargo run -p xtask -- fonts

# Rebuild the bundle and fail if the committed manifest no longer describes it
fonts-verify:
    cargo run -p xtask -- fonts --verify

# Convert a DB-IP City Lite .mmdb into crates/core/geoip/geoip.bin.
# Source: `--input <path>` / DBIP_MMDB_PATH, or `--month <YYYY-MM>` / DBIP_MONTH
# to fetch it. Not required to build or test (memory-mapped at runtime).
geoip *args:
    cargo run -p xtask -- geoip {{args}}

# Build release binary
build: fonts
    cargo build --release

# Build Docker image
docker-build:
    docker build -t time-banner:latest .

# Run benchmarks
bench: fonts
    cargo bench --workspace --benches

# Security audit
audit:
    cargo audit

# Check for unused dependencies
machete:
    cargo machete
