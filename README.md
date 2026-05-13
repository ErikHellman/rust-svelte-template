# Full-stack template

A starter project that pairs a **Rust** API (Axum + sqlx + SQLite) with a
**Svelte 5** SPA, complete with OAuth login (Google, GitHub, Apple, Microsoft),
JWT access tokens, rotating refresh tokens, a small example CRUD feature
("notes"), and the tooling to go from clone to running app in a few commands.

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
   # then open .env and fill in the OAuth client ids/secrets you need
   ```
4. **First boot**:
   ```sh
   just migrate
   just dev
   ```
5. Open <http://localhost:5173>, click "Sign in", pick a configured provider.

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

User clicks a provider → `/api/auth/<provider>/start` redirects to the provider
with PKCE + state cookie → provider redirects back to `/callback` → we exchange
the code, fetch user info, upsert a user, mint an **access JWT** + set a
rotating **refresh cookie** scoped to `/api/auth`. The SPA keeps the access
token in memory only (XSS-safer); `lib/api.ts` retries on 401 by hitting
`/api/auth/refresh` once before giving up.

Token reuse is detected: if the same refresh cookie is presented twice, the
second call is rejected (the first call rotated it).

## Adding a new feature module

Mirror `backend/src/notes/`:

1. Add a migration: `just migrate-add add_widgets` → fill in the `up.sql`.
2. Run `just migrate`, then `just prepare` so the offline cache picks it up.
3. Create `backend/src/widgets/{mod.rs, repo.rs}` (use `notes/` as a template).
4. Register `widgets::router()` in `backend/src/routes.rs`.
5. Add a frontend page in `frontend/src/routes/Widgets.svelte` and a route
   match in `App.svelte`.

## Deploying

The release build is a single binary plus a `static/` directory:

```sh
just build
# backend/target/release/backend now serves SPA + API from one process.
# Provide a writable directory for SQLite — see DATABASE_URL in .env.example.
```

Mount `data/` for the SQLite file, set the env vars, point a TLS terminator at
port `3000`, and you're done. The binary needs no Node runtime in prod.

## License

MIT (or whatever you choose — drop a LICENSE file).
