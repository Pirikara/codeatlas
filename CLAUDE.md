# CodeAtlas — CLAUDE.md

## Project Overview

CodeAtlas is a Rust-based code knowledge graph builder. It parses code with tree-sitter and stores symbols and relationships in SQLite. It can be consumed by AI tools via the MCP server in `codeatlas-mcp/`.

## Build and Run

```bash
# CLI
cargo build --release
./target/release/codeatlas --help

# MCP server
cd codeatlas-mcp
npm install
npm run build
```

## Tests

```bash
cargo test -- --test-threads=1
```

As of 2026-03-19:
- Unit tests: 74 passed
- Integration tests: 66 passed

## Current Directory Layout (Key Parts)

```text
src/
├── main.rs
├── scanner/mod.rs
├── parser/
│   ├── mod.rs
│   ├── extract/{mod,typescript,go,ruby}.rs
│   ├── calls/{mod,typescript,go,ruby}.rs
│   ├── dataflow/{mod,typescript,go,ruby}.rs
│   └── imports/{mod,typescript,go,ruby}.rs
├── analyzer/
│   ├── mod.rs
│   ├── resolver/{mod,calls,imports,inheritance}.rs
│   ├── community.rs
│   └── process.rs
├── storage/{mod,read,write}.rs
├── embedder/mod.rs
├── query/mod.rs
└── cli/{mod,index,metrics,status,embed_cmd,query_cmd,context_cmd,impact_cmd,subgraph_cmd,graph_query_cmd,impact_batch_cmd,clusters_cmd,processes_cmd,eval_cmd,dataflow_cmd}.rs

codeatlas-mcp/
├── src/{index,tools,resources,codeatlas}.ts
└── dist/
```

## Implementation Facts (Important)

### Supported Languages
- TypeScript / TSX / JavaScript / JSX
- Go
- Ruby

### Relationship Kinds
`CALLS`, `CALLS_UNRESOLVED`, `CALLS_EXTERNAL`, `IMPORTS`, `EXTENDS`, `IMPLEMENTS`, `DEFINES`, `CONTAINS`

### CALLS_UNRESOLVED / CALLS_EXTERNAL (P9.1)
- When `resolve_callee()` finds no match, a fallback creates `CALLS_UNRESOLVED` or `CALLS_EXTERNAL`
- `CALLS_EXTERNAL`: only when receiver contains `::` (e.g. `ActiveRecord::Base.execute`)
- All other unresolved calls → `CALLS_UNRESOLVED`
- External pseudo-symbols: `SymbolKind::External`, `file_path = "<external>"`, `start_line = 0`
- `confidence = 0.0` → excluded from `impact()` (default `min_confidence = 0.5`) but visible in `context()` outgoing
- External symbols are deleted and rebuilt on each index run
- UID format: `External:<external>:{name}:0`

### Search Modes
- `bm25` (FTS5)
- `vector` (fastembed + cosine)
- `hybrid` (BM25 + Vector + RRF)
- `grouped` (`query --grouped`: `processes` / `definitions` / `total`)
- `--grouped` and `--mode` are mutually exclusive

### Eval
- `eval --grouped` evaluates process-grouped search quality
- Grouped metrics: `process_recall` / `routing_accuracy`
- `--min-process-hit` works as grouped quality gate (`routing_accuracy`)

### Index Consistency
- Incremental indexing auto-cleans deleted files
- Deletion cleanup also runs with `--force`
- If deletions are detected, all files are re-parsed to keep communities/processes consistent
- `index --metrics` prints phase timings, parse failures, counts, and RSS
- `index --exclude-tests` skips test directories (`spec/`, `test/`, `__tests__/`) and test files (`*_spec.rb`, `*_test.go`, `*.test.ts`, etc.)

### Search Quality
- BM25 / vector / hybrid search excludes `File`, `Folder`, and `External` kind symbols from results
- Ruby receiver expressions longer than 80 chars or containing newlines are dropped (block/hash literal noise filter)

### Impact
- Default: traverse all relationship types
- `--calls-only`: traverse only `CALLS`
- Output includes `risk` / `summary` / `affected_processes` / `affected_modules`

### Subgraph
- `subgraph` returns reachable subgraph (nodes + edges)
- Supports `direction` (outgoing/incoming/both), `depth`, `edge_types`, `max_nodes`, `max_edges`
- Start symbol can be disambiguated by `name` + `--file`
- Also supports direct `--uid` / `--id` start selection (zero ambiguity)

### Graph Query (P5.4)
- `graph-query` executes read-only SQL (`SELECT` / `WITH ... SELECT`)
- 3 write protections: keyword validation / `stmt.readonly()` / `SQLITE_OPEN_READ_ONLY`
- MCP `graph-query` exposes the same read-only guarantees (with audit logging)

### Impact-batch (P4: VCS-independent)
- `impact-batch` accepts `--symbols` (id / name+file) or `--ranges` (file + line ranges)
- VCS operations (e.g., `git diff`) are not in Core; handled on MCP side
- Default kinds are `Function` / `Method` only (`--kinds` / `--all-kinds` available)
- MCP `detect-changes` runs `git diff` in Node.js and passes ranges to `impact-batch`

### Dataflow (P9.2)
- `dataflow` CLI + MCP tool: shows intra-function data flows (Assignment, Argument, StringInterp, Return, FieldAccess)
- `data_flows` table stores flows with `function_uid` reference (nullable for top-level code)
- Supported: TypeScript, Ruby, Go — raw expression text preserved (no normalization)
- Data flows are rebuilt from scratch on each index run

### MCP Multi-repo (P5.5)
- Successful `codeatlas index` auto-registers repo in `~/.codeatlas/registry.json`
- Registry path can be overridden by `CODEATLAS_REGISTRY_PATH`
- MCP `list_repos` returns registered repositories
- MCP tools accept `repo` as either name or absolute path
- When `repo` is omitted:
  - 1 repo in registry: auto-select
  - 0 repos: error prompting to run index
  - 2+ repos: error with available names

## SQLite

Main tables:
- `symbols`
- `relationships`
- `communities`, `community_members`
- `processes`, `process_steps`
- `data_flows`
- `file_index`
- `symbols_fts` (FTS5)
- `embeddings`

## MCP Server

- Tools: `list_repos`, `query`, `context`, `impact`, `subgraph`, `graph-query`, `detect-changes`, `impact-batch`, `dataflow`
- Resource Templates:
  - `codeatlas://repo/{repo}/status`
  - `codeatlas://repo/{repo}/clusters`
  - `codeatlas://repo/{repo}/processes`

## Known Limitations

- Vector search is exact brute-force cosine (ANN deferred; re-evaluate at 100K+ symbols).
- Embedding model is currently fixed to `all-MiniLM-L6-v2`.
- Ruby dynamic dispatch supports static `send(:sym)` / `send("str")` extraction.
- Ruby `method_missing` fallback routes unresolved implicit-self calls at confidence 0.30 (explicit-receiver cases become CALLS_UNRESOLVED).
- Dynamic/indirect call resolution in TSX/JSX is limited (unresolved calls now captured as CALLS_UNRESOLVED).
- Dataflow extraction is intra-function only; cross-function taint tracking deferred.
