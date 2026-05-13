-- Roles on users. 'user' is the default; 'admin' grants elevated access.
ALTER TABLE users ADD COLUMN role TEXT NOT NULL DEFAULT 'user';

-- Password credentials live in a separate table so the users row stays the
-- single source of identity and so a user can later add/remove a password
-- without touching the canonical user record.
CREATE TABLE password_credentials (
    user_id       TEXT PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    password_hash TEXT NOT NULL,
    created_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

-- Single-use invite codes. `email` optionally binds the code to a specific
-- address (password signups must match; OAuth signups must come back from the
-- provider with this email). `role` lets an invite carry an elevated role,
-- used by the bootstrap admin invite.
CREATE TABLE invite_codes (
    code            TEXT PRIMARY KEY,
    email           TEXT,
    role            TEXT NOT NULL DEFAULT 'user',
    created_at      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    expires_at      TEXT,
    used_at          TEXT,
    used_by_user_id TEXT REFERENCES users(id) ON DELETE SET NULL
);
CREATE INDEX idx_invite_email ON invite_codes(email);
