# syntax=docker/dockerfile:1.24

ARG RUST_VERSION=1.95

# Stage 1: cargo-chef base, shared by the planner and builder stages below so
# the apk/cargo-chef install layer is cached identically for both.
FROM rust:${RUST_VERSION}-alpine AS chef
WORKDIR /build

RUN apk add --no-cache musl-dev pkgconfig openssl-dev && \
    cargo install cargo-chef --locked

# Stage 2: recipe planner. `cargo chef prepare` shells out to `cargo metadata`,
# which needs every declared target's entrypoint to actually exist, so the
# real source is copied here too; only the resulting recipe.json is content-
# addressed into the builder stage below, so unrelated source edits don't
# invalidate the dependency-cook cache.
FROM chef AS planner

COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# Stage 3: builder
FROM chef AS builder

# Cache mount ids must carry a literal `s/<service-id>-` prefix for Railway to
# persist them across builds; `${RAILWAY_SERVICE_ID}` is rejected because that
# validation runs before build-arg expansion. This is the `time-banner` service.
COPY --from=planner /build/recipe.json recipe.json
RUN --mount=type=cache,id=s/39522ec9-0888-4986-96cc-91cfa828d5a1-cargo-registry,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,id=s/39522ec9-0888-4986-96cc-91cfa828d5a1-cargo-git,target=/usr/local/cargo/git,sharing=locked \
    --mount=type=cache,id=s/39522ec9-0888-4986-96cc-91cfa828d5a1-cargo-target,target=/build/target,sharing=locked \
    cargo chef cook --release --recipe-path recipe.json --workspace

# `cook` builds dummy stand-ins for every workspace crate (including a no-op
# build.rs), so the real source replaces them here without disturbing the
# cached dependency layer above.
COPY . .

# xtask itself only became real source in the COPY above, so it must build
# here; render's build.rs then needs the fonts it fetches to be on disk.
ARG DBIP_MONTH=2026-08
RUN --mount=type=cache,id=s/39522ec9-0888-4986-96cc-91cfa828d5a1-cargo-registry,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,id=s/39522ec9-0888-4986-96cc-91cfa828d5a1-cargo-git,target=/usr/local/cargo/git,sharing=locked \
    --mount=type=cache,id=s/39522ec9-0888-4986-96cc-91cfa828d5a1-cargo-target,target=/build/target,sharing=locked \
    cargo run --release -p xtask -- fonts && \
    cargo run --release -p xtask -- geoip --month ${DBIP_MONTH}

# target/ is a cache mount and won't land in the image layer, so the binary is
# copied out before the mount is released.
RUN --mount=type=cache,id=s/39522ec9-0888-4986-96cc-91cfa828d5a1-cargo-registry,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,id=s/39522ec9-0888-4986-96cc-91cfa828d5a1-cargo-git,target=/usr/local/cargo/git,sharing=locked \
    --mount=type=cache,id=s/39522ec9-0888-4986-96cc-91cfa828d5a1-cargo-target,target=/build/target,sharing=locked \
    cargo build --release --workspace && \
    cp target/release/time-banner /build/time-banner && \
    strip /build/time-banner

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

# Copy the binary. Templates and fonts are compiled into it; geoip.bin is
# memory-mapped at runtime instead, so it ships alongside as a plain file.
COPY --from=builder --chown=$APP_USER:$APP_USER /build/time-banner ${APP}/time-banner
COPY --from=builder --chown=$APP_USER:$APP_USER /build/crates/core/geoip/geoip.bin ${APP}/geoip.bin

# Set proper permissions
RUN chmod +x ${APP}/time-banner

USER $APP_USER
WORKDIR ${APP}

# Use ARG for build-time configuration, ENV for runtime
ARG PORT=3000
ENV PORT=${PORT}
ENV GEOIP_DB_PATH=${APP}/geoip.bin
EXPOSE ${PORT}

# Add health check (using wget since curl isn't in Alpine by default)
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD wget --quiet --tries=1 --spider http://localhost:${PORT}/health || exit 1

CMD ["./time-banner"]
