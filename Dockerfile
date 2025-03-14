# Build Stage
FROM rust:1.81.0 as builder

RUN USER=root cargo new --bin time-banner
WORKDIR ./time-banner
ENV CARGO_REGISTRIES_CRATES_IO_PROTOCOL=sparse
COPY ./Cargo.toml ./Cargo.toml
# Build empty app with downloaded dependencies to produce a stable image layer for next build
RUN cargo build --release

# Build web app with own code
RUN rm src/*.rs
ADD . ./
RUN rm ./target/release/deps/time_banner*
RUN cargo build --release


FROM debian:bullseye-slim
ARG APP=/usr/src/app

RUN apt-get update \
    && apt-get install -y ca-certificates tzdata \
    && rm -rf /var/lib/apt/lists/*

ENV TZ=Etc/UTC \
    APP_USER=appuser

RUN groupadd $APP_USER \
    && useradd -g $APP_USER $APP_USER \
    && mkdir -p ${APP}

COPY --from=builder /time-banner/target/release/time-banner ${APP}/time-banner
COPY --from=builder /time-banner/src/fonts ${APP}/fonts
COPY --from=builder /time-banner/src/templates ${APP}/templates

RUN chown -R $APP_USER:$APP_USER ${APP}

USER $APP_USER
WORKDIR ${APP}

EXPOSE 3000
ENV PORT 3000


CMD ["./time-banner"]