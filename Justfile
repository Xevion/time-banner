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
lint:
    cargo clippy --all-targets --all-features -- -D warnings

# Run tests
test:
    cargo nextest run

# Build release binary
build:
    cargo build --release

# Build Docker image
docker-build:
    docker build -t time-banner:latest .

# Security audit
audit:
    cargo audit

# Check for unused dependencies
machete:
    cargo machete
