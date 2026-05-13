CREATE TABLE users (
    id            TEXT PRIMARY KEY,
    email         TEXT NOT NULL UNIQUE,
    display_name  TEXT,
    avatar_url    TEXT,
    created_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE TABLE identities (
    provider          TEXT NOT NULL,
    provider_user_id  TEXT NOT NULL,
    user_id           TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at        TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    PRIMARY KEY (provider, provider_user_id)
);
CREATE INDEX idx_identities_user ON identities(user_id);

CREATE TABLE refresh_tokens (
    id            TEXT PRIMARY KEY,
    token_hash    TEXT NOT NULL,
    user_id       TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    issued_at     TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    expires_at    TEXT NOT NULL,
    revoked_at    TEXT,
    replaced_by   TEXT REFERENCES refresh_tokens(id)
);
CREATE INDEX idx_refresh_user ON refresh_tokens(user_id);
