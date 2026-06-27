# syntax=docker/dockerfile:1
#
# Multi-stage build: bun builds the dashboard, cargo builds the binary, and a
# distroless image runs it (CA certs + a nonroot user, no shell).
#
#   docker build -t proofessoor .
#
# Multi-arch (amd64 + arm64) via buildx; the non-native arch builds under
# emulation, so expect it to be slow. Needs a container-driver builder once
# (the default docker driver cannot do multi-platform):
#
#   docker buildx create --driver docker-container --use
#   docker buildx build --platform linux/amd64,linux/arm64 -t <repo>:<tag> --push .

# --- Stage 1: dashboard assets (architecture-independent) ---
# Pin to the build host's platform so the static assets are built once,
# natively, instead of re-running under emulation for every target arch.
FROM --platform=$BUILDPLATFORM oven/bun:1.3.10-alpine AS frontend
WORKDIR /app
# Install against the committed lockfile first so this layer caches on deps alone.
COPY frontend/package.json frontend/bun.lock ./
RUN bun install --frozen-lockfile
COPY frontend/ ./
RUN bun run build

# --- Stage 2: binary (built for the target architecture) ---
# The full rust image (buildpack-deps based) already ships pkg-config, libssl-dev,
# git, and CA certs, so no apt step is needed. The pinned toolchain
# (rust-toolchain.toml, 1.93.1) is installed by rustup on first cargo invocation.
FROM rust:1.93-bookworm AS build
ARG TARGETPLATFORM
WORKDIR /src
COPY . .
# Scope the build cache per target arch so amd64/arm64 artifacts never mix; the
# downloaded crate registry is arch-independent, so it stays shared.
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target,id=target-$TARGETPLATFORM \
    cargo build --release --locked \
    && cp target/release/proofessoor /usr/local/bin/proofessoor

# --- Stage 3: runtime ---
FROM gcr.io/distroless/cc-debian12:nonroot AS runtime
COPY --from=build /usr/local/bin/proofessoor /usr/local/bin/proofessoor
COPY --from=frontend /app/dist /srv/ui
# The HTTP server (metrics + dashboard) is opt-in via --http-addr; publish
# whatever port you bind, e.g. -p 9090:9090 with --http-addr 0.0.0.0:9090.
ENTRYPOINT ["/usr/local/bin/proofessoor"]
