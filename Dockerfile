# ust1-oracle-service — production image for Coolify / Docker hosts.
# Build context: repository root.
#
#   docker build -t ust1-oracle-service .
#   docker run --env-file .env -p 8080:8080 ust1-oracle-service
#
# Env table + Coolify notes: docs/DEPLOYMENT.md § Oracle service.

FROM rust:1.88-bookworm AS build
WORKDIR /app
COPY . .
RUN cargo build --release -p ust1-oracle-service \
    && strip target/release/ust1-oracle-service

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --create-home --shell /usr/sbin/nologin oracle

COPY --from=build /app/target/release/ust1-oracle-service /usr/local/bin/ust1-oracle-service

USER oracle
ENV RUST_LOG=info \
    HEALTHZ_BIND=0.0.0.0:8080
EXPOSE 8080
# Liveness only (process up). Rate freshness is log-based (ORACLE_MAX_SILENCE_SECS).
HEALTHCHECK --interval=30s --timeout=5s --start-period=20s --retries=3 \
  CMD curl -fsS http://127.0.0.1:8080/healthz || exit 1

CMD ["ust1-oracle-service"]
