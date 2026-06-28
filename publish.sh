#!/usr/bin/env bash
set -euo pipefail

# publish.sh — Build and publish mtparser to crates.io and/or npm
#
# Usage:
#   ./publish.sh rs     # Build + publish Rust crate to crates.io
#   ./publish.sh js     # Build WASM + publish to npm (package name: mtparser)
#   ./publish.sh all    # Both
#   ./publish.sh rs --dry-run    # Dry run (no actual publish)
#   ./publish.sh js --dry-run

set -e

MODE="${1:-all}"
DRY_RUN="${2:-}"

ROOT="$(cd "$(dirname "$0")" && pwd)"
RS_DIR="$ROOT/mtparser-core"
WASM_DIR="$ROOT/mtparser-wasm"

# ── Helpers ──────────────────────────────────────────────

log() { echo -e "\033[1;32m▶\033[0m $1"; }
warn() { echo -e "\033[1;33m⚠\033[0m $1"; }
err() { echo -e "\033[1;31m✗\033[0m $1" >&2; }

check_tool() {
    if ! command -v "$1" &>/dev/null; then
        err "$1 is not installed. Please install it first."
        exit 1
    fi
}

get_version() {
    grep '^version' "$1" | head -1 | sed 's/.*"\(.*\)".*/\1/'
}

# ── Rust crate ───────────────────────────────────────────

publish_rs() {
    log "Building Rust crate..."
    cd "$RS_DIR"
    cargo build --release

    if [ "$DRY_RUN" = "--dry-run" ]; then
        log "Dry run: cargo publish --dry-run"
        cargo publish --dry-run
        log "Rust dry run complete."
    else
        log "Publishing to crates.io..."
        cargo publish
        log "Rust crate published."
    fi
}

# ── JS / WASM package ────────────────────────────────────

publish_js() {
    check_tool wasm-pack
    check_tool node

    log "Building WASM package..."
    cd "$WASM_DIR"
    wasm-pack build --release --target bundler

    # wasm-pack names the package after the crate name (mtparser-wasm).
    # Override to "@shapled/mtparser" for npm.
    log "Setting npm package name to '@shapled/mtparser'..."
    sed -i.bak 's/"name": "mtparser-wasm"/"name": "@shapled\/mtparser"/' pkg/package.json
    rm -f pkg/package.json.bak

    # Show info
    local version
    version=$(grep '"version"' pkg/package.json | head -1 | sed 's/.*"\(.*\)".*/\1/')
    local size
    size=$(du -sh pkg/mtparser_wasm_bg.wasm | cut -f1)
    log "Package: mtparser@$version (wasm: $size)"

    if [ "$DRY_RUN" = "--dry-run" ]; then
        log "Dry run: npm publish --access=public --dry-run"
        cd pkg && npm publish --access=public --dry-run
        log "JS dry run complete."
    else
        log "Publishing to npm..."
        cd pkg && npm publish --access=public
        log "JS package published."
    fi
}

# ── Main ─────────────────────────────────────────────────

main() {
    case "$MODE" in
        rs)
            publish_rs
            ;;
        js)
            publish_js
            ;;
        all)
            publish_rs
            echo ""
            publish_js
            ;;
        *)
            err "Usage: $0 {rs|js|all} [--dry-run]"
            exit 1
            ;;
    esac
}

main
