# Build Stage
FROM rust:1.81.0-alpine as builder

# Install build dependencies
RUN apk add --no-cache \
    musl-dev \
    pkgconfig \
    openssl-dev

WORKDIR /usr/src
RUN USER=root cargo new --bin time-banner
WORKDIR /usr/src/time-banner

# Copy dependency files for better layer caching
COPY ./Cargo.toml ./Cargo.lock* ./build.rs ./
# Copy the timezone data file needed by build.rs
COPY ./src/abbr_tz ./src/abbr_tz

# Build empty app with downloaded dependencies to produce a stable image layer for next build
RUN cargo build --release

# Build web app with own code
RUN rm src/*.rs
COPY ./src ./src
RUN rm ./target/release/deps/time_banner*
RUN cargo build --release

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

# Copy application files
COPY --from=builder --chown=$APP_USER:$APP_USER /usr/src/time-banner/target/release/time-banner ${APP}/time-banner
COPY --from=builder --chown=$APP_USER:$APP_USER /usr/src/time-banner/src/fonts ${APP}/fonts
COPY --from=builder --chown=$APP_USER:$APP_USER /usr/src/time-banner/src/templates ${APP}/templates

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