#!/usr/bin/env bash
set -euo pipefail

# publish.sh — Build and publish mtparser (Gleam)
#
# Usage:
#   ./publish.sh build     # Build JS package into pkg/ (no publish)
#   ./publish.sh npm       # Build + publish to npm as @shapled/mtparser
#   ./publish.sh hex       # Publish the Gleam package to hex
#   ./publish.sh all       # hex + npm
#   ./publish.sh npm --dry-run
#
# The npm package replaces the old Rust/wasm one (../mtparser-rs, 0.3.x);
# the version continues that lineage (gleam.toml is the single source of
# truth — the JS consumer directly uses the compiled Gleam classes).

MODE="${1:-all}"
DRY_RUN="${2:-}"

ROOT="$(cd "$(dirname "$0")" && pwd)"
PKG="$ROOT/pkg"
BUILD_JS="$ROOT/build/dev/javascript"

NPM_NAME="@shapled/mtparser"
# Always publish to the official registry — a locally configured mirror
# (npmmirror etc.) must never receive the publish.
NPM_REGISTRY="https://registry.npmjs.org"

# ── Helpers ──────────────────────────────────────────────

log() { echo -e "\033[1;32m▶\033[1;0m $1"; }
warn() { echo -e "\033[1;33m⚠\033[1;0m $1"; }
err() { echo -e "\033[1;31m✗\033[1;0m $1" >&2; }

check_tool() {
    if ! command -v "$1" &>/dev/null; then
        err "$1 is not installed. Please install it first."
        exit 1
    fi
}

version() {
    grep '^version' "$ROOT/gleam.toml" | head -1 | sed 's/version *= *"\(.*\)"/\1/'
}

# ── npm package ───────────────────────────────────────────

build_js() {
    check_tool gleam
    check_tool node

    log "Clean JavaScript build (with .d.mts declarations)..."
    cd "$ROOT"
    rm -rf build
    gleam build --target javascript

    if [ ! -f "$BUILD_JS/mtparser/mtparser.d.mts" ]; then
        err "Type declarations missing — check [javascript] typescript_declarations in gleam.toml."
        exit 1
    fi

    log "Assembling pkg/..."
    rm -rf "$PKG"
    mkdir -p "$PKG"

    # The compiled tree is position-relative (prelude at the root, sibling
    # package dirs) — copy it as-is, then prune what the library never
    # imports: the test-only dependency gleeunit with the test modules,
    # stdlib's Erlang FFI sources, and build metadata.
    cp -R "$BUILD_JS"/. "$PKG"/
    rm -rf "$PKG"/gleeunit
    rm -rf "$PKG"/_gleam_artefacts "$PKG"/*/_gleam_artefacts
    rm -f "$PKG"/gleam_stdlib/*.erl "$PKG"/fingerprint \
        "$PKG"/mtparser/*_test.mjs "$PKG"/mtparser/*_test.d.mts \
        "$PKG"/mtparser/gleam@@private_main*

    local ver
    ver="$(version)"
    log "Writing package.json ($NPM_NAME@$ver)..."
    cat > "$PKG/package.json" << JSON
{
  "name": "$NPM_NAME",
  "version": "$ver",
  "description": "Parser for MySQL mysqltest and MariaDB mariadb-test files",
  "type": "module",
  "license": "Apache-2.0",
  "repository": {
    "type": "git",
    "url": "git+https://github.com/shapled/mtparser.git"
  },
  "main": "./mtparser/mtparser.mjs",
  "types": "./mtparser/mtparser.d.mts",
  "exports": {
    ".": {
      "types": "./mtparser/mtparser.d.mts",
      "default": "./mtparser/mtparser.mjs"
    },
    "./ast": {
      "types": "./mtparser/mtparser/ast.d.mts",
      "default": "./mtparser/mtparser/ast.mjs"
    },
    "./error": {
      "types": "./mtparser/mtparser/error.d.mts",
      "default": "./mtparser/mtparser/error.mjs"
    },
    "./helper": {
      "types": "./mtparser/mtparser/helper.d.mts",
      "default": "./mtparser/mtparser/helper.mjs"
    },
    "./parser": {
      "types": "./mtparser/mtparser/parser.d.mts",
      "default": "./mtparser/mtparser/parser.mjs"
    },
    "./package.json": "./package.json"
  },
  "files": [
    "mtparser/",
    "gleam_stdlib/",
    "prelude.mjs",
    "prelude.d.mts"
  ]
}
JSON

    # npm renders the README on the package page.
    [ -f "$ROOT/README.md" ] && cp "$ROOT/README.md" "$PKG/README.md"

    smoke_test

    local size
    size="$(du -sh "$PKG" | cut -f1)"
    log "Package assembled: $NPM_NAME@$ver ($size)"
}

smoke_test() {
    log "Smoke test: import + parse + instanceof..."
    cd "$PKG"
    node --input-type=module -e '
import { parse } from "./mtparser/mtparser.mjs";
import { Ok } from "./gleam_stdlib/gleam.mjs";
import { DashCommand, Echo } from "./mtparser/mtparser/ast.mjs";

const result = parse("--echo hello $var\n");
if (!(result instanceof Ok)) throw new Error("parse failed");
const stmts = [...result[0]];
if (stmts.length !== 1) throw new Error("expected 1 statement");
const stmt = stmts[0];
if (!(stmt instanceof DashCommand) || !(stmt.statement instanceof Echo))
  throw new Error("expected DashCommand wrapping Echo");
console.log("smoke ok:", [...stmt.statement.text].map(p => p.text ?? ("$" + p.name)).join(""));
' || { err "Smoke test failed."; exit 1; }
    cd "$ROOT"
}

publish_js() {
    check_tool npm

    if [ "$DRY_RUN" != "--dry-run" ]; then
        log "Verifying npm login on $NPM_REGISTRY..."
        if ! npm whoami --registry="$NPM_REGISTRY" &>/dev/null; then
            err "Not logged in to $NPM_REGISTRY. Run: npm login --registry=$NPM_REGISTRY"
            exit 1
        fi
    fi

    build_js

    if [ "$DRY_RUN" = "--dry-run" ]; then
        log "Dry run: npm publish --access=public --dry-run (registry: $NPM_REGISTRY)"
        (cd "$PKG" && npm publish --access=public --dry-run --registry="$NPM_REGISTRY")
        log "npm dry run complete."
    else
        log "Publishing to $NPM_REGISTRY..."
        (cd "$PKG" && npm publish --access=public --registry="$NPM_REGISTRY")
        log "npm package published."
    fi
}

# ── hex package ───────────────────────────────────────────

publish_hex() {
    check_tool gleam

    if [ "$DRY_RUN" = "--dry-run" ]; then
        warn "gleam publish has no dry run — skipping hex publish."
        return
    fi

    log "Publishing Gleam package to hex..."
    cd "$ROOT"
    gleam publish
    log "hex package published."
}

# ── Main ─────────────────────────────────────────────────

main() {
    case "$MODE" in
        build)
            build_js
            ;;
        npm|js)
            publish_js
            ;;
        hex)
            publish_hex
            ;;
        all)
            publish_hex
            echo ""
            publish_js
            ;;
        *)
            err "Usage: $0 {build|npm|hex|all} [--dry-run]"
            exit 1
            ;;
    esac
}

main
