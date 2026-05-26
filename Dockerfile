# larkstack-console — single-binary supervisor for the three subsystems.
#
# Build context: workspace root.
#   docker build -t larkstack-console .
#
# Three stages: build the React UI, build the Rust binary (with the UI
# embedded via rust-embed), copy into a minimal Debian runtime.

# ---- stage 1: frontend ----------------------------------------------------
FROM node:lts-slim AS web-builder
WORKDIR /web
COPY crates/console/web/package.json crates/console/web/package-lock.json ./
RUN npm ci
COPY crates/console/web/ ./
RUN npm run build

# ---- stage 2: rust release build ------------------------------------------
FROM rust:slim AS rust-builder
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        pkg-config libssl-dev protobuf-compiler ca-certificates \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /build
COPY . .
COPY --from=web-builder /web/dist crates/console/web/dist
RUN cargo build -p console --release

# ---- stage 3: runtime -----------------------------------------------------
FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=rust-builder /build/target/release/larkstack-console /usr/local/bin/larkstack-console
ENV CONSOLE_DATA_DIR=/data
ENV CONSOLE_PORT=8080
# Persist SQLite event log + config.toml between restarts.
VOLUME ["/data"]
EXPOSE 8080
# linear-bridge HTTP listener (configurable via [linear-bridge.server.port]).
EXPOSE 3000
HEALTHCHECK --interval=30s --timeout=5s --retries=3 \
    CMD ["sh", "-c", "wget -q --spider http://127.0.0.1:${CONSOLE_PORT}/api/health || exit 1"]
CMD ["larkstack-console"]
