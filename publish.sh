#!/usr/bin/env bash
set -euo pipefail

# publish.sh — Build and publish mtparser
#
# Usage:
#   ./publish.sh build-js     # Build WASM + TS types (no publish)
#   ./publish.sh build-rs     # Build Rust crate (no publish)
#   ./publish.sh js           # Build WASM + publish to npm
#   ./publish.sh rs           # Build + publish Rust crate to crates.io
#   ./publish.sh all          # Build + publish both
#   ./publish.sh js --dry-run

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

# ── Rust crate ───────────────────────────────────────────

build_rs() {
    log "Building Rust crate..."
    cd "$RS_DIR"
    cargo build --release
}

publish_rs() {
    build_rs

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

build_js() {
    check_tool wasm-pack

    cd "$WASM_DIR"

    # Generate TypeScript types via ts-rs (output to ./types/)
    log "Generating TypeScript types..."
    rm -rf types
    mkdir -p types
    TS_RS_EXPORT_DIR="$WASM_DIR/types" cargo test --features typescript -p mtparser 2>&1 | tail -1

    log "Building WASM package..."
    wasm-pack build --release --target bundler

    # wasm-pack names the package after the crate name (mtparser-wasm).
    # Override to "@shapled/mtparser" for npm.
    log "Setting npm package name to '@shapled/mtparser'..."
    sed -i.bak 's/"name": "mtparser-wasm"/"name": "@shapled\/mtparser"/' pkg/package.json
    rm -f pkg/package.json.bak

    # Copy generated TypeScript types into pkg
    log "Copying TypeScript types into pkg..."
    mkdir -p pkg/types
    cp -r "$WASM_DIR/types/"* pkg/types/

    # Generate barrel index.ts that re-exports all types
    log "Generating type barrel..."
    cat > pkg/types/index.ts << 'BARREL'
// Auto-generated barrel — re-exports all types
BARREL
    for f in pkg/types/*.ts; do
        [ "$(basename "$f")" = "index.ts" ] && continue
        name=$(basename "$f" .ts)
        echo "export type { $name } from './$name';" >> pkg/types/index.ts
    done

    # Override .d.ts — import Statement for return type, re-export all from barrel
    log "Writing typed declaration..."
    cat > pkg/mtparser_wasm.d.ts << 'DTS'
export type * from './types/index';
import type { Statement } from './types/index';

/**
 * Parse a mysqltest/mariadb-test file.
 * @param input - File content (.test or .inc)
 * @param version - "5.7", "8.0", "8.4", "9.7", "mariadb", "compatible", or omit for all MySQL
 * @returns Array of Statement objects, or throws on parse error
 */
export function parse_mt(input: string, version?: string | null): Statement[];
DTS

    # Update package.json to include types directory
    sed -i.bak2 's/"files": \[/"files": [\n    "types\/",/' pkg/package.json
    rm -f pkg/package.json.bak2

    # Show info
    local version
    version=$(grep '"version"' pkg/package.json | head -1 | sed 's/.*"\(.*\)".*/\1/')
    local size
    size=$(du -sh pkg/mtparser_wasm_bg.wasm | cut -f1)
    log "Package: @shapled/mtparser@$version (wasm: $size)"
}

publish_js() {
    build_js

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
        build-js)
            build_js
            ;;
        build-rs)
            build_rs
            ;;
        build-all)
            build_rs
            echo ""
            build_js
            ;;
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
            err "Usage: $0 {build-js|build-rs|build-all|js|rs|all} [--dry-run]"
            exit 1
            ;;
    esac
}

main
