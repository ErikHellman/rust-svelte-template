# CLAUDE.md

Repo-wide conventions for future Claude sessions working in this template.

## Commands

The `justfile` is the source of truth for every dev command. Always prefer
`just <target>` over invoking cargo / pnpm directly — that's how CI and Docker
call into it, so it stays in sync.

Common entry points: `just dev`, `just test`, `just check`, `just migrate`,
`just prepare`.

## sqlx offline cache

Queries use `sqlx::query!` and `sqlx::query_as!` (compile-time checked). After
editing any SQL — including migrations — run `just prepare` and commit the
resulting `backend/.sqlx/` changes. CI runs `cargo sqlx prepare --check` and
fails if the cache is stale.

SQLite columns are inferred as nullable by default. Annotate non-null columns
in `SELECT`s with `column as "column!"` to get `String` instead of
`Option<String>`. See `backend/src/notes/repo.rs` for the pattern.

## OAuth account linking (template default vs. project policy)

`auth/mod.rs::upsert_user` always creates a fresh `users` row when a new
`(provider, provider_user_id)` pair arrives — even if the email matches an
existing user. Account linking across providers by verified email is a
**project-specific policy decision** with real security implications (email
takeover, provider trust, etc.). Each forked project should make a deliberate
choice and adjust `upsert_user` to match.

## What this template is (and isn't)

It's a starting skeleton: auth, one example CRUD feature ("notes"), CI, and
dev tooling. Features beyond those are **per-project** — add them in the fork,
not upstream here. Resist the urge to grow the template into a framework.

## Code style

- Backend: `cargo fmt` defaults, `clippy -D warnings`. `AppError` for all
  fallible handlers; never `unwrap()` outside tests.
- Frontend: Prettier defaults (`.prettierrc.json`), ESLint flat config, no
  warnings. Svelte 5 runes (`$state`, `$effect`) — not Svelte 4 stores.
- Don't write WHAT-the-code-does comments. Comment only the non-obvious WHY.

## Useful files to know

- `backend/src/lib.rs` — `AppState` (the thing every handler receives).
- `backend/src/auth/oauth.rs` — provider-specific wiring lives here.
- `backend/src/routes.rs` — top-level router assembly + static file fallback.
- `frontend/src/lib/api.ts` — fetch wrapper with auto-refresh on 401.
- `frontend/src/lib/auth.svelte.ts` — auth store; access token is in memory
  only by design (refresh lives in an HttpOnly cookie).
