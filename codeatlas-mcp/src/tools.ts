/**
 * Tool catalog + dispatcher for the CodeAtlas MCP bridge.
 * Each tool maps to one or more `codeatlas` CLI subcommands.
 */

import * as codeatlas from './codeatlas.js';

export interface ToolDefinition {
  name: string;
  description: string;
  inputSchema: Record<string, any>;
}

export const TOOLS: ToolDefinition[] = [
  {
    name: 'list_repos',
    description: 'Return repositories currently registered by CodeAtlas indexing (name, path, indexed_at). Use this to pick a target repo before other calls.',
    inputSchema: {
      type: 'object',
      additionalProperties: false,
      properties: {},
      required: [],
    },
  },
  {
    name: 'query',
    description:
      'Search symbols by term using CodeAtlas index ranking. Returns matched symbols with score and location. Set grouped=true to organize results by execution process.',
    inputSchema: {
      type: 'object',
      properties: {
        term: { type: 'string', description: 'Search keyword (e.g., "handleRequest", "parse")' },
        repo: { type: 'string', description: 'Repository name or absolute path. Omit if only one repo is indexed. Use list_repos to see available names.' },
        limit: { type: 'number', description: 'Max results (default: 20)', default: 20 },
        grouped: {
          type: 'boolean',
          description: 'Return results grouped by execution process. Symbols not in any process go to "definitions".',
          default: false,
        },
      },
      required: ['term'],
    },
  },
  {
    name: 'context',
    description:
      'Get relationship context for one symbol: metadata, incoming refs, and outgoing refs. Ambiguous names return candidate symbols; use uid for exact lookup.',
    inputSchema: {
      type: 'object',
      properties: {
        name: { type: 'string', description: 'Symbol name to inspect' },
        uid:  { type: 'string', description: 'Direct UID from prior results (zero-ambiguity lookup)' },
        file: { type: 'string', description: 'File path to disambiguate same-name symbols' },
        repo: { type: 'string', description: 'Repository name or absolute path. Omit if only one repo is indexed. Use list_repos to see available names.' },
      },
      required: [],
    },
  },
  {
    name: 'impact',
    description:
      'Estimate change impact from a target symbol. Traverses upstream/downstream relations by depth and returns risk, summary, affected modules, and affected execution processes.',
    inputSchema: {
      type: 'object',
      properties: {
        name: { type: 'string', description: 'Symbol name to analyze' },
        repo: { type: 'string', description: 'Repository name or absolute path. Omit if only one repo is indexed. Use list_repos to see available names.' },
        direction: {
          type: 'string',
          description: 'upstream (who calls me?) or downstream (what do I call?)',
          enum: ['upstream', 'downstream'],
          default: 'upstream',
        },
        depth: { type: 'number', description: 'Max traversal depth (default: 3)', default: 3 },
        min_confidence: {
          type: 'number',
          description: 'Minimum confidence threshold (0-1, default: 0.5)',
          default: 0.5,
        },
      },
      required: ['name'],
    },
  },
  {
    name: 'subgraph',
    description:
      'Extract a bounded reachable subgraph (nodes + edges) from a starting symbol. Supports direction, depth, edge kind filters, and output caps.',
    inputSchema: {
      type: 'object',
      properties: {
        name: { type: 'string', description: 'Symbol name to start from' },
        uid:  { type: 'string', description: 'Symbol UID for zero-ambiguity lookup (alternative to name)' },
        id:   { type: 'number', description: 'Symbol integer ID for zero-ambiguity lookup (alternative to name)' },
        repo: { type: 'string', description: 'Repository name or absolute path. Omit if only one repo is indexed. Use list_repos to see available names.' },
        direction: {
          type: 'string',
          enum: ['outgoing', 'incoming', 'both'],
          description: 'outgoing (what I call), incoming (who calls me), or both',
          default: 'outgoing',
        },
        depth: { type: 'number', description: 'Max traversal depth (default: 3)', default: 3 },
        edge_types: {
          type: 'array',
          items: { type: 'string' },
          description: 'Edge kinds to follow (e.g. ["CALLS","IMPORTS"]). Empty = all.',
        },
        max_nodes: { type: 'number', description: 'Max nodes to return (default: 100)', default: 100 },
        max_edges: { type: 'number', description: 'Max edges to return (default: 500)', default: 500 },
      },
      required: [],
    },
  },
  {
    name: 'detect-changes',
    description:
      'Analyze impact of local git changes. Diff is computed in MCP (Node.js), converted to file ranges, then delegated to Core impact-batch for symbol-level impact.',
    inputSchema: {
      type: 'object',
      properties: {
        repo:           { type: 'string', description: 'Repository name or absolute path. Omit if only one repo is indexed. Use list_repos to see available names.' },
        base:           { type: 'string', description: 'Base git ref to diff against (default: HEAD)', default: 'HEAD' },
        head:           { type: 'string', description: 'Head git ref (default: working tree)' },
        direction:      { type: 'string', enum: ['upstream', 'downstream'], description: 'upstream (who calls me?) or downstream (what do I call?)', default: 'upstream' },
        depth:          { type: 'number', description: 'Max traversal depth (default: 3)', default: 3 },
        min_confidence: { type: 'number', description: 'Minimum confidence threshold (0-1, default: 0.5)', default: 0.5 },
        calls_only:     { type: 'boolean', description: 'Limit to CALLS relationships only (default: false)', default: false },
        max_symbols:    { type: 'number', description: 'Max changed symbols to return (default: 20)', default: 20 },
        kinds: {
          type: 'array',
          items: { type: 'string' },
          description: 'Symbol kinds to include (default: ["Function","Method"]; empty array = all kinds)',
          default: ['Function', 'Method'],
        },
      },
      required: [],
    },
  },
  {
    name: 'graph-query',
    description: `Run a read-only SQL query against the CodeAtlas graph index.
Only SELECT and WITH...SELECT (CTE) queries are allowed.

Schema (src/storage/mod.rs):
- symbols(id, uid, name, kind, file_path, start_line, end_line, is_exported, parent_name)
  kinds: Function, Method, Class, Interface, Struct, Variable, Constant, Type, Enum, File, Module
- relationships(id, source_id, target_id, kind, confidence, reason)
  kinds: CALLS, IMPORTS, EXTENDS, IMPLEMENTS, DEFINES, CONTAINS
- communities(id, label, cohesion, symbol_count)
- community_members(community_id, symbol_id)
- processes(id, label, process_type, priority, step_count)
- process_steps(process_id, symbol_id, step_index)   -- PK(process_id, step_index)
- file_index(path, content_hash, last_indexed, language, size_bytes)

Examples:
  "SELECT name, file_path FROM symbols WHERE kind='Function' AND is_exported=1 LIMIT 20"
  "SELECT t.name, t.file_path FROM relationships r JOIN symbols s ON r.source_id=s.id JOIN symbols t ON r.target_id=t.id WHERE s.name='Execute' AND r.kind='CALLS'"
  "SELECT s.name, c.label FROM symbols s JOIN community_members cm ON s.id=cm.symbol_id JOIN communities c ON cm.community_id=c.id LIMIT 10"`,
    inputSchema: {
      type: 'object',
      properties: {
        repo:  { type: 'string', description: 'Repository name or absolute path. Omit if only one repo is indexed. Use list_repos to see available names.' },
        query: { type: 'string', description: 'SQL SELECT query' },
        limit: { type: 'number', description: 'Max rows (default 200)', default: 200 },
      },
      required: ['query'],
    },
  },
  {
    name: 'impact-batch',
    description:
      'Run VCS-independent impact analysis for an explicit symbol/range set. Accepts symbol IDs, name+file pairs, and file line ranges; returns per-symbol impact results.',
    inputSchema: {
      type: 'object',
      properties: {
        repo: { type: 'string', description: 'Repository name or absolute path. Omit if only one repo is indexed. Use list_repos to see available names.' },
        symbols: {
          type: 'array',
          items: {
            type: 'object',
            description: '{"id": 123} or {"name": "Foo", "file": "path/to/file.go"}',
          },
          description: 'Symbol entries by id or name+file',
        },
        ranges: {
          type: 'array',
          items: {
            type: 'object',
            description: '{"file": "path/to/file.go", "start": 10, "end": 20}',
          },
          description: 'File line ranges to resolve symbols from',
        },
        direction:      { type: 'string', enum: ['upstream', 'downstream'], default: 'upstream' },
        depth:          { type: 'number', default: 3 },
        min_confidence: { type: 'number', default: 0.5 },
        calls_only:     { type: 'boolean', default: false },
        max_symbols:    { type: 'number', default: 20 },
        kinds: {
          type: 'array',
          items: { type: 'string' },
          description: 'Symbol kinds to include (default: ["Function","Method"]; empty array = all kinds)',
          default: ['Function', 'Method'],
        },
      },
      required: [],
    },
  },
];

/** Next-step hints to guide agents through a natural workflow. */
function getHint(toolName: string): string {
  switch (toolName) {
    case 'query':
      return '\n\n---\n**Next:** Use `--grouped` で process 別にシンボルを整理。変更対象がどのフローに影響するか把握してから impact を呼ぶと効率的。context({name: "<symbol_name>"}) で個別シンボルの詳細も確認できます。';
    case 'context':
      return '\n\n---\n**Next:** If status is "ambiguous", use context({uid: "<uid>"}) to select a specific symbol. If status is "found", use impact({name: "<symbol_name>", direction: "upstream"}) to check blast radius before changes.';
    case 'impact':
      return '\n\n---\n**Next:** Check risk field first. If "high", review affected_processes and affected_modules before making changes. Then review depth-1 items (WILL BREAK).';
    default:
      return '';
  }
}

/** Handle a tool call. */
export async function callTool(name: string, args: Record<string, any>): Promise<string> {
  if (name === 'list_repos') {
    const repos = await codeatlas.listRepos();
    return JSON.stringify(repos, null, 2);
  }

  // 全ツール共通: repo を解決してから処理
  const repoPath = await codeatlas.resolveRepo(args.repo);

  let result: any;
  switch (name) {
    case 'query':
      result = await codeatlas.query(args.term, repoPath, args.limit, args.grouped ?? false);
      break;
    case 'context':
      if (!args.name && !args.uid) throw new Error('context requires either name or uid');
      result = await codeatlas.context(repoPath, args.name, args.uid, args.file);
      break;
    case 'impact':
      result = await codeatlas.impact(args.name, repoPath, args.direction, args.depth, args.min_confidence);
      break;
    case 'subgraph':
      result = await codeatlas.subgraph(
        repoPath, args.direction, args.depth,
        args.edge_types ?? [], args.max_nodes, args.max_edges,
        args.name, args.uid, args.id,
      );
      break;
    case 'detect-changes':
      result = await codeatlas.detectChanges(
        repoPath, args.base, args.head, args.direction, args.depth,
        args.min_confidence, args.calls_only, args.max_symbols,
        args.kinds ?? ['Function', 'Method'],
      );
      break;
    case 'graph-query': {
      process.stderr.write(`[audit] graph-query repo=${repoPath} query=${JSON.stringify(args.query)}\n`);
      result = await codeatlas.graphQuery(repoPath, args.query, args.limit);
      break;
    }
    case 'impact-batch':
      result = await codeatlas.impactBatch(
        repoPath, { symbols: args.symbols, ranges: args.ranges },
        args.direction, args.depth, args.min_confidence, args.calls_only,
        args.max_symbols, args.kinds ?? ['Function', 'Method'],
      );
      break;
    default:
      throw new Error(`Unknown tool: ${name}`);
  }

  if (result === null) {
    return 'No results found.';
  }

  return JSON.stringify(result, null, 2) + getHint(name);
}
