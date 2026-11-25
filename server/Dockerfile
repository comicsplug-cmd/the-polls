# Build stage. The build context is the repository root because the API embeds
# data/seed/elections.json at compile time and applies server/migrations.
FROM rust:1-bookworm AS build

# Mirror the repository layout so `include_str!("../../data/...")` and
# `sqlx::migrate!("./migrations")` resolve exactly as they do locally.
WORKDIR /app/server

# Compile dependencies first so source edits do not invalidate the whole cache.
COPY server/Cargo.toml server/Cargo.lock ./
RUN mkdir src && printf 'fn main() {}\n' > src/main.rs \
    && cargo build --release --locked \
    && rm -rf src

COPY server/migrations ./migrations
COPY server/src ./src
COPY data /app/data

RUN cargo build --release --locked

# Runtime stage. SQLite is bundled into the binary; rustls removes the OpenSSL
# dependency, so only CA certificates and curl (for the health probe) are needed.
FROM debian:bookworm-slim AS runtime

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --home-dir /app --shell /usr/sbin/nologin polls \
    && mkdir -p /data \
    && chown polls:polls /data

COPY --from=build /app/server/target/release/the-polls-api /usr/local/bin/the-polls-api

ENV API_BIND=0.0.0.0:8080 \
    DATABASE_URL="sqlite:///data/polls.sqlite?mode=rwc"

USER polls
WORKDIR /app
VOLUME ["/data"]
EXPOSE 8080

HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD curl -fsS http://127.0.0.1:8080/api/health/ready || exit 1

CMD ["the-polls-api"]
