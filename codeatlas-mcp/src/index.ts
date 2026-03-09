#!/usr/bin/env node

/**
 * CodeAtlas MCP Bridge (stdio)
 *
 * Thin MCP server that forwards tool/resource requests to the `codeatlas` CLI.
 * The server itself is stateless: source-of-truth is always the local index DB
 * managed by the Rust CLI (`<repo>/.codeatlas/index.db`).
 *
 * Exposed tools are defined in `tools.ts`.
 * Exposed resource templates are defined in `resources.ts`.
 */

import { Server } from '@modelcontextprotocol/sdk/server/index.js';
import { StdioServerTransport } from '@modelcontextprotocol/sdk/server/stdio.js';
import {
  CallToolRequestSchema,
  ListToolsRequestSchema,
  ListResourceTemplatesRequestSchema,
  ReadResourceRequestSchema,
} from '@modelcontextprotocol/sdk/types.js';
import { TOOLS, callTool } from './tools.js';
import { RESOURCE_TEMPLATES, readResource } from './resources.js';

const server = new Server(
  { name: 'codeatlas', version: '0.1.0' },
  { capabilities: { tools: {}, resources: {} } },
);

// List tools
server.setRequestHandler(ListToolsRequestSchema, async () => ({
  tools: TOOLS.map((t) => ({
    name: t.name,
    description: t.description,
    inputSchema: t.inputSchema,
  })),
}));

// Handle tool calls
server.setRequestHandler(CallToolRequestSchema, async (request) => {
  const { name, arguments: args } = request.params;
  try {
    const text = await callTool(name, (args as Record<string, any>) ?? {});
    return { content: [{ type: 'text', text }] };
  } catch (error) {
    const message = error instanceof Error ? error.message : 'Unknown error';
    return { content: [{ type: 'text', text: `Error: ${message}` }], isError: true };
  }
});

// List resource templates
server.setRequestHandler(ListResourceTemplatesRequestSchema, async () => ({
  resourceTemplates: RESOURCE_TEMPLATES.map((t) => ({
    uriTemplate: t.uriTemplate,
    name: t.name,
    description: t.description,
    mimeType: t.mimeType,
  })),
}));

// Read resources
server.setRequestHandler(ReadResourceRequestSchema, async (request) => {
  const { uri } = request.params;
  try {
    const text = await readResource(uri);
    return { contents: [{ uri, mimeType: 'application/json', text }] };
  } catch (err: any) {
    return { contents: [{ uri, mimeType: 'text/plain', text: `Error: ${err.message}` }] };
  }
});

// Start on stdio
async function main() {
  const transport = new StdioServerTransport();
  await server.connect(transport);
  console.error('CodeAtlas MCP server running on stdio');

  const shutdown = async () => {
    try { await server.close(); } catch {}
    process.exit(0);
  };
  process.on('SIGINT', shutdown);
  process.on('SIGTERM', shutdown);
  process.stdin.on('end', shutdown);
}

main().catch((err) => {
  console.error('Fatal:', err);
  process.exit(1);
});
