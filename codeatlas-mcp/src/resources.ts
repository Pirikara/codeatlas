/**
 * MCP Resource definitions and handlers for CodeAtlas.
 *
 * Resources are read-only data endpoints that provide context.
 * They use a URI template with {repo} as the repository path.
 */

import * as codeatlas from './codeatlas.js';
import { resolveRepo } from './codeatlas.js';

export interface ResourceTemplate {
  uriTemplate: string;
  name: string;
  description: string;
  mimeType: string;
}

export const RESOURCE_TEMPLATES: ResourceTemplate[] = [
  {
    uriTemplate: 'codeatlas://repo/{repo}/status',
    name: 'Index Status',
    description: 'Index statistics: symbol count, relationship count, file count, communities, execution flows, and last indexed time. Use this to check if the index is fresh.',
    mimeType: 'application/json',
  },
  {
    uriTemplate: 'codeatlas://repo/{repo}/clusters',
    name: 'Communities',
    description: 'List of detected communities (symbol clusters) with cohesion scores and top member symbols. Communities represent functional areas of the codebase.',
    mimeType: 'application/json',
  },
  {
    uriTemplate: 'codeatlas://repo/{repo}/processes',
    name: 'Execution Flows',
    description: 'List of detected execution flows (call chains) with their steps, types, and priority scores. Flows represent how code actually executes.',
    mimeType: 'application/json',
  },
];

/** Parse a resource URI and return the repo path + resource type. */
function parseUri(uri: string): { repo: string; resource: string } | null {
  // codeatlas://repo/{repo}/status
  const match = uri.match(/^codeatlas:\/\/repo\/(.+)\/(status|clusters|processes)$/);
  if (!match) return null;
  return { repo: match[1], resource: match[2] };
}

/** Read a resource by URI. */
export async function readResource(uri: string): Promise<string> {
  const parsed = parseUri(uri);
  if (!parsed) {
    throw new Error(`Invalid resource URI: ${uri}. Expected: codeatlas://repo/{path}/{status|clusters|processes}`);
  }

  const { resource } = parsed;
  const repo = await resolveRepo(parsed.repo);

  switch (resource) {
    case 'status': {
      const data = await codeatlas.status(repo);
      if (!data) return JSON.stringify({ error: 'No index found. Run `codeatlas index` first.' });
      return JSON.stringify(data, null, 2);
    }
    case 'clusters': {
      const data = await codeatlas.clusters(repo);
      if (!data) return '[]';
      return JSON.stringify(data, null, 2);
    }
    case 'processes': {
      const data = await codeatlas.processes(repo);
      if (!data) return '[]';
      return JSON.stringify(data, null, 2);
    }
    default:
      throw new Error(`Unknown resource: ${resource}`);
  }
}
