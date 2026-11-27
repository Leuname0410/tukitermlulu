set windows-shell := ["powershell.exe", "-NoLogo", "-NoProfile", "-Command"]

export TAURI_APP_PATH := justfile_directory() / "crates/desktop"

default:
    just --list

dev:
    npm --prefix ui run tauri dev

test-int:
    docker compose -f tests/fixtures/docker-compose.yml up -d --build --wait
    cargo test -p tuki-adapters --features integration -- --test-threads=1

kpi:
    docker compose -f tests/fixtures/docker-compose.yml up -d --build --wait
    cargo test -p tuki-adapters --features integration --test kpi -- --ignored --nocapture --test-threads=1

bundle:
    npm --prefix ui run tauri build{{ if os() == "macos" { " --target universal-apple-darwin" } else { "" } }}

[windows]
bundle-size:
    Get-ChildItem -Path target -Recurse -File -ErrorAction SilentlyContinue | Where-Object { $_.FullName -match '[\\/]release[\\/]bundle[\\/]' -and $_.Extension -in @('.exe', '.msi', '.AppImage', '.deb', '.dmg', '.rpm') } | ForEach-Object { '{0} {1:N2} MB' -f $_.FullName, ($_.Length / 1MB) }

[unix]
bundle-size:
    find target -path '*/release/bundle/*' -type f \( -name '*.exe' -o -name '*.msi' -o -name '*.AppImage' -o -name '*.deb' -o -name '*.dmg' -o -name '*.rpm' \) -exec du -m {} \;

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
