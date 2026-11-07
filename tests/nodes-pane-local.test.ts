// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
/**
 * Tools -> Nodes against the local vault in the desktop app (R12): the pane's requests go to the
 * embedded hub through `nodes_local_request`, with the same paths a server would see.
 */
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { loadRealIndexHtml, resetState } from './helpers';

const $ = (id: string) => document.getElementById(id)!;
const flush = (ms = 20) => new Promise((r) => setTimeout(r, ms));

type Req = { method: string; path: string; body: unknown };

async function boot(answer: (r: Req) => { status: number; body: unknown }) {
  loadRealIndexHtml();
  vi.resetModules();
  const calls: Req[] = [];
  const invoke = vi.fn(async (cmd: string, args?: unknown) => {
    if (cmd !== 'nodes_local_request') return null;
    const r = args as Req;
    calls.push(r);
    return answer(r);
  });
  (window as unknown as { __TAURI__: unknown }).__TAURI__ = { core: { invoke } };
  const { st } = await import('../src/ts/state');
  (await import('../src/ts/tools-markup')).mountToolsPanes();
  resetState(st);
  (await import('../src/ts/nodes-pane')).initNodesPane();
  return calls;
}

beforeEach(() => {
  delete (window as unknown as { __TAURI__?: unknown }).__TAURI__;
});

describe('Nodes pane against the local vault', () => {
  it('lists nodes through the embedded hub instead of asking for a server', async () => {
    const calls = await boot(() => ({
      status: 200,
      body: {
        nodes: [
          {
            id: 'n1',
            name: 'edge-1',
            fingerprint: 'ab'.repeat(32),
            projects: ['web'],
            last_seen: null,
            revoked_at: null,
            host: null,
            targets: [],
            approvals: [],
          },
        ],
      },
    }));
    $('nodes-refresh-btn').click();
    await flush();
    expect(calls[0]).toEqual({ method: 'GET', path: '/api/nodes', body: null });
    expect($('nodes-status').textContent ?? '').not.toContain('Nodes live on a server');
    expect(document.body.textContent).toContain('edge-1');
  });

  it('shows the hub refusal for a locked vault in words', async () => {
    await boot(() => ({ status: 503, body: { error: 'the vault is locked' } }));
    $('nodes-refresh-btn').click();
    await flush();
    expect($('nodes-status').textContent).toContain('Refused (503): the vault is locked');
  });
});
