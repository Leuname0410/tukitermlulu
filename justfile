set windows-shell := ["powershell.exe", "-NoLogo", "-NoProfile", "-Command"]

export TAURI_APP_PATH := justfile_directory() / "crates/desktop"

default:
    just --list

dev:
    npm --prefix ui run tauri dev

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
