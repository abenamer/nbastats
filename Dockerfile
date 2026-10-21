# syntax=docker/dockerfile:1.7
# ---- build stage -------------------------------------------------------------
# Optional build secret `cacert`: a PEM bundle for environments where outbound TLS goes
# through an inspecting proxy, e.g.
#   docker build --secret id=cacert,src=/path/to/ca.pem -t nba-elo-simulator .
# Without the secret the build uses the system trust store.
FROM rust:1-bookworm AS builder
WORKDIR /build
# Cache dependencies: build a stub binary with only the manifests first.
COPY Cargo.toml Cargo.lock ./
RUN --mount=type=secret,id=cacert,required=false \
    if [ -f /run/secrets/cacert ]; then export CARGO_HTTP_CAINFO=/run/secrets/cacert SSL_CERT_FILE=/run/secrets/cacert; fi; \
    mkdir -p src && echo 'fn main() {}' > src/main.rs && echo '' > src/lib.rs \
    && cargo build --release --locked 2>/dev/null || true
COPY . .
# Touch sources so cargo rebuilds the crate (not the cached dependencies).
RUN --mount=type=secret,id=cacert,required=false \
    if [ -f /run/secrets/cacert ]; then export CARGO_HTTP_CAINFO=/run/secrets/cacert SSL_CERT_FILE=/run/secrets/cacert; fi; \
    touch src/main.rs src/lib.rs && cargo build --release --locked --bin nba-elo-simulator

# ---- runtime stage -----------------------------------------------------------
# No package installs at runtime: CA certificates come from the builder image and the
# health check uses the application binary itself.
FROM debian:bookworm-slim AS runtime
RUN useradd --system --uid 10001 --create-home --home-dir /home/app app \
    && mkdir -p /var/lib/nba-app && chown app:app /var/lib/nba-app
WORKDIR /app
COPY --from=builder /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/ca-certificates.crt
COPY --from=builder /build/target/release/nba-elo-simulator /usr/local/bin/nba-elo-simulator
COPY --chown=app:app static ./static
COPY --chown=app:app templates ./templates
COPY --chown=app:app migrations ./migrations
COPY --chown=app:app data ./data
USER app
ENV APP_BIND=0.0.0.0:3000 \
    APP_DATA_DIR=/var/lib/nba-app \
    BUNDLED_DATASET=/app/data/nba_elo_neil_paine.csv.gz \
    SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt \
    RUST_LOG=info,sqlx=warn
EXPOSE 3000
VOLUME ["/var/lib/nba-app"]
HEALTHCHECK --interval=15s --timeout=5s --start-period=60s --retries=5 \
    CMD ["/usr/local/bin/nba-elo-simulator", "healthcheck"]
ENTRYPOINT ["/usr/local/bin/nba-elo-simulator"]
CMD ["serve"]
