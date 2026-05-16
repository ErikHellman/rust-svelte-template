# syntax=docker/dockerfile:1.7

# ---- Stage 1: build the SPA ----
FROM node:22-bookworm-slim AS frontend-builder
RUN corepack enable
WORKDIR /frontend
COPY frontend/package.json frontend/pnpm-lock.yaml frontend/pnpm-workspace.yaml ./
RUN pnpm install --frozen-lockfile
COPY frontend/ ./
RUN pnpm build

# ---- Stage 2: cache cargo dependencies ----
FROM rust:1-bookworm AS backend-deps
WORKDIR /backend
COPY backend/Cargo.toml backend/Cargo.lock ./
RUN mkdir -p src \
    && echo 'fn main() {}' > src/main.rs \
    && echo '' > src/lib.rs \
    && cargo build --release --locked \
    && rm -rf src

# ---- Stage 3: build the real backend binary ----
FROM backend-deps AS backend-builder
ENV SQLX_OFFLINE=true
COPY backend/src ./src
COPY backend/migrations ./migrations
COPY backend/.sqlx ./.sqlx
# Force rebuild of the crate itself; deps remain cached in target/release/deps.
RUN rm -f target/release/deps/backend* target/release/backend \
    && cargo build --release --locked --bin backend

# ---- Stage 4: runtime ----
FROM debian:bookworm-slim AS runtime
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd -g 1000 app \
    && useradd -m -u 1000 -g 1000 -s /usr/sbin/nologin app \
    && mkdir -p /data /app \
    && chown -R app:app /data /app

WORKDIR /app
COPY --from=backend-builder --chown=app:app /backend/target/release/backend /app/backend
COPY --from=frontend-builder --chown=app:app /frontend/dist /app/static

USER app
ENV DATABASE_URL=sqlite:///data/app.db?mode=rwc \
    BIND_ADDR=0.0.0.0:3000 \
    STATIC_DIR=/app/static \
    RUST_LOG=info,backend=info

VOLUME ["/data"]
EXPOSE 3000
HEALTHCHECK --interval=30s --timeout=3s --start-period=10s --retries=3 \
    CMD curl -fsS http://localhost:3000/api/health || exit 1

CMD ["/app/backend"]
