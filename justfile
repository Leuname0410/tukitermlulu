set windows-shell := ["powershell.exe", "-NoLogo", "-NoProfile", "-Command"]

export TAURI_APP_PATH := justfile_directory() / "crates/desktop"

default:
    just --list

dev:
    npm --prefix ui run tauri dev

test-int:
    docker compose -f tests/fixtures/docker-compose.yml up -d --build --wait
    cargo test -p tuki-adapters --features integration -- --test-threads=1

check:
    cargo fmt --all -- --check
    cargo clippy --workspace --all-targets -- -D warnings
    cargo test --workspace
    cargo audit
    cargo deny check
    npm --prefix ui run lint
    npm --prefix ui run typecheck
    npm --prefix ui run test
    npm --prefix ui audit --audit-level=high
