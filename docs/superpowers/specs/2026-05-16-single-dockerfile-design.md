# Single Dockerfile for Production

**Date:** 2026-05-16
**Status:** Approved, ready for implementation

## Goal

Ship the entire application — Rust backend, Svelte 5 frontend, SQLite database
— as a single OCI image built from one multi-stage `Dockerfile`. The image
must be runnable on a generic Linux host with `docker run`, persisting SQLite
data in a mounted volume.

## Why this is feasible

The application is already structured for single-process deployment:

- The Axum backend serves the built frontend as static files via `ServeDir`
  (`backend/src/routes.rs:15-17`), with SPA fallback to `index.html`.
- SQLite is embedded — no separate database service.
- Migrations run automatically at startup (`backend/src/main.rs:13`).
- All TLS goes through `rustls`, so the runtime image needs no OpenSSL.
- All configuration is environment-driven (`AppConfig::from_env`), making it
  12-factor compatible.

## Non-goals

- Pushing to a container registry from CI (recipe will support `--push` but
  no GitHub Actions wiring).
- TLS termination inside the container (assume an upstream reverse proxy).
- Backup/restore tooling for the SQLite volume.
- Replacing or modifying the existing dev-mode `docker-compose.yml`.

## Architecture

Three build stages and one runtime stage in a single `Dockerfile`:

### Stage 1: `frontend-builder` (`node:22-bookworm-slim`)

- `corepack enable pnpm`
- Copy `frontend/package.json` + `frontend/pnpm-lock.yaml` + `frontend/pnpm-workspace.yaml` first (layer cache).
- `pnpm install --frozen-lockfile`
- Copy the rest of `frontend/`.
- `pnpm build` → outputs to `/frontend/dist`.

### Stage 2: `backend-deps` (`rust:1-bookworm`) — dependency cache layer

- Copy `backend/Cargo.toml` + `backend/Cargo.lock` only.
- Create stub `src/main.rs` (`fn main(){}`) and stub `src/lib.rs` (empty).
- `cargo build --release --locked` — populates the dependency layer.
- This layer only invalidates when `Cargo.{toml,lock}` changes.

### Stage 3: `backend-builder` (`rust:1-bookworm`, FROM `backend-deps`)

- Copy `backend/src/`, `backend/migrations/`, `backend/.sqlx/`.
- Remove the stub-built artifacts so cargo rebuilds the real crate:
  `rm -f target/release/deps/backend* target/release/backend`
- `SQLX_OFFLINE=true cargo build --release --locked --bin backend`
- Final binary at `/backend/target/release/backend`.

### Stage 4: `runtime` (`debian:bookworm-slim`)

- `apt-get install -y --no-install-recommends ca-certificates curl && rm -rf /var/lib/apt/lists/*`
  - `ca-certificates` for outbound HTTPS (OAuth providers).
  - `curl` for HEALTHCHECK.
- Create `app` user: `groupadd -g 1000 app && useradd -m -u 1000 -g 1000 -s /usr/sbin/nologin app`
- `WORKDIR /app`
- Copy `--from=backend-builder /backend/target/release/backend /app/backend`
- Copy `--from=frontend-builder /frontend/dist /app/static`
- `mkdir -p /data && chown -R app:app /data /app`
- Baked-in env defaults:
  - `DATABASE_URL=sqlite:///data/app.db?mode=rwc`
  - `BIND_ADDR=0.0.0.0:3000`
  - `STATIC_DIR=/app/static`
  - `RUST_LOG=info,backend=info`
- `VOLUME /data`
- `USER app`
- `EXPOSE 3000`
- `HEALTHCHECK --interval=30s --timeout=3s --start-period=10s --retries=3 CMD curl -fsS http://localhost:3000/api/health || exit 1`
- `CMD ["/app/backend"]`

## Required runtime environment

The image fails fast on boot if any of these are missing (existing
`AppConfig::from_env` behavior):

- `COOKIE_SECRET` — 32+ bytes
- `JWT_PRIVATE_KEY_PEM`, `JWT_PUBLIC_KEY_PEM`
- `PUBLIC_BASE_URL`
- Any OAuth `*_CLIENT_ID` / `*_CLIENT_SECRET` pairs in use
- `INITIAL_INVITE_CODE` — only on first boot, then unset

Operator passes these via `--env-file` or repeated `-e`.

## File changes

### New files

- `Dockerfile` — multi-stage as above.
- `.dockerignore` — excludes:
  - `.git/`
  - `backend/target/`
  - `frontend/node_modules/`
  - `frontend/dist/` (built fresh inside the image)
  - `data/`
  - `.env`, `.env.local`
  - `**/*.swp`, `.DS_Store`, `.vscode/`, `.idea/`
  - `docs/`
  - `README.md`, `CLAUDE.md` (not needed in the image)

### Modified files

- `backend/src/routes.rs` — add `GET /api/health` handler. Performs a
  `SELECT 1` round-trip against the SQLite pool and returns `200 OK` with
  body `{"ok": true}` on success, `503` on DB error. Mounted under the `/api`
  nest so it shares the existing tracing/CORS/compression layers.
- `justfile` — add three recipes:
  ```
  docker-build:
      docker build -t full-stack-template:latest .

  docker-build-multiarch TAG:
      docker buildx build --platform linux/amd64,linux/arm64 -t {{TAG}} --push .

  docker-run:
      docker run --rm -p 3000:3000 -v app-data:/data --env-file .env full-stack-template:latest
  ```
- `README.md` — short "Production deployment" section covering: build, run,
  required env vars, volume persistence.

## Layer caching strategy

Manual two-step (no `cargo-chef` dependency):

- Frontend: `package.json` + lockfile copied → install → then sources →
  build. Standard pattern.
- Backend: `Cargo.toml` + `Cargo.lock` copied → stub crate → `cargo build`
  caches deps → real sources copied → second `cargo build` only recompiles
  the crate itself.

Trade-off accepted: ~10 lines of stub-and-rebuild logic vs adding cargo-chef
as a build-time dep. Manual is simpler to read and maintain at this scale.

## Health endpoint

New file or inline in `backend/src/routes.rs`:

```rust
async fn health(State(state): State<AppState>) -> Result<Json<Value>, AppError> {
    sqlx::query_scalar::<_, i64>("SELECT 1").fetch_one(&state.db).await?;
    Ok(Json(json!({ "ok": true })))
}
```

Mounted at `/api/health` inside the existing `api` router so it inherits the
tracing layer and is reachable at the same origin as everything else.

## Verification plan

After implementation:

1. `just docker-build` succeeds locally.
2. Image size < 150 MB compressed (`docker images` shows the size).
3. `docker run` with a populated `.env` and a fresh named volume:
   - Container boots, logs show "backend listening" and migrations run.
   - `curl http://localhost:3000/api/health` → 200 with `{"ok": true}`.
   - `curl http://localhost:3000/` returns the SPA `index.html`.
   - Register an account through the SPA, restart the container, log back
     in (proves volume persistence across restarts).
4. `docker inspect <container> --format '{{.State.Health.Status}}'` flips to
   `healthy` within ~10s of boot.
5. `just docker-build-multiarch ghcr.io/example/test:scratch` runs `buildx`
   for both amd64 and arm64 (does not need to actually push; can use a
   throwaway tag and `--load` swapped in if `--push` would require auth).

## Open considerations (not blockers)

- The `pnpm-workspace.yaml` exists in `frontend/` but the project doesn't
  appear to use workspaces beyond the single `frontend` package. Confirm
  during implementation whether this needs special handling.
- If `apt-get install ca-certificates curl` ever bloats the image
  meaningfully, swap `curl` for `wget --spider` (already in `slim`) or write
  a tiny shell-only TCP probe. Not worth doing preemptively.
