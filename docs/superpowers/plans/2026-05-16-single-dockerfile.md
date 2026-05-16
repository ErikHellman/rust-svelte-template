# Single Dockerfile Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship the entire application (Rust backend + Svelte SPA + SQLite) as a single OCI image built from one multi-stage `Dockerfile`, runnable with `docker run` on a generic Linux host with persistent SQLite via a volume.

**Architecture:** Multi-stage build — `node:22-bookworm-slim` builds the SPA, `rust:1-bookworm` builds the backend binary (with a separate dep-cache stage for layer reuse), then the binary + built SPA are copied into a `debian:bookworm-slim` runtime image. The runtime image runs as non-root `app` (uid 1000), declares `VOLUME /data`, exposes 3000, and includes a `HEALTHCHECK` against a new `GET /api/health` endpoint.

**Tech Stack:** Docker (BuildKit), `docker buildx` for multi-arch, Axum, sqlx, `just`.

**Spec:** [`docs/superpowers/specs/2026-05-16-single-dockerfile-design.md`](../specs/2026-05-16-single-dockerfile-design.md)

---

## File Structure

| File | New/Modified | Responsibility |
| --- | --- | --- |
| `backend/src/routes.rs` | Modified | Add `health` handler + mount under `/api/health`. |
| `backend/tests/health.rs` | Created | Integration test asserting `GET /api/health` returns 200 + `{"ok": true}`. |
| `Dockerfile` | Created | Four-stage build: frontend, backend deps cache, backend real build, runtime. |
| `.dockerignore` | Created | Excludes `target/`, `node_modules/`, `data/`, `.env`, etc. — keeps build context small. |
| `justfile` | Modified | Add `docker-build`, `docker-build-multiarch TAG`, `docker-run` recipes. |
| `README.md` | Modified | Replace existing "Deploying" section with Dockerfile-based instructions. |

---

## Task 1: Add `GET /api/health` endpoint (TDD)

**Files:**
- Test: `backend/tests/health.rs` (create)
- Modify: `backend/src/routes.rs`

The health endpoint returns 200 + `{"ok": true}` on success; the handler does a `SELECT 1` round-trip to SQLite to confirm DB liveness. We use the runtime (non-macro) `sqlx::query_scalar` form so we don't have to update the offline `.sqlx/` cache.

- [ ] **Step 1: Write the failing test**

Create `backend/tests/health.rs`:

```rust
mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt;

#[tokio::test]
async fn health_endpoint_returns_ok() {
    let (state, _tmp) = common::test_state().await;
    let app = backend::routes::build(state);

    let res = app
        .oneshot(
            Request::builder()
                .uri("/api/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(json, serde_json::json!({ "ok": true }));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd backend && cargo test --test health`
Expected: test fails — `/api/health` is not yet routed, so the response status is `404 NOT FOUND`, not `200 OK`. The assertion `assert_eq!(res.status(), StatusCode::OK)` panics.

- [ ] **Step 3: Add the health handler and route in `backend/src/routes.rs`**

Modify `backend/src/routes.rs`. Add imports + handler + route. The full revised `build()` function (replace the existing one):

```rust
use crate::{AppError, AppState};
use axum::extract::State;
use axum::http::{HeaderValue, Method};
use axum::response::Json;
use axum::routing::get;
use axum::Router;
use serde_json::{json, Value};
use tower_http::compression::CompressionLayer;
use tower_http::cors::CorsLayer;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::TraceLayer;

pub fn build(state: AppState) -> Router {
    let api = Router::new()
        .route("/health", get(health))
        .nest("/auth", crate::auth::router())
        .nest("/notes", crate::notes::router())
        .with_state(state.clone());

    let static_dir = std::env::var("STATIC_DIR").unwrap_or_else(|_| "static".into());
    let static_service = ServeDir::new(&static_dir)
        .not_found_service(ServeFile::new(format!("{static_dir}/index.html")));

    let cors = build_cors(&state);

    Router::new()
        .nest("/api", api)
        .fallback_service(static_service)
        .layer(TraceLayer::new_for_http())
        .layer(CompressionLayer::new())
        .layer(cors)
}

async fn health(State(state): State<AppState>) -> Result<Json<Value>, AppError> {
    sqlx::query_scalar::<_, i64>("SELECT 1")
        .fetch_one(&state.db)
        .await?;
    Ok(Json(json!({ "ok": true })))
}
```

Leave the existing `build_cors` function below this unchanged.

- [ ] **Step 4: Run the test to verify it passes**

Run: `cd backend && cargo test --test health`
Expected: PASS (1 test, `health_endpoint_returns_ok`).

- [ ] **Step 5: Run the full check to confirm nothing regressed**

Run: `just check`
Expected: PASS — no clippy warnings, formatting clean, frontend lint/type-check clean.

- [ ] **Step 6: Commit**

```bash
git add backend/src/routes.rs backend/tests/health.rs
git commit -m "Add /api/health liveness endpoint"
```

---

## Task 2: Create `.dockerignore`

**Files:**
- Create: `.dockerignore`

Without a `.dockerignore`, the build context will include `backend/target/` (gigabytes once compiled) and `frontend/node_modules/` (hundreds of MB), making every build slow and bloated.

- [ ] **Step 1: Create `.dockerignore` at the repo root**

```gitignore
# Version control
.git
.github
.gitignore
.pre-commit-config.yaml

# Build artifacts (image rebuilds these from source)
backend/target
frontend/node_modules
frontend/dist
backend/static

# Local data
data
*.db
*.db-shm
*.db-wal

# Local env (image consumer provides at runtime)
.env
.env.local
.env.*.local

# Editors / OS
.vscode
.idea
*.swp
.DS_Store
.txt

# Docs / templates not needed at runtime
docs
README.md
CLAUDE.md
docker-compose.yml
```

Note: do **not** exclude `backend/.sqlx/` — the offline query cache is required for `SQLX_OFFLINE=true cargo build`.

- [ ] **Step 2: Commit**

```bash
git add .dockerignore
git commit -m "Add .dockerignore for production image build context"
```

---

## Task 3: Create the multi-stage `Dockerfile`

**Files:**
- Create: `Dockerfile`

Four stages: `frontend-builder`, `backend-deps`, `backend-builder`, `runtime`. The `backend-deps` stage exists purely to cache compiled dependencies — they only re-build when `Cargo.toml` or `Cargo.lock` change.

- [ ] **Step 1: Create `Dockerfile` at the repo root**

```dockerfile
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
```

- [ ] **Step 2: Build the image**

Run: `docker build -t full-stack-template:test .`
Expected: build succeeds for all four stages, ends with `naming to docker.io/library/full-stack-template:test`. First build will take several minutes (compiling all Rust deps).

- [ ] **Step 3: Verify image size is under the 150 MB target**

Run: `docker images full-stack-template:test --format '{{.Size}}'`
Expected: under ~150 MB (typical: 100–130 MB). If significantly larger, investigate (likely the `backend-builder` stage was not properly stripped from the runtime image — check for stray `COPY` statements).

- [ ] **Step 4: Commit**

```bash
git add Dockerfile
git commit -m "Add multi-stage Dockerfile for production image"
```

---

## Task 4: Add `justfile` recipes

**Files:**
- Modify: `justfile`

Add three recipes appended to the existing file. The split between `docker-build` (single-arch, loads into local daemon) and `docker-build-multiarch TAG` (cross-platform, pushes to a registry) is intentional: `docker buildx` cannot both produce a multi-arch image AND `--load` it into the local Docker daemon, so we provide separate recipes for the two use cases.

- [ ] **Step 1: Append to `justfile`**

Add these recipes at the end of `justfile`:

```just
# Build the production image for the local host architecture.
docker-build:
    docker build -t full-stack-template:latest .

# Build a multi-arch (amd64 + arm64) image and push to a registry.
# Example: just docker-build-multiarch ghcr.io/me/full-stack-template:0.1.0
docker-build-multiarch TAG:
    docker buildx build --platform linux/amd64,linux/arm64 -t {{TAG}} --push .

# Run the production image locally, mounting a named volume for SQLite.
docker-run:
    docker run --rm -p 3000:3000 -v app-data:/data --env-file .env full-stack-template:latest
```

- [ ] **Step 2: Confirm `just` lists the new recipes**

Run: `just`
Expected: output includes `docker-build`, `docker-build-multiarch TAG`, `docker-run`.

- [ ] **Step 3: Verify `just docker-build` works end-to-end**

Run: `just docker-build`
Expected: builds successfully, tags as `full-stack-template:latest`. `docker images full-stack-template:latest` shows the tag.

- [ ] **Step 4: Commit**

```bash
git add justfile
git commit -m "Add justfile recipes for Docker image build and run"
```

---

## Task 5: End-to-end runtime verification

**Files:** none (verification only — no commits in this task)

This task does not produce code; it confirms the image actually works end-to-end. If anything fails here, return to the earlier task that owns the failure rather than patching forward.

- [ ] **Step 1: Confirm `.env` is populated**

Run: `test -f .env && grep -E '^(COOKIE_SECRET|JWT_PRIVATE_KEY_PEM|JWT_PUBLIC_KEY_PEM|PUBLIC_BASE_URL)=' .env | wc -l`
Expected: `4` (all four required vars present). If not, run `cp .env.example .env && just gen-jwt-keys >> .env` and edit the file.

- [ ] **Step 2: Start the container with a fresh named volume**

```bash
docker volume rm app-data 2>/dev/null || true
just docker-run
```

The container runs in the foreground (no `-d`). Watch for log lines:
- `backend listening` (Axum is up)
- Any migration messages
- No panics

Leave this terminal running; open a second terminal for the next steps.

- [ ] **Step 3: Verify the health endpoint**

In a new terminal:
Run: `curl -fsS http://localhost:3000/api/health`
Expected: `{"ok":true}` (HTTP 200). `curl -f` exits non-zero on any error.

- [ ] **Step 4: Verify the SPA is served**

Run: `curl -fsS http://localhost:3000/ | grep -i '<html'`
Expected: an HTML opening tag (the SPA's `index.html`).

- [ ] **Step 5: Verify the HEALTHCHECK reports `healthy`**

Wait ~15 seconds (HEALTHCHECK has a 10s start period plus a 30s interval).
Run: `docker ps --filter ancestor=full-stack-template:latest --format '{{.Status}}'`
Expected: status contains `(healthy)`.

- [ ] **Step 6: Verify volume persistence**

Through the SPA at `http://localhost:3000`, register a new account using your `INITIAL_INVITE_CODE` (or sign up via OAuth). Confirm you are logged in.

Then in the terminal running the container, press Ctrl-C to stop it. Re-run: `just docker-run`. After it boots, reload the browser and confirm you can still log in with the same account — the SQLite file persisted in the named volume across container lifecycles.

- [ ] **Step 7: Stop the container**

Press Ctrl-C in the terminal running `just docker-run`.

- [ ] **Step 8: Verify the multi-arch recipe parses (without pushing)**

Run: `docker buildx build --platform linux/amd64,linux/arm64 -t full-stack-template:multiarch-test .`
Expected: build runs both platforms (slower than single-arch). It will warn that without `--push` or `--load` the result is discarded — that's fine; we are only verifying the recipe and the Dockerfile cross-build cleanly. Cancel with Ctrl-C once both platforms are clearly compiling.

If you would rather not wait for the full cross-build, skip this step and trust that `docker buildx build --platform linux/amd64,linux/arm64` will work since the Dockerfile uses no architecture-specific flags.

---

## Task 6: Update README "Deploying" section

**Files:**
- Modify: `README.md`

The existing "Deploying" section at the end of `README.md` describes building a release binary directly. Replace it with Docker-based instructions while preserving the option of building the binary directly (still useful).

- [ ] **Step 1: Replace the existing "Deploying" section**

Find this section in `README.md` (it begins with `## Deploying` and ends just before `## License`):

```markdown
## Deploying

The release build is a single binary plus a `static/` directory:

```sh
just build
# backend/target/release/backend now serves SPA + API from one process.
# Provide a writable directory for SQLite — see DATABASE_URL in .env.example.
```

Mount `data/` for the SQLite file, set the env vars, point a TLS terminator at
port `3000`, and you're done. The binary needs no Node runtime in prod.
```

Replace it with:

```markdown
## Deploying

### Single Docker image (recommended)

The repo ships a multi-stage `Dockerfile` that builds the SPA, compiles the
backend, and packages both into one ~100 MB `debian:bookworm-slim` image. The
container runs as a non-root user, persists SQLite under `/data`, and includes
a `HEALTHCHECK` against `/api/health`.

```sh
just docker-build         # build full-stack-template:latest for your host arch
just docker-run           # run it, mounting the `app-data` named volume at /data
```

`just docker-run` reads `.env` for required runtime vars (`COOKIE_SECRET`,
`JWT_PRIVATE_KEY_PEM`, `JWT_PUBLIC_KEY_PEM`, `PUBLIC_BASE_URL`, plus any
OAuth credentials you use). The SPA is served from `/` and the API from
`/api`, both on port `3000`.

For a multi-architecture image (amd64 + arm64) pushed to a registry:

```sh
just docker-build-multiarch ghcr.io/<you>/<repo>:0.1.0
```

Mount a real volume in production (`-v /srv/app-data:/data` or a Docker named
volume) and put a TLS terminator (Caddy, nginx, Cloudflare) in front of
port `3000`.

### Bare binary (alternative)

If you would rather not use Docker, the release build is a single binary plus
a `static/` directory:

```sh
just build
# backend/target/release/backend now serves SPA + API from one process.
# Provide a writable directory for SQLite — see DATABASE_URL in .env.example.
```
```

- [ ] **Step 2: Confirm the section reads cleanly**

Run: `grep -A 40 '^## Deploying' README.md`
Expected: the new section content shown above, ending before `## License`.

- [ ] **Step 3: Commit**

```bash
git add README.md
git commit -m "Document single-Dockerfile deployment in README"
```

---

## Done

After Task 6, the working tree should be clean. The repo now ships:

- A `/api/health` endpoint with a passing integration test.
- A four-stage `Dockerfile` and matching `.dockerignore`.
- `just docker-build`, `just docker-build-multiarch`, `just docker-run` recipes.
- README instructions for both Docker and bare-binary deployment.

`just docker-build && just docker-run` (with a populated `.env`) gets the
whole application running on any Docker host.
