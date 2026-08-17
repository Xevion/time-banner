# Build Stage
FROM rust:alpine AS builder

# Install build dependencies
RUN apk add --no-cache \
    musl-dev \
    pkgconfig \
    openssl-dev

WORKDIR /usr/src/time-banner

# Copy manifests and the core crate's build script (needs its data file) for
# a dependency-only layer, cached separately from the application code.
COPY ./Cargo.toml ./Cargo.lock* ./
COPY ./crates/core/Cargo.toml ./crates/core/build.rs ./crates/core/
COPY ./crates/core/src/abbr_tz ./crates/core/src/abbr_tz
COPY ./crates/render/Cargo.toml ./crates/render/build.rs ./crates/render/
COPY ./crates/server/Cargo.toml ./crates/server/
COPY ./xtask ./xtask
RUN mkdir -p crates/core/src crates/render/src crates/render/benches crates/server/src \
    && echo "fn main() {}" > crates/core/src/lib.rs \
    && echo "" > crates/render/src/lib.rs \
    && echo "fn main() {}" > crates/render/benches/render.rs \
    && echo "fn main() {}" > crates/server/src/main.rs

# Fetch the bundled fonts before anything compiles the render crate: its
# build.rs requires them present, even for the stub-source layer below.
RUN cargo run --release -p xtask -- fonts

# Build with stub sources to produce a stable, dependency-only image layer
RUN cargo build --release --workspace

# Build with the real application code
RUN rm crates/core/src/lib.rs crates/render/src/lib.rs crates/server/src/main.rs
COPY ./crates ./crates
RUN rm target/release/deps/time_banner* target/release/deps/libtime_banner*
RUN cargo build --release --workspace

# Strip the binary to reduce size
RUN strip target/release/time-banner

# Runtime Stage - Alpine for smaller size and musl compatibility
FROM alpine:3.19
ARG APP=/usr/src/app
ARG APP_USER=appuser
ARG UID=1000
ARG GID=1000

# Install runtime dependencies
RUN apk add --no-cache \
    ca-certificates \
    tzdata

ENV TZ=Etc/UTC

# Create user with specific UID/GID
RUN addgroup -g $GID -S $APP_USER \
    && adduser -u $UID -D -S -G $APP_USER $APP_USER \
    && mkdir -p ${APP}

# Copy the binary. Templates and fonts are compiled into it, so nothing else
# needs to ship alongside it.
COPY --from=builder --chown=$APP_USER:$APP_USER /usr/src/time-banner/target/release/time-banner ${APP}/time-banner

# Set proper permissions
RUN chmod +x ${APP}/time-banner

USER $APP_USER
WORKDIR ${APP}

# Use ARG for build-time configuration, ENV for runtime
ARG PORT=3000
ENV PORT=${PORT}
EXPOSE ${PORT}

# Add health check (using wget since curl isn't in Alpine by default)
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD wget --quiet --tries=1 --spider http://localhost:${PORT}/health || exit 1

CMD ["./time-banner"]