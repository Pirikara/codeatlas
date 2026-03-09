# CodeAtlas

A Rust-based code knowledge graph builder. It analyzes source code and stores symbols (functions/classes/methods, etc.) and relationships (calls/imports/inheritance, etc.) in SQLite.

## Features

- Supported languages: TypeScript / TSX / JavaScript / JSX, Go, Ruby
- Relationship kinds: `CALLS`, `IMPORTS`, `EXTENDS`, `IMPLEMENTS`, `DEFINES`, `CONTAINS`
- Search:
  - BM25 (FTS5)
  - Vector (fastembed + cosine)
  - Hybrid (BM25 + Vector + RRF)
  - Process-grouped (`--grouped`, `processes` / `definitions` / `total`)
- Subgraph exploration:
  - `direction` (outgoing/incoming/both)
  - `edge_types` / `max_depth` / `max_nodes` / `max_edges`
- Read-only graph querying:
  - `graph-query "<SQL>"` (`SELECT` / `WITH` only)
  - `--limit` to cap returned rows
- Diff impact analysis (P4: VCS-independent Core API):
  - `impact-batch` (input: symbol ID / name+file / file range; VCS-independent)
  - MCP tool `detect-changes` runs `git diff` on the Node.js side, then passes ranges to `impact-batch`
  - Defaults to `Function` / `Method` only (`--kinds`, `--all-kinds` available)
- MCP multi-repo operations (P5.5):
  - On successful `codeatlas index`, repos are auto-registered in `~/.codeatlas/registry.json`
  - MCP `list_repos` returns registered repositories
  - MCP `repo` accepts both name and absolute path; omitted `repo` auto-selects when exactly one repo is registered
- Incremental indexing:
  - Change detection via `file_index.content_hash`
  - Automatic cleanup of deleted files
  - Full re-analysis on deletions to preserve consistency
- Deterministic outputs:
  - Stable ordering for context/impact/query and graph traversal outputs
- Index metrics:
  - `index --metrics` prints per-phase timing, parse failures, counts, and RSS
- Impact analysis:
  - Full relationship traversal + `--calls-only` mode
  - Returns `risk` / `summary` / `affected_processes` / `affected_modules`
- MCP integration (`codeatlas-mcp/`)

## Installation

```bash
cargo build --release

# Optional: put binary in PATH
cp target/release/codeatlas /usr/local/bin/
```

## Usage

### 1) Index a repository

```bash
codeatlas index /path/to/repo
codeatlas index --force /path/to/repo
codeatlas index --force --metrics /path/to/repo
```

### 2) Generate embeddings

```bash
codeatlas embed /path/to/repo
codeatlas embed /path/to/repo --force
```

### 3) Search

```bash
# BM25 (default)
codeatlas query "handleRequest" -p /path/to/repo -l 20

# Vector / Hybrid
codeatlas query "handleRequest" -p /path/to/repo --mode vector
codeatlas query "handleRequest" -p /path/to/repo --mode hybrid

# Process-grouped (organized by execution flow)
codeatlas query "handleRequest" -p /path/to/repo --grouped
# NOTE: --grouped and --mode are mutually exclusive
```

### 4) Context and impact

```bash
codeatlas context "UserService" -p /path/to/repo
codeatlas context --uid "Function:cmd/root.go:Execute:11" -p /path/to/repo

# All relationships (default)
codeatlas impact "validateUser" -p /path/to/repo --direction upstream --depth 3 --min-confidence 0.5

# CALLS only
codeatlas impact "validateUser" -p /path/to/repo --calls-only
```

### 5) Subgraph exploration

```bash
codeatlas subgraph "Execute" -p /path/to/repo --direction outgoing --depth 3
codeatlas subgraph "Execute" -p /path/to/repo --direction both --edge-types CALLS,IMPORTS
codeatlas subgraph "Execute" -p /path/to/repo --max-nodes 100 --max-edges 500

# Direct start node by uid / id (no ambiguity)
codeatlas subgraph --uid "Function:cmd/root.go:Execute:11" -p /path/to/repo
codeatlas subgraph --id 42 -p /path/to/repo
```

### 6) Read-only graph query

```bash
# Basic SELECT
codeatlas graph-query "SELECT name, kind FROM symbols WHERE kind='Function' LIMIT 20" -p /path/to/repo

# CTE (WITH ... SELECT)
codeatlas graph-query "WITH f AS (SELECT id, name FROM symbols WHERE kind='Function') SELECT name FROM f LIMIT 10" -p /path/to/repo
```

### 7) Batch impact analysis (VCS-independent)

```bash
# Symbol input by name+file
codeatlas impact-batch -p /path/to/repo \
  --symbols '[{"name":"Execute","file":"cmd/root.go"}]'

# Input by file line ranges (equivalent to git diff hunks)
codeatlas impact-batch -p /path/to/repo \
  --ranges '[{"file":"cmd/root.go","start":1,"end":30}]'

# Include all symbol kinds
codeatlas impact-batch -p /path/to/repo \
  --ranges '[{"file":"cmd/root.go","start":1,"end":30}]' --all-kinds
```

### 8) Communities and execution flows

```bash
codeatlas clusters /path/to/repo
codeatlas processes /path/to/repo
```

### JSON output

`--json` is available for all commands.

```bash
codeatlas status --json /path/to/repo
codeatlas query "parse" -p /path/to/repo --mode hybrid --json
codeatlas query "Execute" -p /path/to/repo --grouped --json
codeatlas graph-query "SELECT name FROM symbols LIMIT 3" -p /path/to/repo --json
codeatlas subgraph "Execute" -p /path/to/repo --direction both --json
codeatlas impact-batch -p /path/to/repo --ranges '[{"file":"main.go","start":1,"end":50}]' --json
```

### Search evaluation (including grouped)

```bash
# Existing: bm25 / vector / hybrid
codeatlas eval testdata/eval/ts-webapp.json --mode bm25 -k 5

# Grouped evaluation
codeatlas eval testdata/eval/go-cli.json --grouped -k 5

# Grouped quality gate (routing_accuracy)
codeatlas eval testdata/eval/go-cli.json --grouped -k 5 --min-process-hit 0.7
```

## Where analysis artifacts are stored

```text
/path/to/repo/.codeatlas/index.db
```

It is recommended to ignore `.codeatlas/` in VCS.

Global registry (for MCP multi-repo operation):

```text
~/.codeatlas/registry.json
```

In tests/CI, you can override it with `CODEATLAS_REGISTRY_PATH`.

## MCP server (`codeatlas-mcp/`)

### Setup

```bash
cd codeatlas-mcp
npm install
npm run build
```

### Example registration in Claude Code

`~/.claude/settings.json`:

```json
{
  "mcpServers": {
    "codeatlas": {
      "command": "node",
      "args": ["/path/to/codeatlas/codeatlas-mcp/dist/index.js"],
      "env": {
        "CODEATLAS_BIN": "/path/to/codeatlas/target/release/codeatlas"
      }
    }
  }
}
```

### MCP tools / resources

- Tools: `list_repos`, `query`, `context`, `impact`, `subgraph`, `graph-query`, `detect-changes`, `impact-batch`
  - `detect-changes`: runs `git diff` on MCP side and passes ranges to Core `impact-batch`
  - `repo` accepts both name and absolute path (if omitted, auto-select/error depends on registry size)
- Resource Templates:
  - `codeatlas://repo/{repo}/status`
  - `codeatlas://repo/{repo}/clusters`
  - `codeatlas://repo/{repo}/processes`

## Architecture (high-level)

```text
src/
├── main.rs
├── scanner/mod.rs
├── parser/
│   ├── mod.rs
│   ├── extract/{mod,typescript,go,ruby}.rs
│   ├── calls/{mod,typescript,go,ruby}.rs
│   └── imports/{mod,typescript,go,ruby}.rs
├── analyzer/
│   ├── mod.rs
│   ├── resolver/{mod,calls,imports,inheritance}.rs
│   ├── community.rs
│   └── process.rs
├── storage/{mod,read,write}.rs
├── embedder/mod.rs
├── query/mod.rs
└── cli/{mod,index,metrics,status,embed_cmd,query_cmd,context_cmd,impact_cmd,subgraph_cmd,graph_query_cmd,impact_batch_cmd,clusters_cmd,processes_cmd,eval_cmd}.rs
```

## Known limitations

- Vector search is brute-force (ANN deferred; re-evaluate at 100K+ symbols).
- Embedding model is currently fixed to `all-MiniLM-L6-v2`.
- `impact` defaults to full relationship traversal. Use `--calls-only` for call-only analysis.
- Dynamic/indirect calls in TSX/JSX are not fully covered.
- Ruby `send(:sym)` / `send("str")` static extraction is supported.
- Ruby `method_missing` fallback routes unresolved implicit-self calls at confidence 0.30 (explicit-receiver cases remain unresolved).

## License

MIT
