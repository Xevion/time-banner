# Variables
image_name := "time-banner"
container_name := "time-banner-dev"
port := "3000"

# Default recipe
default:
    @just --list

# Development server with hot reload
dev:
    @echo "🚀 Starting development server..."
    cargo watch -x "run --bin time-banner"

# Simple development server (no hot reload)
run:
    @echo "🚀 Starting server..."
    cargo run --bin time-banner

# Comprehensive check pipeline
check: format lint build test docker-build
    @echo "✅ All checks passed!"

# Format code
format:
    @echo "🎨 Formatting code..."
    cargo fmt --all

# Check formatting
format-check:
    @echo "🔍 Checking formatting..."
    cargo fmt --all -- --check

# Lint with clippy
lint:
    @echo "🔍 Running clippy..."
    cargo clippy --all-targets --all-features --

# Build project
build:
    @echo "🔨 Building project..."
    cargo build --release

# Run tests
test:
    @echo "🧪 Running tests..."
    cargo test

# Build Docker image
docker-build:
    @echo "🐳 Building Docker image..."
    docker build -t {{image_name}}:latest .

# Run Docker container
docker-run: docker-build
    @echo "🚀 Running Docker container..."
    docker run --rm -d --name {{container_name}} -p {{port}}:{{port}} {{image_name}}:latest
    @echo "Container started at http://localhost:{{port}}"

# Stop Docker container
docker-stop:
    @echo "🛑 Stopping Docker container..."
    docker stop {{container_name}} || true

# Docker logs
docker-logs:
    @echo "📋 Showing Docker logs..."
    docker logs {{container_name}}

# Follow Docker logs
docker-logs-follow:
    @echo "📋 Following Docker logs..."
    docker logs -f {{container_name}}

# Clean Docker artifacts
docker-clean: docker-stop
    @echo "🧹 Cleaning Docker artifacts..."
    docker rmi {{image_name}}:latest || true

# Full Docker development cycle
docker-dev: docker-clean docker-run
    @echo "🐳 Docker development environment ready!"

# Clean cargo artifacts
clean:
    @echo "🧹 Cleaning cargo artifacts..."
    cargo clean

# Install development dependencies
install-deps:
    @echo "📦 Installing development dependencies..."
    cargo install cargo-watch

# Security audit
audit:
    @echo "🔒 Running security audit..."
    cargo audit

# Check dependencies for updates
outdated:
    @echo "📅 Checking for outdated dependencies..."
    cargo outdated

# Release build with optimizations
release:
    @echo "🚀 Building release version..."
    cargo build --release

# Full CI pipeline (like what would run in CI)
ci: format-check lint build test docker-build
    @echo "🎯 CI pipeline completed!"

# Quick development check (faster than full check)
quick: format lint
    @echo "⚡ Quick check completed!" 