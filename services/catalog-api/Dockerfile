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
RUN cargo chef cook --release --recipe-path recipe.json -p catalog-api

COPY . .
RUN cargo build --release -p catalog-api

# ==============================================================================
# Stage 3: Minimal Distroless Debian 12 Runtime Image (Matching Glibc 2.36)
# ==============================================================================
FROM gcr.io/distroless/cc-debian12:nonroot AS runtime
WORKDIR /app

ENV PORT=8083 \
    HOST=0.0.0.0 \
    RUST_LOG=info,catalog_api=debug,tower_http=info

COPY --from=builder /app/target/release/catalog-api /app/catalog-api
COPY --from=builder /app/services/catalog-api/migrations /app/migrations

EXPOSE 8083

ENTRYPOINT ["/app/catalog-api"]
