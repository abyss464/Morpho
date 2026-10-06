# ---------------------------------------------------------------------------
# Morpho content engine — multi-stage Docker build
# Stages: ui-builder, rs-builder, py-builder, runtime
# ---------------------------------------------------------------------------

# -- 1. Admin UI (React/Vite via pnpm) -------------------------------------
FROM node:22-slim AS ui-builder

RUN corepack enable && corepack prepare pnpm@11.21.0 --activate

WORKDIR /build
COPY admin-ui/package.json admin-ui/pnpm-lock.yaml admin-ui/pnpm-workspace.yaml ./
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

COPY --from=ghcr.io/astral-sh/uv:0.12.23 /uv /usr/local/bin/uv

WORKDIR /build/adapters

# Copy common first (both adapters reference ../common as editable source)
COPY adapters/common/ ./common/

# TTS adapter
COPY adapters/tts/ ./tts/
RUN cd tts && uv sync --frozen

# Morfessor adapter
COPY adapters/morfessor/ ./morfessor/
RUN cd morfessor && uv sync --frozen

# Codex adapter. Its *generator* is not in the image — a hosted CLI with
# somebody's credentials behind it does not belong in a container built from
# this repository — so `MORPHO_CODEX_BIN` resolves to nothing and morphod
# reports the source as disabled. Mount the binary in and the source comes
# alive; leave it out and the image chain simply stops at SDXL.
COPY adapters/codex/ ./codex/
RUN cd codex && uv sync --frozen

# CLIP adapter. Its lockfile pins CPU-only torch from PyTorch's dedicated
# index, so the image stays small and accelerator-free. GPU auto-detection
# kicks in when running natively under a venv that holds a CUDA/ROCm build.
COPY adapters/clip/ ./clip/
RUN cd clip && uv sync --frozen

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
COPY --from=py-builder /build/adapters/codex/    /app/adapters/codex/
COPY --from=py-builder /build/adapters/clip/     /app/adapters/clip/

# uv binary (morphod spawns adapters via `uv run`)
COPY --from=ghcr.io/astral-sh/uv:0.12.23 /uv /usr/local/bin/uv

# Container config
COPY morphod.docker.toml /app/morphod.toml

EXPOSE 8787
VOLUME /app/data

CMD ["/app/morphod", "serve", "--config", "/app/morphod.toml"]
