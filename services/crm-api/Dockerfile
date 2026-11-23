# ==============================================================================
# Stage 1: Recipe Planner (cargo-chef)
# ==============================================================================
FROM lukemathwalker/cargo-chef:latest-rust-1-bookworm AS planner
WORKDIR /app
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# ==============================================================================
# Stage 2: Dependencies Caching & Binary Build
# ==============================================================================
FROM lukemathwalker/cargo-chef:latest-rust-1-bookworm AS builder
WORKDIR /app

RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*

COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json -p crm-api

COPY . .
RUN cargo build --release -p crm-api

# ==============================================================================
# Stage 3: Minimal Debian Bookworm Runtime Image (Matching Glibc)
# ==============================================================================
FROM debian:bookworm-slim AS runtime
WORKDIR /app

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

RUN useradd -u 10001 -U -s /bin/sh appuser

ENV PORT=8081 \
    HOST=0.0.0.0 \
    RUST_LOG=info,crm_api=debug,tower_http=info

COPY --from=builder /app/target/release/crm-api /app/crm-api
COPY --from=builder /app/services/crm-api/migrations /app/migrations

USER appuser:appuser

EXPOSE 8081

ENTRYPOINT ["/app/crm-api"]
