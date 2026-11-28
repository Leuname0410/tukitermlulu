set windows-shell := ["powershell.exe", "-NoLogo", "-NoProfile", "-Command"]

export TAURI_APP_PATH := justfile_directory() / "crates/desktop"

default:
    just --list

dev:
    npm --prefix ui run tauri dev

test-int:
    docker compose -f tests/fixtures/docker-compose.yml up -d --build --wait
    cargo test -p tuki-adapters --features integration -- --test-threads=1

e2e:
    docker compose -f tests/fixtures/docker-compose.yml up -d --build --wait
    just e2e-build
    npm --prefix e2e ci
    npm --prefix e2e test

# El CSP del config bloquea el IPC crudo de terminal_write en el binario release.
# El override solo afecta al build que usa el e2e, no al repo ni a distribución.
[windows]
e2e-build:
    $env:TAURI_CONFIG = '{"app":{"security":{"csp":"default-src ''self''; style-src ''self'' ''unsafe-inline''; connect-src ''self'' ipc: http://ipc.localhost"}}}'; npm --prefix ui exec -- tauri build --no-bundle

[unix]
e2e-build:
    TAURI_CONFIG='{"app":{"security":{"csp":"default-src '\''self'\''; style-src '\''self'\'' '\''unsafe-inline'\''; connect-src '\''self'\'' ipc: http://ipc.localhost"}}}' npm --prefix ui exec -- tauri build --no-bundle

coverage:
    docker compose -f tests/fixtures/docker-compose.yml up -d --build --wait
    cargo llvm-cov clean --workspace
    cargo llvm-cov -p tuki-core -p tuki-application --fail-under-lines 85 --summary-only
    cargo llvm-cov -p tuki-adapters --features integration --fail-under-lines 70 --summary-only -- --test-threads=1

kpi:
    docker compose -f tests/fixtures/docker-compose.yml up -d --build --wait
    cargo test --release -p tuki-adapters --features integration --test kpi -- --ignored --nocapture --test-threads=1

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
