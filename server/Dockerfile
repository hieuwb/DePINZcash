# syntax=docker/dockerfile:1.7

FROM rust:1.91-bookworm AS builder
WORKDIR /build

# Cache dependency build by copying manifest first.
COPY Cargo.toml ./
# sqlx::migrate! reads at compile time, so migrations/ must be present during build.
COPY migrations ./migrations
RUN mkdir -p src && echo "fn main() {}" > src/main.rs && echo "" > src/lib.rs \
    && cargo build --release --bin depinzcash-server || true

COPY src ./src
RUN touch src/main.rs src/lib.rs && cargo build --release --bin depinzcash-server

FROM debian:bookworm-slim AS runtime
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates tini \
    && rm -rf /var/lib/apt/lists/*

RUN useradd --system --uid 10001 --home /data --shell /sbin/nologin depinz \
    && mkdir -p /data \
    && chown depinz:depinz /data

COPY --from=builder /build/target/release/depinzcash-server /usr/local/bin/depinzcash-server

ENV BIND_ADDR=0.0.0.0:8080 \
    DATABASE_URL=sqlite:///data/depinzcash.sqlite?mode=rwc \
    RUST_LOG=info,sqlx=warn,hyper=warn,tower_http=info \
    SCHEDULER_ENABLED=true

EXPOSE 8080
VOLUME ["/data"]
USER depinz
ENTRYPOINT ["/usr/bin/tini", "--", "/usr/local/bin/depinzcash-server"]
