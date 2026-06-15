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
RUN corepack enable
COPY dashboard/package.json dashboard/pnpm-lock.yaml ./
RUN pnpm install --frozen-lockfile
COPY dashboard/ ./
RUN pnpm run build

# ---- stage 2: rust release build ------------------------------------------
# Pin the Debian release: the runtime base (stage 3) must be the SAME release,
# or the binary links a newer glibc than the runtime provides and won't start.
FROM rust:slim-trixie AS rust-builder
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        pkg-config libssl-dev protobuf-compiler ca-certificates \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /build
COPY . .
COPY --from=web-builder /web/dist dashboard/dist
RUN cargo build -p console --release

# ---- stage 3: runtime -----------------------------------------------------
# Must match the builder's Debian release (stage 2) so glibc versions align.
FROM debian:trixie-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*
COPY --from=rust-builder /build/target/release/larkstack-console /usr/local/bin/larkstack-console
ENV CONSOLE_DATA_DIR=/data
ENV CONSOLE_PORT=8080
# Admin UI + API + integration webhooks (/webhooks/<app>/) all on the one port.
EXPOSE 8080
HEALTHCHECK --interval=30s --timeout=5s --retries=3 \
    CMD ["sh", "-c", "curl -fsS http://127.0.0.1:${CONSOLE_PORT}/api/health || exit 1"]
CMD ["larkstack-console"]
