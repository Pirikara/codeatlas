# CodeAtlas Development Guide

## Prerequisites

- Rust (`cargo`)
- Node.js / npm (for building `codeatlas-mcp`)

`.tool-versions`: `rust 1.87.0`

## Setup

```bash
# CLI
cargo build

# MCP
cd codeatlas-mcp
npm install
npm run build
```

Notes:
- Successful `codeatlas index` updates the registry at `~/.codeatlas/registry.json`
- In tests, use `CODEATLAS_REGISTRY_PATH` to isolate registry output

## Frequently Used Commands

```bash
# Tests
cargo test -- --test-threads=1

# CLI help
cargo run -- --help

# Index
./target/debug/codeatlas index --force /path/to/repo
./target/debug/codeatlas index --force --metrics /path/to/repo

# Generate embeddings
./target/debug/codeatlas embed /path/to/repo

# Search (3 modes)
./target/debug/codeatlas query "UserService" -p /path/to/repo --mode bm25
./target/debug/codeatlas query "UserService" -p /path/to/repo --mode vector
./target/debug/codeatlas query "UserService" -p /path/to/repo --mode hybrid
```

## Existing Fixtures

`testdata/benchmark/`:
- `ts-webapp`
- `go-cli`
- `ruby-service`

Validation example:

```bash
./target/debug/codeatlas index --force testdata/benchmark/ts-webapp
./target/debug/codeatlas embed testdata/benchmark/ts-webapp
./target/debug/codeatlas status --json testdata/benchmark/ts-webapp
```

## Common Implementation Changes

### Add a New Language

1. Add tree-sitter grammar dependency to `Cargo.toml`
2. Extend `Language` / `ParserPool` in `src/parser/mod.rs`
3. Add `src/parser/extract/{lang}.rs`
4. Add `src/parser/calls/{lang}.rs` and wire into `calls/mod.rs`
5. Add `src/parser/imports/{lang}.rs` and wire into `imports/mod.rs`
6. Add extension mapping in `Language::from_extension()`
7. If needed, add language-specific resolution in `src/analyzer/resolver/*`

### Add a New Relationship Kind

1. Add enum variant in `RelationKind` (`src/analyzer/resolver/mod.rs`)
2. Implement resolver logic under `src/analyzer/resolver/`
3. Integrate into `resolve_relationships` (`src/analyzer/mod.rs`)
4. Update query layer + CLI output (`src/query/mod.rs`, `src/cli/*`)
5. Check schema impact in `src/storage/mod.rs`

### Add a New CLI Command

1. Add args in `src/cli/mod.rs`
2. Implement `src/cli/{command}_cmd.rs`
3. Add dispatch in `src/main.rs`
4. If MCP exposure is needed, update `codeatlas-mcp/src/tools.ts` / `resources.ts`

## SQLite Debugging

```bash
DB=/path/to/repo/.codeatlas/index.db

sqlite3 "$DB" "SELECT COUNT(*) FROM symbols;"
sqlite3 "$DB" "SELECT kind, COUNT(*) FROM relationships GROUP BY kind ORDER BY COUNT(*) DESC;"
sqlite3 "$DB" "SELECT model_id, dims, COUNT(*) FROM embeddings GROUP BY model_id, dims;"
```

## Parser Behavior Checks

```bash
cargo test parser::extract::typescript::tests::interface_method_with_generics
cargo test parser::imports::go::tests::go_grouped_import
cargo test parser::calls::typescript::tests::ts_new_expression
```

## Test Status (2026-03-09)

- `cargo test -- --test-threads=1`: all passing
  - Unit: 60
  - Integration: 55

## Known Limitations

- Vector search is exact brute-force cosine (ANN deferred; re-evaluate at 100K+ symbols).
- Embedding model is currently fixed to `all-MiniLM-L6-v2`.
- `impact` defaults to all relationship kinds (`--calls-only` for CALLS-only).
- Ruby static `send(:sym)` / `send("str")` extraction is supported.
- Ruby `method_missing` fallback is supported for unresolved implicit-self calls (explicit-receiver cases remain out of scope).
- TSX/JSX call resolution is limited.
