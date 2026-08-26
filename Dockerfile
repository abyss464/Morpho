# ---------------------------------------------------------------------------
# Morpho content engine — multi-stage Docker build
# Stages: ui-builder, rs-builder, py-builder, runtime
# ---------------------------------------------------------------------------

# -- 1. Admin UI (React/Vite via pnpm) -------------------------------------
FROM node:22-slim AS ui-builder

RUN corepack enable && corepack prepare pnpm@latest --activate

WORKDIR /build
COPY admin-ui/package.json admin-ui/pnpm-lock.yaml ./
RUN pnpm install --frozen-lockfile --ignore-scripts && \
    pnpm rebuild esbuild

COPY admin-ui/ ./
RUN pnpm build

# -- 2. morphod binary (Rust) ----------------------------------------------
FROM rust:1-slim-bookworm AS rs-builder

RUN apt-get update && \
    apt-get install -y --no-install-recommends pkg-config libssl-dev && \
    rm -rf /var/lib/apt/lists/*

WORKDIR /build/core
COPY core/ ./
COPY docs/contracts/ /build/docs/contracts/
RUN cargo build --release --bin morphod

# -- 3. Python adapters (uv-managed venvs) ---------------------------------
FROM python:3.12-slim-bookworm AS py-builder

COPY --from=ghcr.io/astral-sh/uv:latest /uv /usr/local/bin/uv

WORKDIR /build/adapters

# Copy common first (both adapters reference ../common as editable source)
COPY adapters/common/ ./common/

# TTS adapter
COPY adapters/tts/ ./tts/
RUN cd tts && uv sync --frozen

# Morfessor adapter
COPY adapters/morfessor/ ./morfessor/
RUN cd morfessor && uv sync --frozen

# -- 4. Runtime image -------------------------------------------------------
FROM python:3.12-slim-bookworm AS runtime

RUN apt-get update && \
    apt-get install -y --no-install-recommends ffmpeg libopus0 && \
    rm -rf /var/lib/apt/lists/*

WORKDIR /app

# morphod binary
COPY --from=rs-builder /build/core/target/release/morphod /app/morphod

# Admin UI dist
COPY --from=ui-builder /build/dist/ /app/admin-ui/dist/

# Python adapters + venvs
COPY --from=py-builder /build/adapters/common/ /app/adapters/common/
COPY --from=py-builder /build/adapters/tts/    /app/adapters/tts/
COPY --from=py-builder /build/adapters/morfessor/ /app/adapters/morfessor/

# uv binary (morphod spawns adapters via `uv run`)
COPY --from=ghcr.io/astral-sh/uv:latest /uv /usr/local/bin/uv

# Container config
COPY morphod.docker.toml /app/morphod.toml

EXPOSE 8787
VOLUME /app/data

CMD ["/app/morphod", "serve", "--config", "/app/morphod.toml"]
