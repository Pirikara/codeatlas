/**
 * CodeAtlas CLI wrapper.
 * Spawns the codeatlas binary and parses JSON output.
 */

import { execFile, spawn } from 'node:child_process';
import { promisify } from 'node:util';
import { existsSync } from 'node:fs';
import { readFile } from 'node:fs/promises';
import { resolve, resolve as resolvePath } from 'node:path';
import { homedir } from 'node:os';

// ─── Registry ──────────────────────────────────────────────────────

interface RegistryEntry {
  name: string;
  path: string;
  indexed_at: string;
}

function registryPath(): string {
  return process.env.CODEATLAS_REGISTRY_PATH
    ?? `${homedir()}/.codeatlas/registry.json`;
}

/** Read registry and filter entries whose index.db no longer exists. */
export async function readRegistry(): Promise<RegistryEntry[]> {
  const p = registryPath();
  if (!existsSync(p)) return [];
  try {
    const text = await readFile(p, 'utf-8');
    const entries: RegistryEntry[] = JSON.parse(text);
    return entries.filter(e => existsSync(`${e.path}/.codeatlas/index.db`));
  } catch {
    return [];
  }
}

/**
 * Resolve a repo param to an absolute path.
 *
 * Resolution order:
 * 1. resolve(param) して .codeatlas/index.db が存在する → 絶対パス扱い（後方互換）
 * 2. レジストリで name 完全一致 → path を返す
 * 3. レジストリで name 部分一致 → 1 件のみヒットなら使用、複数は候補付きエラー
 * 4. 未指定 → 登録 1 件なら自動選択、複数は候補付きエラー
 */
export async function resolveRepo(param?: string): Promise<string> {
  // Step 1: path として解釈を試みる（相対・絶対・UNC すべて対応）
  if (param) {
    const abs = resolvePath(param);
    if (existsSync(`${abs}/.codeatlas/index.db`)) {
      return abs;
    }
  }

  const entries = await readRegistry();
  const names = entries.map(e => e.name).join(', ');

  // Step 2 & 3: name 解決
  if (param) {
    const lower = param.toLowerCase();
    const exact = entries.filter(e => e.name === lower);
    if (exact.length === 1) return exact[0].path;
    if (exact.length > 1) {
      throw new Error(
        `Ambiguous repo name "${param}". Multiple entries found: ${exact.map(e => e.path).join(', ')}`
      );
    }
    const partial = entries.filter(e => e.name.includes(lower));
    if (partial.length === 1) return partial[0].path;
    if (partial.length > 1) {
      throw new Error(
        `Ambiguous repo "${param}". Matches: ${partial.map(e => e.name).join(', ')}. Use an exact name or absolute path.`
      );
    }
    throw new Error(
      `Repository "${param}" not found. Available: ${names || '(none — run `codeatlas index <path>` first)'}`
    );
  }

  // Step 4: param 省略
  if (entries.length === 1) return entries[0].path;
  if (entries.length === 0) {
    throw new Error('No indexed repositories. Run `codeatlas index <path>` first.');
  }
  throw new Error(
    `Multiple repositories indexed. Specify "repo" parameter. Available: ${names}`
  );
}

/** List all registered repos (for list_repos tool). */
export async function listRepos(): Promise<RegistryEntry[]> {
  return readRegistry();
}

const execFileAsync = promisify(execFile);

/** Resolve the codeatlas binary path. */
function findBinary(): string {
  // 1. Environment variable override
  if (process.env.CODEATLAS_BIN) {
    return process.env.CODEATLAS_BIN;
  }

  // 2. Sibling to this package (monorepo layout)
  const adjacent = resolve(import.meta.dirname, '..', '..', 'target', 'release', 'codeatlas');
  if (existsSync(adjacent)) {
    return adjacent;
  }

  // 3. Fallback to PATH
  return 'codeatlas';
}

const BINARY = findBinary();

/** Run a codeatlas CLI command and return parsed JSON. */
export async function run(args: string[]): Promise<any> {
  const { stdout, stderr } = await execFileAsync(BINARY, [...args, '--json'], {
    timeout: 30_000,
    maxBuffer: 10 * 1024 * 1024,
  });
  if (stderr) {
    console.error(`[codeatlas stderr] ${stderr.trim()}`);
  }
  const text = stdout.trim();
  if (!text || text === 'null') return null;
  return JSON.parse(text);
}

/** Get index status for a repository path. */
export async function status(repoPath: string): Promise<any> {
  return run(['status', repoPath]);
}

/** Search symbols by keyword. */
export async function query(term: string, repoPath: string, limit = 20, grouped = false): Promise<any> {
  const args = ['query', term, '-p', repoPath, '-l', String(limit)];
  if (grouped) args.push('--grouped');
  return run(args);
}

/** Get 360-degree context for a symbol. */
export async function context(
  repoPath: string,
  name?: string,
  uid?: string,
  file?: string,
): Promise<any> {
  const args = ['context', '-p', repoPath];
  if (uid) {
    args.push('--uid', uid);
  } else if (name) {
    args.push(name);
  }
  if (file) args.push('--file', file);
  return run(args);
}

/** Analyze blast radius of changing a symbol. */
export async function impact(
  name: string,
  repoPath: string,
  direction = 'upstream',
  depth = 3,
  minConfidence = 0.5,
): Promise<any> {
  return run([
    'impact', name,
    '-p', repoPath,
    '-d', direction,
    '--depth', String(depth),
    '--min-confidence', String(minConfidence),
  ]);
}

/** Get reachable subgraph from a symbol (nodes + edges). */
export async function subgraph(
  repoPath: string,
  direction = 'outgoing',
  depth = 3,
  edgeTypes: string[] = [],
  maxNodes = 100,
  maxEdges = 500,
  name?: string,
  uid?: string,
  id?: number,
): Promise<any> {
  const args = ['subgraph'];
  if (id !== undefined) {
    args.push('--id', String(id));
  } else if (uid) {
    args.push('--uid', uid);
  } else if (name) {
    args.push(name);
  }
  args.push(
    '-p', repoPath,
    '--direction', direction,
    '--depth', String(depth),
    '--max-nodes', String(maxNodes),
    '--max-edges', String(maxEdges),
  );
  if (edgeTypes.length > 0) args.push('--edge-types', edgeTypes.join(','));
  return run(args);
}

// ─── P4: impact-batch (VCS-independent) ────────────────────────────

/** Parse a unified diff string into file ranges (old + new). */
function parseDiffToRanges(diff: string): Array<{ file: string; start: number; end: number }> {
  const ranges: Array<{ file: string; start: number; end: number }> = [];
  // oldFile / newFile track each file pair independently so rename + edit
  // correctly attributes old ranges to the old path and new ranges to the new path.
  let oldFile: string | null = null;
  let newFile: string | null = null;
  let oldIsNull = false;
  let newIsNull = false;

  const parseRange = (s: string, isNull: boolean): [number, number] => {
    if (isNull) return [0, 0];
    const [startStr, countStr] = s.split(',');
    const start = parseInt(startStr, 10) || 1;
    const count = countStr !== undefined ? parseInt(countStr, 10) : 1;
    return [start, count];
  };

  for (const line of diff.split('\n')) {
    if (line.startsWith('--- ')) {
      const path = line.slice(4);
      oldIsNull = path === '/dev/null';
      oldFile = oldIsNull ? null : (path.startsWith('a/') ? path.slice(2) : path);
    } else if (line.startsWith('+++ ')) {
      const path = line.slice(4);
      newIsNull = path === '/dev/null';
      newFile = newIsNull ? null : (path.startsWith('b/') ? path.slice(2) : path);
    } else if (line.startsWith('@@ ') && (oldFile || newFile)) {
      // @@ -old_start[,old_count] +new_start[,new_count] @@
      const inner = line.slice(3).split(' @@')[0];
      const parts = inner.trim().split(/\s+/);
      if (parts.length < 2) continue;

      const [oldStart, oldCount] = parseRange(parts[0].slice(1), oldIsNull);
      const [newStart, newCount] = parseRange(parts[1].slice(1), newIsNull);

      // Old ranges → old file path (rename-safe: old path may differ from new path)
      if (oldCount > 0 && oldFile) {
        ranges.push({ file: oldFile, start: oldStart, end: oldStart + oldCount - 1 });
      }
      // New ranges → new file path
      if (newCount > 0 && newFile) {
        ranges.push({ file: newFile, start: newStart, end: newStart + newCount - 1 });
      }
    }
  }

  return ranges;
}

/** Spawn the codeatlas binary, pass JSON via CLI args, return parsed output. */
export async function impactBatch(
  repoPath: string,
  input: { symbols?: any[]; ranges?: any[] },
  direction = 'upstream',
  depth = 3,
  minConfidence = 0.5,
  callsOnly = false,
  maxSymbols = 20,
  kinds: string[] = ['Function', 'Method'],
): Promise<any> {
  const args = [
    'impact-batch', '-p', repoPath,
    '--direction', direction,
    '--depth', String(depth),
    '--min-confidence', String(minConfidence),
    '--max-symbols', String(maxSymbols),
  ];
  if (input.symbols && input.symbols.length > 0) {
    args.push('--symbols', JSON.stringify(input.symbols));
  }
  if (input.ranges && input.ranges.length > 0) {
    args.push('--ranges', JSON.stringify(input.ranges));
  }
  if (callsOnly) args.push('--calls-only');
  if (kinds.length === 0) {
    args.push('--all-kinds');
  } else {
    args.push('--kinds', kinds.join(','));
  }
  return run(args);
}

/**
 * Find symbols changed in a git diff and analyze their upstream/downstream impact.
 * Git diff runs on the MCP (Node.js) side; Core receives file ranges only.
 */
export async function detectChanges(
  repoPath: string,
  base = 'HEAD',
  head?: string,
  direction = 'upstream',
  depth = 3,
  minConfidence = 0.5,
  callsOnly = false,
  maxSymbols = 20,
  kinds: string[] = ['Function', 'Method'],
): Promise<any> {
  // 1. Run git diff on the MCP side
  const gitArgs = ['-C', repoPath, 'diff', '--unified=0', base];
  if (head) gitArgs.push(head);
  const { stdout: diffText } = await execFileAsync('git', gitArgs, {
    timeout: 30_000,
    maxBuffer: 10 * 1024 * 1024,
  });

  if (!diffText.trim()) {
    return { results: [], total: 0, truncated: false };
  }

  // 2. Parse diff into file ranges (no VCS concepts reach Core)
  const ranges = parseDiffToRanges(diffText);
  if (ranges.length === 0) {
    return { results: [], total: 0, truncated: false };
  }

  // 3. Delegate to impact-batch
  return impactBatch(repoPath, { ranges }, direction, depth, minConfidence, callsOnly, maxSymbols, kinds);
}

/** Execute a read-only SQL SELECT query against the knowledge graph. */
export async function graphQuery(
  repoPath: string,
  query: string,
  limit: number = 200,
): Promise<any> {
  return run(['graph-query', query, '-p', repoPath, '--limit', String(limit)]);
}

/** List communities/clusters. */
export async function clusters(repoPath: string): Promise<any> {
  return run(['clusters', repoPath]);
}

/** List execution flows/processes. */
export async function processes(repoPath: string): Promise<any> {
  return run(['processes', repoPath]);
}
