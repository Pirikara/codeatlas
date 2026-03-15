# CodeAtlas

A Rust-based code knowledge graph builder. It analyzes source code and stores symbols (functions/classes/methods, etc.) and relationships (calls/imports/inheritance, etc.) in SQLite. Designed to give AI tools structural context about codebases via MCP.

## Features

- Supported languages: TypeScript / TSX / JavaScript / JSX, Go, Ruby
- Relationship kinds: `CALLS`, `CALLS_UNRESOLVED`, `CALLS_EXTERNAL`, `IMPORTS`, `EXTENDS`, `IMPLEMENTS`, `DEFINES`, `CONTAINS`
- Search: BM25 (FTS5), Vector (fastembed + cosine), Hybrid (BM25 + Vector + RRF), Process-grouped
- Subgraph exploration with direction/depth/edge type filters
- Read-only SQL graph querying
- Impact analysis (single symbol + batch + git diff)
- Intra-function data flow tracking
- Community detection and execution flow discovery
- MCP server for AI tool integration
- Incremental indexing with content hash change detection

## Installation

```bash
cargo build --release

# Optional: put binary in PATH
cp target/release/codeatlas /usr/local/bin/
```

## Usage with Response Examples

All examples below use `--json` output. Every command supports `--json`.

### 1) Index a repository

```bash
codeatlas index /path/to/repo
codeatlas index --force /path/to/repo          # full re-index
codeatlas index --force --metrics /path/to/repo # with timing info
```

Output (human-readable, without `--json`):

```
Indexing: /path/to/repo
7 file(s) to index (out of 7 total)
Extracted 36 symbols from 7 files
  18 external pseudo-symbols collected
  Stored 78 relationships (out of 79 candidates)
  Stored 45 data flows

Done in 0.0s — 54 symbols, 79 relationships, 4 communities, 1 flows
```

### 2) Status

```bash
codeatlas status /path/to/repo --json
```

```json
{
  "symbol_count": 62,
  "relationship_count": 78,
  "file_count": 7,
  "community_count": 4,
  "process_count": 1,
  "last_indexed": "1773508267"
}
```

### 3) Search (`query`)

Search symbols by keyword. Returns matched symbols ranked by BM25 score.

```bash
codeatlas query "createUser" -p /path/to/repo --json
```

```json
[
  {
    "symbol": {
      "uid": "Method:src/handler.go:CreateUser:12",
      "name": "CreateUser",
      "kind": "Method",
      "file_path": "src/handler.go",
      "start_line": 12,
      "end_line": 17,
      "is_exported": true,
      "parent_name": "Handler"
    },
    "score": 3.12
  },
  {
    "symbol": {
      "uid": "Method:src/user_service.ts:createUser:7",
      "name": "createUser",
      "kind": "Method",
      "file_path": "src/user_service.ts",
      "start_line": 7,
      "end_line": 10,
      "is_exported": false,
      "parent_name": "UserService"
    },
    "score": 2.91
  }
]
```

#### Process-grouped search (`--grouped`)

Organizes results by execution flow. Symbols not in any process go to `definitions`.

```bash
codeatlas query "User" -p /path/to/repo --grouped --json
```

```json
{
  "processes": [
    {
      "id": 0,
      "label": "findUser → find_by_id",
      "process_type": "cross_community",
      "matched_symbols": [
        {
          "symbol": {
            "uid": "Method:src/user.rb:find_by_id:23",
            "name": "find_by_id",
            "kind": "Method",
            "file_path": "src/user.rb",
            "start_line": 23,
            "end_line": 25,
            "is_exported": true,
            "parent_name": "User"
          },
          "score": 0.48,
          "step_index": 3
        }
      ]
    }
  ],
  "definitions": [
    {
      "symbol": {
        "uid": "Class:src/user.rb:User:11",
        "name": "User",
        "kind": "Class",
        "file_path": "src/user.rb",
        "start_line": 11,
        "end_line": 26,
        "is_exported": true,
        "parent_name": null
      },
      "score": 0.55
    }
  ],
  "total": 10
}
```

Other search modes:

```bash
codeatlas query "handleRequest" -p /path/to/repo --mode vector  # vector search
codeatlas query "handleRequest" -p /path/to/repo --mode hybrid  # BM25 + vector + RRF
# NOTE: --grouped and --mode are mutually exclusive
```

### 4) Context (`context`)

Get 360-degree relationship context for a symbol: what calls it (incoming) and what it calls (outgoing).

```bash
codeatlas context createUser -p /path/to/repo --json
```

```json
{
  "status": "found",
  "symbol": {
    "uid": "Method:src/user_service.ts:createUser:7",
    "name": "createUser",
    "kind": "Method",
    "file_path": "src/user_service.ts",
    "start_line": 7,
    "end_line": 10,
    "is_exported": false,
    "parent_name": "UserService"
  },
  "incoming": [
    {
      "symbol": {
        "uid": "File:src/user_service.ts",
        "name": "src/user_service.ts",
        "kind": "File"
      },
      "kind": "DEFINES",
      "confidence": 1.0,
      "reason": "file-defines-symbol"
    }
  ],
  "outgoing": [
    {
      "symbol": {
        "uid": "Method:src/user_repository.ts:save:9",
        "name": "save",
        "kind": "Method",
        "file_path": "src/user_repository.ts",
        "parent_name": "UserRepository"
      },
      "kind": "CALLS",
      "confidence": 0.9,
      "reason": "field-type-annotation"
    },
    {
      "symbol": {
        "uid": "Function:src/utils.ts:hashPassword:1",
        "name": "hashPassword",
        "kind": "Function",
        "file_path": "src/utils.ts"
      },
      "kind": "CALLS",
      "confidence": 0.9,
      "reason": "imported-file-match"
    }
  ]
}
```

If the name is ambiguous (multiple symbols match), the response returns candidates:

```json
{
  "status": "ambiguous",
  "message": "Multiple symbols match 'initialize'. Use uid for exact lookup.",
  "candidates": [
    { "uid": "Constructor:src/app.rb:initialize:3", "name": "initialize", "file_path": "src/app.rb" },
    { "uid": "Constructor:src/user.rb:initialize:14", "name": "initialize", "file_path": "src/user.rb" }
  ]
}
```

Use `--uid` for exact lookup:

```bash
codeatlas context --uid "Constructor:src/user.rb:initialize:14" -p /path/to/repo --json
```

### 5) Impact analysis (`impact`)

Estimate the blast radius of changing a symbol. Traverses upstream (who calls me?) or downstream (what do I call?) relationships.

```bash
codeatlas impact createUser -p /path/to/repo --direction upstream --json
```

```json
{
  "target": {
    "uid": "Method:src/user_service.ts:createUser:7",
    "name": "createUser",
    "kind": "Method",
    "file_path": "src/user_service.ts",
    "parent_name": "UserService"
  },
  "direction": "upstream",
  "by_depth": {
    "1": [
      {
        "symbol": {
          "uid": "File:src/user_service.ts",
          "name": "src/user_service.ts",
          "kind": "File"
        },
        "confidence": 1.0,
        "reason": "file-defines-symbol"
      }
    ],
    "2": [
      {
        "symbol": {
          "uid": "File:src/admin.ts",
          "name": "src/admin.ts",
          "kind": "File"
        },
        "confidence": 1.0,
        "reason": "import-resolved"
      }
    ]
  },
  "total_affected": 3,
  "risk": "medium",
  "summary": "Changing createUser (upstream) affects 3 symbol(s) (1 direct). Risk: medium.",
  "affected_processes": [],
  "affected_modules": []
}
```

`risk` levels: `low` / `medium` / `high` — based on affected count and relationship types.

```bash
codeatlas impact createUser -p /path/to/repo --calls-only  # CALLS edges only
```

### 6) Subgraph exploration (`subgraph`)

Extract a bounded reachable subgraph (nodes + edges) from a starting symbol.

```bash
codeatlas subgraph createUser -p /path/to/repo --direction outgoing --depth 2 --json
```

```json
{
  "start_id": 33,
  "nodes": [
    {
      "id": 33,
      "uid": "Method:src/user_service.ts:createUser:7",
      "name": "createUser",
      "kind": "Method",
      "file_path": "src/user_service.ts",
      "parent_name": "UserService"
    },
    {
      "id": 28,
      "uid": "Method:src/user_repository.ts:save:9",
      "name": "save",
      "kind": "Method",
      "file_path": "src/user_repository.ts",
      "parent_name": "UserRepository"
    },
    {
      "id": 35,
      "uid": "Function:src/utils.ts:hashPassword:1",
      "name": "hashPassword",
      "kind": "Function",
      "file_path": "src/utils.ts"
    },
    {
      "id": 46,
      "uid": "External:<external>:password.split(\"\").reverse().join:0",
      "name": "password.split(\"\").reverse().join",
      "kind": "External",
      "file_path": "<external>"
    }
  ],
  "edges": [
    {
      "source_id": 33,
      "target_id": 28,
      "kind": "CALLS",
      "confidence": 0.9,
      "reason": "field-type-annotation"
    },
    {
      "source_id": 33,
      "target_id": 35,
      "kind": "CALLS",
      "confidence": 0.9,
      "reason": "imported-file-match"
    },
    {
      "source_id": 35,
      "target_id": 46,
      "kind": "CALLS_UNRESOLVED",
      "confidence": 0.0,
      "reason": "unresolved"
    }
  ],
  "node_count": 7,
  "edge_count": 6,
  "truncated": false,
  "truncated_reason": null
}
```

```bash
codeatlas subgraph createUser -p /path/to/repo --edge-types CALLS  # filter by edge kind
codeatlas subgraph --uid "Method:src/user_service.ts:createUser:7" -p /path/to/repo  # exact start
codeatlas subgraph --id 33 -p /path/to/repo  # by integer ID
```

### 7) Data flow (`dataflow`)

Show intra-function data flows: how data moves through assignments, arguments, returns, string interpolation, and field access within a function body.

```bash
codeatlas dataflow createUser -p /path/to/repo --json
```

```json
{
  "symbol": {
    "uid": "Method:src/user_service.ts:createUser:7",
    "name": "createUser",
    "kind": "Method",
    "file_path": "src/user_service.ts",
    "start_line": 7,
    "end_line": 10,
    "parent_name": "UserService"
  },
  "flows": [
    {
      "source_expr": "hashPassword(password)",
      "sink_expr": "hashed",
      "flow_kind": "Assignment",
      "source_line": 8,
      "sink_line": 8
    },
    {
      "source_expr": "password",
      "sink_expr": "hashPassword[arg1]",
      "flow_kind": "Argument",
      "source_line": 8,
      "sink_line": 8
    },
    {
      "source_expr": "{ name, password: hashed }",
      "sink_expr": "this.repo.save[arg1]",
      "flow_kind": "Argument",
      "source_line": 9,
      "sink_line": 9
    }
  ]
}
```

Flow kinds: `Assignment`, `Argument`, `StringInterp`, `Return`, `FieldAccess`.

### 8) Read-only graph query (`graph-query`)

Execute arbitrary read-only SQL against the knowledge graph. Only `SELECT` and `WITH ... SELECT` are allowed.

```bash
codeatlas graph-query "SELECT name, kind, file_path FROM symbols WHERE kind='Function' LIMIT 5" -p /path/to/repo --json
```

```json
[
  { "name": "NewHandler", "kind": "Function", "file_path": "src/handler.go" },
  { "name": "hashPassword", "kind": "Function", "file_path": "src/utils.ts" },
  { "name": "validateEmail", "kind": "Function", "file_path": "src/utils.ts" }
]
```

Available tables: `symbols`, `relationships`, `data_flows`, `communities`, `community_members`, `processes`, `process_steps`, `file_index`, `symbols_fts`, `embeddings`.

### 9) Batch impact analysis (`impact-batch`)

VCS-independent impact analysis. Accepts symbol IDs, name+file pairs, or file line ranges.

```bash
# By name+file
codeatlas impact-batch -p /path/to/repo \
  --symbols '[{"name":"createUser","file":"src/user_service.ts"}]' --json

# By file line ranges (equivalent to git diff hunks)
codeatlas impact-batch -p /path/to/repo \
  --ranges '[{"file":"src/user_service.ts","start":7,"end":10}]' --json

# Include all symbol kinds (default: Function/Method only)
codeatlas impact-batch -p /path/to/repo \
  --ranges '[{"file":"src/handler.go","start":1,"end":30}]' --all-kinds --json
```

### 10) Communities and execution flows

```bash
codeatlas clusters /path/to/repo --json
```

```json
[
  {
    "id": 0,
    "label": "CreateFull",
    "cohesion": 0.67,
    "symbol_count": 3,
    "top_symbols": ["Method create", "Method full_name", "Method run"]
  },
  {
    "id": 1,
    "label": "Find",
    "cohesion": 1.0,
    "symbol_count": 2,
    "top_symbols": ["Method find", "Method find_by_id"]
  }
]
```

```bash
codeatlas processes /path/to/repo --json
```

```json
[
  {
    "id": 0,
    "label": "findUser → find_by_id",
    "process_type": "cross_community",
    "priority": 1.0,
    "step_count": 4,
    "steps": [
      { "name": "findUser",  "kind": "Method", "file_path": "src/user_service.ts",  "step_index": 0 },
      { "name": "findById",  "kind": "Method", "file_path": "src/user_repository.ts", "step_index": 1 },
      { "name": "find",      "kind": "Method", "file_path": "src/user.rb",           "step_index": 2 },
      { "name": "find_by_id","kind": "Method", "file_path": "src/user.rb",           "step_index": 3 }
    ]
  }
]
```

### 11) Generate embeddings

```bash
codeatlas embed /path/to/repo
codeatlas embed /path/to/repo --force
```

### 12) Search evaluation

```bash
codeatlas eval testdata/eval/ts-webapp.json --mode bm25 -k 5
codeatlas eval testdata/eval/go-cli.json --grouped -k 5
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

### MCP tools

| Tool | Description |
|---|---|
| `list_repos` | List indexed repositories (name, path, indexed_at) |
| `query` | Search symbols by keyword (BM25, supports `--grouped`) |
| `context` | Get incoming/outgoing relationships for a symbol |
| `impact` | Estimate blast radius of changing a symbol |
| `subgraph` | Extract reachable subgraph (nodes + edges) |
| `dataflow` | Show intra-function data flows |
| `graph-query` | Run read-only SQL against the knowledge graph |
| `detect-changes` | Analyze impact of local git changes (runs `git diff` on MCP side) |
| `impact-batch` | VCS-independent batch impact analysis |

All tools accept an optional `repo` parameter (name or absolute path). When omitted:
- 1 repo indexed: auto-select
- 0 repos: error prompting to run `codeatlas index`
- 2+ repos: error listing available names

### MCP resource templates

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
└── cli/{mod,index,metrics,status,embed_cmd,query_cmd,context_cmd,
         impact_cmd,subgraph_cmd,graph_query_cmd,impact_batch_cmd,
         clusters_cmd,processes_cmd,eval_cmd,dataflow_cmd}.rs

codeatlas-mcp/
├── src/{index,tools,resources,codeatlas}.ts
└── dist/
```

## Known limitations

- Vector search is brute-force (ANN deferred; re-evaluate at 100K+ symbols).
- Embedding model is currently fixed to `all-MiniLM-L6-v2`.
- `impact` defaults to full relationship traversal. Use `--calls-only` for call-only analysis.
- Dynamic/indirect calls in TSX/JSX are not fully covered (captured as `CALLS_UNRESOLVED`).
- Dataflow extraction is intra-function only; cross-function taint tracking is deferred.
- Ruby `send(:sym)` / `send("str")` static extraction is supported.
- Ruby `method_missing` fallback routes unresolved implicit-self calls at confidence 0.30.

## License

MIT
