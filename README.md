# Full-stack template

A starter project that pairs a **Rust** API (Axum + sqlx + SQLite) with a
**Svelte 5** SPA. It ships with invite-gated registration (email/password +
OAuth from Google, GitHub, Apple, Microsoft), per-user roles, JWT access
tokens, rotating refresh tokens, a small example CRUD feature ("notes"), and
the tooling to go from clone to running app in a few commands.

## Use this template for a new project

1. **Clone** (or use GitHub's "Use this template" button), then `git remote set-url origin …`.
2. **Rename** the crate and package:
   - `backend/Cargo.toml` → `name = "<your-app>"`
   - `frontend/package.json` → `"name": "<your-app>"`
   - Repository name + this README.
3. **Configure env**:
   ```sh
   cp .env.example .env
   just gen-jwt-keys >> .env       # appends RSA keys for JWT signing
   # open .env and:
   #  - set INITIAL_INVITE_CODE to a long random string
   #  - fill in the OAuth client ids/secrets you actually want
   ```
4. **First boot**:
   ```sh
   just migrate
   just dev
   ```
5. Open <http://localhost:5173>, click **Sign up**, paste the
   `INITIAL_INVITE_CODE`, then either set an email + password or pick a
   configured provider. The first signup with that code becomes the **admin**;
   the code is single-use.

Once you have an admin, generate per-person invite codes directly in SQLite
(see "Issuing more invites" below) or build an admin UI.

Only the providers you fill in are exposed; the rest return 503 at runtime, so
you don't have to set up all four.

## Stack

| Layer        | Choice                                                        |
| ------------ | ------------------------------------------------------------- |
| Backend      | Rust 2024, Axum 0.8, Tokio                                    |
| Database     | SQLite via sqlx 0.8 (compile-time checked queries, WAL mode)  |
| Auth         | OAuth 2.0 / OIDC → RS256 JWT access + rotating refresh        |
| Frontend     | Svelte 5 (runes), Vite 6, TypeScript                          |
| Routing (FE) | Tiny built-in hash router — swap for SvelteKit if you outgrow |
| Build (prod) | Vite builds into `backend/static/`; the Rust binary serves it |
| DX           | `just`, Docker Compose, pre-commit, GitHub Actions CI         |

## Repository layout

```
backend/          Rust crate. Axum HTTP server + sqlx + migrations.
  src/auth/       OAuth, JWT, refresh-token session, AuthUser extractor.
  src/notes/      Example user-scoped CRUD module — clone for new features.
  migrations/     sqlx migration .sql files (sequentially numbered).
  .sqlx/          Offline query cache. Run `just prepare` after query edits.
  tests/          Integration tests against a temp SQLite file.
frontend/         Svelte 5 SPA. Built into backend/static for prod.
  src/lib/        api fetcher, auth store, hash router.
  src/routes/     Home, Login, Notes pages.
justfile          The source of truth for dev commands. Run `just` to list.
docker-compose.yml  `docker compose up` for one-command dev.
.github/workflows/ci.yml  Type-check, lint, fmt, test, build on every push.
```

## Common commands

```sh
just              # list all targets
just dev          # backend (:3000) and frontend (:5173) concurrently
just test         # cargo test + vitest
just check        # fmt + clippy + svelte-check + eslint + prettier
just fmt          # auto-format everything
just migrate      # apply pending sqlx migrations
just migrate-add add_widgets    # create a new migration
just prepare      # refresh .sqlx/ offline cache after editing queries
just build        # production build → backend/target/release/<binary>
```

## How auth works (one-line version)

Registration is **invite-gated**. Login flows authenticate existing users only.

**Signup** — `POST /api/auth/signup/invite/check` validates the code (returning
any bound email + role), then either `POST /api/auth/signup/password` for
email + password, or `GET /api/auth/<provider>/signup/start?code=…` to attach
an OAuth identity. Either path consumes the invite atomically.

**Login** — existing accounts use `POST /api/auth/login` (password) or
`GET /api/auth/<provider>/start` (OAuth). The OAuth callback refuses to
auto-create users; if no identity matches, it redirects back to `/signup` with
an error.

**Session** — login/signup mint an RS256 access JWT (15 min, in memory on the
client) and a rotating refresh token in an HttpOnly cookie scoped to
`/api/auth`. The SPA's `lib/api.ts` retries once after a 401 by hitting
`/api/auth/refresh`. Refresh-token reuse is detected: presenting the same
cookie twice fails — the first call rotated it.

### Issuing more invites

There's no admin UI; create invites with a single SQL statement. From the
backend directory:

```sh
sqlite3 ../data/app.db \
  "INSERT INTO invite_codes (code, email, role) VALUES ('SOMETHING-RANDOM', 'newperson@example.com', 'user');"
```

`email` is optional. Omitting it lets anyone with the code register, with any
email. `role` defaults to `'user'`; use `'admin'` to grant admin.

### Roles

Every user has a `role` (default `'user'`). The role is included in the JWT
claims and in `/api/auth/me`, so server middleware can check it cheaply. There
are currently no admin-only endpoints in the template; add `AuthUser`-based
checks in handlers as you need them.

## Adding a new feature module

Mirror `backend/src/notes/`:

1. Add a migration: `just migrate-add add_widgets` → fill in the `up.sql`.
2. Run `just migrate`, then `just prepare` so the offline cache picks it up.
3. Create `backend/src/widgets/{mod.rs, repo.rs}` (use `notes/` as a template).
4. Register `widgets::router()` in `backend/src/routes.rs`.
5. Add a frontend page in `frontend/src/routes/Widgets.svelte` and a route
   match in `App.svelte`.

## Deploying

### Single Docker image (recommended)

The repo ships a multi-stage `Dockerfile` that builds the SPA, compiles the
backend, and packages both into one ~125 MB `debian:bookworm-slim` image.
The container runs as a non-root user, persists SQLite under `/data`, and
includes a `HEALTHCHECK` against `/api/health`.

```sh
just docker-build         # build full-stack-template:latest for your host arch
just docker-run           # run it on :3000, mounting the `app-data` named volume at /data
```

#### Required env vars

The image fails fast at boot unless these are provided at runtime:

- `COOKIE_SECRET` — 32+ bytes (`openssl rand -hex 32`)
- `JWT_PRIVATE_KEY_PEM`, `JWT_PUBLIC_KEY_PEM` — RSA keypair from `just gen-jwt-keys`
- `PUBLIC_BASE_URL` — the origin the SPA reaches the API on (e.g. `https://yourapp.com`)
- Whichever OAuth `*_CLIENT_ID` / `*_CLIENT_SECRET` pairs you actually use
- `INITIAL_INVITE_CODE` — only on first boot, then unset

The Dockerfile bakes sensible defaults for `DATABASE_URL` (sqlite under `/data`),
`BIND_ADDR` (`0.0.0.0:3000`), `STATIC_DIR`, and `RUST_LOG`; override only if needed.

#### .env handling (important)

The `just docker-run` recipe mounts your local `.env` as a file at `/app/.env`
inside the container (not `docker --env-file`). This is intentional: the
multi-line PEM secrets in `.env` use `\n` escape sequences that `dotenvy`
parses correctly when reading the file directly, but `docker --env-file`
passes the literal `\n` characters through, corrupting the key. For
non-secret env vars only, `--env-file` would be fine; for the full `.env`,
mount the file.

Ensure the host `.env` is readable by uid 1000 (the container's `app` user):

```sh
chmod 644 .env       # if your default umask makes it 600
```

#### Multi-architecture image

For a multi-arch image (amd64 + arm64) pushed to a registry:

```sh
docker login <registry>    # if not already authenticated
just docker-build-multiarch ghcr.io/<you>/<repo>:0.1.0
```

#### Production wiring

Mount a real volume (`-v /srv/app-data:/data` or a Docker named volume) and
put a TLS terminator (Caddy, nginx, Cloudflare) in front of port `3000`. The
container does no TLS itself.

### Bare binary (alternative)

If you'd rather not use Docker, the release build is a single binary plus a
`static/` directory:

```sh
just build
# backend/target/release/backend now serves SPA + API from one process.
# Provide a writable directory for SQLite — see DATABASE_URL in .env.example.
```

## License

MIT (or whatever you choose — drop a LICENSE file).
