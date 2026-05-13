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

## Invite-gated registration

All user creation goes through `auth/mod.rs::register_oauth_user` (OAuth) or
`password_signup` (email/password), both of which:

1. Pull an invite via `invites::find_valid` (consumed-or-expired ⇒ rejected).
2. If the invite has a bound email, enforce it (case-insensitive match).
3. Insert the user with the invite's `role` (so the bootstrap admin code
   confers admin).
4. Call `invites::mark_used` inside the same transaction.

`INITIAL_INVITE_CODE` from the env is seeded idempotently at boot via
`invites::ensure_initial_admin`. Drop the env var once the admin account
exists — the row remains in the DB and is single-use.

Account linking across providers by verified email is intentionally **not**
done: a fresh user is created for each `(provider, provider_user_id)`. This is
a project-specific policy decision with real security implications (email
takeover, provider trust); revisit per project.

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
