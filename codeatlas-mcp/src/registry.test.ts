/**
 * Unit tests for resolveRepo(), readRegistry(), and listRepos().
 * Uses Node.js built-in test runner.
 * CODEATLAS_REGISTRY_PATH is pointed at a tmp file for each test.
 *
 * Run: node --test src/registry.test.ts
 * (requires Node >= 22 with type-stripping support)
 */

import { test } from 'node:test';
import assert from 'node:assert/strict';
import { writeFile, mkdir, rm } from 'node:fs/promises';
import { mkdtempSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { resolveRepo, readRegistry, listRepos } from './codeatlas.ts';

// registryPath() is evaluated on each call, so setting process.env before
// each test is sufficient — no need for dynamic/fresh imports.

/** Create a temporary directory with a fake index.db to simulate an indexed repo. */
async function makeFakeRepo(base: string, name: string): Promise<string> {
  const repoPath = join(base, name);
  await mkdir(join(repoPath, '.codeatlas'), { recursive: true });
  await writeFile(join(repoPath, '.codeatlas', 'index.db'), '');
  return repoPath;
}

/** Write a registry JSON file. */
async function writeRegistry(registryPath: string, entries: object[]): Promise<void> {
  await writeFile(registryPath, JSON.stringify(entries));
}

test('single entry: omitting repo auto-selects the only registered repo', async () => {
  const tmp = mkdtempSync(join(tmpdir(), 'codeatlas-test-'));
  const repoPath = await makeFakeRepo(tmp, 'myrepo');
  const regPath = join(tmp, 'registry.json');
  await writeRegistry(regPath, [{ name: 'myrepo', path: repoPath, indexed_at: '2026-01-01T00:00:00Z' }]);

  process.env.CODEATLAS_REGISTRY_PATH = regPath;
  try {
    const resolved = await resolveRepo(undefined);
    assert.equal(resolved, repoPath);
  } finally {
    delete process.env.CODEATLAS_REGISTRY_PATH;
    await rm(tmp, { recursive: true, force: true });
  }
});

test('multiple entries + exact name match → returns correct path', async () => {
  const tmp = mkdtempSync(join(tmpdir(), 'codeatlas-test-'));
  const repo1 = await makeFakeRepo(tmp, 'alpha');
  const repo2 = await makeFakeRepo(tmp, 'beta');
  const regPath = join(tmp, 'registry.json');
  await writeRegistry(regPath, [
    { name: 'alpha', path: repo1, indexed_at: '2026-01-01T00:00:00Z' },
    { name: 'beta',  path: repo2, indexed_at: '2026-01-01T00:00:00Z' },
  ]);

  process.env.CODEATLAS_REGISTRY_PATH = regPath;
  try {
    const resolved = await resolveRepo('beta');
    assert.equal(resolved, repo2);
  } finally {
    delete process.env.CODEATLAS_REGISTRY_PATH;
    await rm(tmp, { recursive: true, force: true });
  }
});

test('multiple entries + partial match with 1 hit → resolves correctly', async () => {
  const tmp = mkdtempSync(join(tmpdir(), 'codeatlas-test-'));
  const repo1 = await makeFakeRepo(tmp, 'go-cli');
  const repo2 = await makeFakeRepo(tmp, 'ts-webapp');
  const regPath = join(tmp, 'registry.json');
  await writeRegistry(regPath, [
    { name: 'go-cli',    path: repo1, indexed_at: '2026-01-01T00:00:00Z' },
    { name: 'ts-webapp', path: repo2, indexed_at: '2026-01-01T00:00:00Z' },
  ]);

  process.env.CODEATLAS_REGISTRY_PATH = regPath;
  try {
    const resolved = await resolveRepo('webapp');
    assert.equal(resolved, repo2);
  } finally {
    delete process.env.CODEATLAS_REGISTRY_PATH;
    await rm(tmp, { recursive: true, force: true });
  }
});

test('multiple entries + partial match with multiple hits → throws ambiguous error', async () => {
  const tmp = mkdtempSync(join(tmpdir(), 'codeatlas-test-'));
  const repo1 = await makeFakeRepo(tmp, 'my-app-frontend');
  const repo2 = await makeFakeRepo(tmp, 'my-app-backend');
  const regPath = join(tmp, 'registry.json');
  await writeRegistry(regPath, [
    { name: 'my-app-frontend', path: repo1, indexed_at: '2026-01-01T00:00:00Z' },
    { name: 'my-app-backend',  path: repo2, indexed_at: '2026-01-01T00:00:00Z' },
  ]);

  process.env.CODEATLAS_REGISTRY_PATH = regPath;
  try {
    await assert.rejects(
      () => resolveRepo('my-app'),
      (err: Error) => {
        assert.match(err.message, /Ambiguous/);
        assert.match(err.message, /my-app-frontend/);
        assert.match(err.message, /my-app-backend/);
        return true;
      }
    );
  } finally {
    delete process.env.CODEATLAS_REGISTRY_PATH;
    await rm(tmp, { recursive: true, force: true });
  }
});

test('absolute path with index.db → returns the path directly (backward-compat)', async () => {
  const tmp = mkdtempSync(join(tmpdir(), 'codeatlas-test-'));
  const repoPath = await makeFakeRepo(tmp, 'directrepo');
  const regPath = join(tmp, 'registry.json');
  // Empty registry — not even registered
  await writeRegistry(regPath, []);

  process.env.CODEATLAS_REGISTRY_PATH = regPath;
  try {
    const resolved = await resolveRepo(repoPath);
    assert.equal(resolved, repoPath);
  } finally {
    delete process.env.CODEATLAS_REGISTRY_PATH;
    await rm(tmp, { recursive: true, force: true });
  }
});

test('no registered repos → throws "No indexed repositories" error', async () => {
  const tmp = mkdtempSync(join(tmpdir(), 'codeatlas-test-'));
  const regPath = join(tmp, 'registry.json');
  await writeRegistry(regPath, []);

  process.env.CODEATLAS_REGISTRY_PATH = regPath;
  try {
    await assert.rejects(
      () => resolveRepo(undefined),
      (err: Error) => {
        assert.match(err.message, /No indexed repositories/);
        return true;
      }
    );
  } finally {
    delete process.env.CODEATLAS_REGISTRY_PATH;
    await rm(tmp, { recursive: true, force: true });
  }
});

test('multiple repos with param omitted → throws error listing candidates', async () => {
  const tmp = mkdtempSync(join(tmpdir(), 'codeatlas-test-'));
  const repo1 = await makeFakeRepo(tmp, 'repo-a');
  const repo2 = await makeFakeRepo(tmp, 'repo-b');
  const regPath = join(tmp, 'registry.json');
  await writeRegistry(regPath, [
    { name: 'repo-a', path: repo1, indexed_at: '2026-01-01T00:00:00Z' },
    { name: 'repo-b', path: repo2, indexed_at: '2026-01-01T00:00:00Z' },
  ]);

  process.env.CODEATLAS_REGISTRY_PATH = regPath;
  try {
    await assert.rejects(
      () => resolveRepo(undefined),
      (err: Error) => {
        assert.match(err.message, /Multiple repositories/);
        assert.match(err.message, /repo-a/);
        assert.match(err.message, /repo-b/);
        return true;
      }
    );
  } finally {
    delete process.env.CODEATLAS_REGISTRY_PATH;
    await rm(tmp, { recursive: true, force: true });
  }
});
