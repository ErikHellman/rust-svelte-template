set dotenv-load := true
set positional-arguments

default:
    @just --list

# Run backend and frontend concurrently with hot reload.
dev:
    #!/usr/bin/env bash
    set -eu
    trap 'kill 0' EXIT
    just dev-backend &
    just dev-frontend &
    wait

# Watch + run the backend on http://localhost:3000.
dev-backend:
    cd backend && cargo watch -q -c -w src -w migrations -x run

# Vite dev server on http://localhost:5173.
dev-frontend:
    cd frontend && pnpm dev --host

# Build the frontend into backend/static, then the backend release binary.
build:
    cd frontend && pnpm install && pnpm build
    cd backend && cargo build --release

# Run all tests (backend Rust + frontend vitest).
test:
    cd backend && cargo test
    cd frontend && pnpm test || true

# sqlx migrations
migrate:
    cd backend && sqlx migrate run

migrate-add NAME:
    cd backend && sqlx migrate add -r "{{NAME}}"

# Refresh the offline sqlx query cache (commit the result).
prepare:
    cd backend && cargo sqlx prepare

# Lint + format-check + type-check everything.
check:
    cd backend && cargo fmt --check && cargo clippy --all-targets -- -D warnings
    cd frontend && pnpm exec svelte-check --tsconfig ./tsconfig.json && pnpm exec eslint src --max-warnings 0 && pnpm exec prettier --check src

# Format the codebase.
fmt:
    cd backend && cargo fmt
    cd frontend && pnpm exec prettier --write src

# Generate an RSA keypair for JWT signing and print PEM-encoded env values.
gen-jwt-keys:
    #!/usr/bin/env bash
    set -eu
    tmp=$(mktemp -d)
    openssl genpkey -algorithm RSA -pkeyopt rsa_keygen_bits:2048 -out "$tmp/jwt.key" 2>/dev/null
    openssl rsa -pubout -in "$tmp/jwt.key" -out "$tmp/jwt.pub" 2>/dev/null
    echo "# Paste these into your .env (mind the surrounding quotes):"
    echo
    printf 'JWT_PRIVATE_KEY_PEM="%s"\n' "$(awk '{printf "%s\\n", $0}' "$tmp/jwt.key")"
    printf 'JWT_PUBLIC_KEY_PEM="%s"\n'  "$(awk '{printf "%s\\n", $0}' "$tmp/jwt.pub")"
    rm -rf "$tmp"

# Build the production image for the local host architecture.
docker-build:
    docker build -t full-stack-template:latest .

# Build a multi-arch (amd64 + arm64) image and push to a registry.
# Example: just docker-build-multiarch ghcr.io/me/full-stack-template:0.1.0
docker-build-multiarch TAG:
    docker buildx build --platform linux/amd64,linux/arm64 -t {{TAG}} --push .

# Run the production image locally, mounting a named volume for SQLite.
# We mount .env as a file so dotenvy parses it inside the container —
# `docker --env-file` does not handle the multi-line PEM escape sequences
# in our .env format (it passes literal `\n` instead of real newlines).
docker-run:
    docker run --rm -p 3000:3000 -v app-data:/data -v "$(pwd)/.env":/app/.env:ro full-stack-template:latest
