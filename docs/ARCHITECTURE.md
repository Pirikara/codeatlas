# CodeAtlas Architecture

## System Overview

```text
┌─────────────┐         ┌──────────────┐         ┌─────────────┐
│ codeatlas   │         │ codeatlas-mcp │         │ AI Clients   │
│ CLI (Rust)  │◄───────►│ (TypeScript)  │◄───────►│ Claude/Cursor│
└──────┬──────┘  JSON   └──────┬───────┘  MCP     └─────────────┘
       │                       │
       ▼                       ▼
┌──────────────────────────────────────────────────────────┐
│ Scan → Parse → Resolve → Community/Flow → SQLite        │
│                ↘ Embed (fastembed) ↗                    │
└─────────────────────────┬────────────────────────────────┘
                          ▼
                .codeatlas/index.db
```

## Pipeline

1. `scanner::scan`
- Respects `.gitignore`
- Target extensions: `.ts`, `.tsx`, `.js`, `.jsx`, `.go`, `.rb`
- Computes `content_hash` (xxh3)

2. `storage cleanup` (pre-index)
- Diffs DB `file_index.path` vs current scan results
- Removes deleted files via `cleanup_deleted_files`
- Cleanup also runs on `--force`
- If deletions are found, all files are re-parsed for consistency

3. `parser::ParserPool::parse_full`
- Language-specific tree-sitter parsers
- Phase 5 parsing runs in parallel (`rayon` + thread-local `ParserPool`)
- Extracts:
  - symbols (`parser/extract/*`)
  - imports (`parser/imports/*`)
  - calls (`parser/calls/*`)

4. `analyzer::resolve_relationships`
- `IMPORTS`, `CALLS`, `EXTENDS`, `IMPLEMENTS`, `DEFINES`, `CONTAINS`
- Additional detection for Go implicit interface implementations

5. `analyzer::community::detect_communities`
- Louvain on CALLS graph

6. `analyzer::process::detect_processes`
- Entry scoring + BFS

7. `storage::*`
- Persists `symbols`, `relationships`, `communities`, `processes`, `file_index`
- Rebuilds `symbols_fts`

8. `metrics output` (optional, P8)
- `index --metrics` reports phase timings, parse failures, counts, and RSS

9. `registry update` (P5.5)
- On successful `index`, upserts repo into `~/.codeatlas/registry.json`
- Path/name collisions are auto-suffixed (`name-2`, `name-3`, ...)
- `CODEATLAS_REGISTRY_PATH` can override path (tests/CI)

10. `embedder + embeddings`
- `codeatlas embed` generates symbol embeddings
- Stores vectors as BLOB in `embeddings`
- Used by `query --mode vector|hybrid`

## Major Modules

### Scanner (`src/scanner/mod.rs`)
- Output: `Vec<FileInfo>`
- Main exclusions: `node_modules`, `.git`, `.codeatlas`, `target`, `dist`

### Parser (`src/parser/`)
- `extract/{typescript,go,ruby}.rs`
- `calls/{typescript,go,ruby}.rs`
- `imports/{typescript,go,ruby}.rs`

### Resolver (`src/analyzer/resolver/`)
- `imports.rs`: import path resolution
- `calls.rs`: multi-step call resolution (with confidence)
- `inheritance.rs`: extends/implements + Go implicit implements

### Storage (`src/storage/`)
- `mod.rs`: DB connection/schema
- `write.rs`: store/cleanup/upsert_embeddings
- `read.rs`: status/clusters/processes/embed target reads

### Query (`src/query/mod.rs`)
- `search` (BM25)
- `search_vector_only` (Vector; top-n heap scan to reduce memory)
- `search_hybrid` (RRF)
- `search_grouped` (process-grouped: `processes` / `definitions` / `total`)
- `context` (`found` / `ambiguous` / `not_found`, disambiguation with `uid` + `file`)
- `impact` (all relationships or `calls_only`; returns `risk` / `summary` / `affected_processes` / `affected_modules`)
- `subgraph` (reachable nodes + edges)
- `impact_by_id` / `symbol_by_name_file` (foundation for batch impact)

### CLI (`src/cli/`)
- `index`, `status`, `embed`, `query`, `eval`, `context`, `impact`, `subgraph`, `impact-batch`, `clusters`, `processes`
- `query --mode bm25|vector|hybrid`
- `query --grouped` (process-grouped output, mutually exclusive with `--mode`)
- `eval --grouped` (grouped evaluation with `process_recall` / `routing_accuracy`)
- `eval --min-process-hit` (grouped quality gate)
- `graph-query "<SQL>" --limit N` (read-only SQL exploration)
- `impact --calls-only`
- `impact-batch --symbols|--ranges` (VCS-independent)

## DB Schema Highlights

- `symbols`: core symbol table (`uid` unique)
- `relationships`: `source_id`, `target_id`, `kind`, `confidence`, `reason`
- `symbols_fts`: FTS5 (`name`, `file_path`, `kind`, `parent_name`)
- `file_index`: incremental indexing metadata (`content_hash`, `last_indexed`, `language`, `size_bytes`)
- `embeddings`: `symbol_id`, `model_id`, `dims`, `vector_blob`, `content_hash`, `updated_at`

## Responsibility Split: CLI vs MCP

- CLI: parsing, storage, and query core logic
- MCP: adapter layer that invokes CLI

MCP surface:
- Tools: `list_repos`, `query`, `context`, `impact`, `subgraph`, `graph-query`, `impact-batch`, `detect-changes`
- Resource Templates:
  - `codeatlas://repo/{repo}/status`
  - `codeatlas://repo/{repo}/clusters`
  - `codeatlas://repo/{repo}/processes`

MCP repo resolution rules:
- `repo` accepts name or absolute path
- If omitted and exactly one repo is registered, it auto-selects
- If 0 or multiple repos are registered, it returns an error with candidates

## Current Notes

- Vector search remains exact cosine scan (ANN deferred; re-evaluate at 100K+ symbols).
- `impact` defaults to all relationship kinds (`--calls-only` for CALLS-only).
- `graph-query` executes with read-only constraints (keyword validation + `stmt.readonly()` + read-only open flags).
- Core CLI does not include `detect-changes`; it provides VCS-independent `impact-batch`.
- MCP `detect-changes` runs `git diff` in Node.js and delegates extracted ranges to Core `impact-batch`.
- Context/impact/query outputs are deterministic for identical inputs.
