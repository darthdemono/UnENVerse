// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
/**
 * Tools -> Operations (R15): the page that gathers runs, held approvals, deployments and leases.
 * Names come from the vault, so a hostile one must stay text; a locked vault must say so rather
 * than show an empty page that reads as "nothing is happening".
 */
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { loadRealIndexHtml } from './helpers';
import type { Overview } from '../src/ts/ops-centre';

let confirmAnswer = true;
vi.mock('../src/ts/utils', async (importOriginal) => {
  const real = await importOriginal<typeof import('../src/ts/utils')>();
  return { ...real, showConfirm: async () => confirmAnswer };
});

const $ = (id: string) => document.getElementById(id)!;
const flush = (ms = 20) => new Promise((r) => setTimeout(r, ms));

const overview = (over: Partial<Overview> = {}): Overview => ({
  runs: [
    {
      id: 'r1',
      kind: 'backup.package',
      state: 'running',
      created_at: '2026-11-07T10:00:00Z',
    },
    {
      id: 'r2',
      kind: 'backup.drill',
      state: 'succeeded',
      outcome: 'restored 3 entries',
      created_at: '2026-11-07T09:00:00Z',
    },
  ],
  approvals: [{ id: 'a1', node_id: 'edge-1', target: 'nginx', sha256: 'ab'.repeat(32) }],
  deployments: [
    {
      entry: 'GitHub',
      field: 'api_key',
      target_id: 'edge-1/nginx',
      generation: 2,
      result: 'applied',
      checked_at: '2026-11-07T10:05:00Z',
    },
  ],
  mismatches: [
    {
      entry: 'Stripe',
      field: 'api_key',
      target_id: 'edge-1/env',
      deployed_generation: 1,
      active_generation: 2,
    },
  ],
  leases: [{ node_id: 'edge-1', target: 'nginx', holder: 'rot-1', expires_secs: 9_999_999_999 }],
  nodes: 1,
  ...over,
});

async function boot(invoke?: (cmd: string, args?: unknown) => Promise<unknown>) {
  loadRealIndexHtml();
  vi.resetModules();
  delete (window as unknown as { __TAURI__?: unknown }).__TAURI__;
  if (invoke) (window as unknown as { __TAURI__: unknown }).__TAURI__ = { core: { invoke } };
  (await import('../src/ts/tools-markup')).mountToolsPanes();
  const m = await import('../src/ts/ops-centre');
  m.initOpsCentre();
  return m;
}

beforeEach(() => {
  confirmAnswer = true;
});

describe('Operations centre', () => {
  it('summarises the counts in one line', async () => {
    const m = await boot();
    expect(m.summary(overview())).toBe(
      '1 running or waiting · 1 held for approval · 1 rotated but not redeployed · 1 lease · 1 node',
    );
    expect(
      m.summary(overview({ runs: [], approvals: [], mismatches: [], leases: [], nodes: 3 })),
    ).toBe(
      '0 running or waiting · 0 held for approval · 0 rotated but not redeployed · 0 leases · 3 nodes',
    );
  });

  it('shows every part, a cancel button only on an open run, and escapes hostile names', async () => {
    const invoke = vi.fn(async () =>
      overview({
        deployments: [
          {
            entry: '<img src=x onerror=alert(1)>',
            field: 'api_key',
            target_id: 't',
            generation: 1,
            result: 'applied',
          },
        ],
      }),
    );
    const m = await boot(invoke);
    await m.refreshOperations();
    const list = $('ops-list');
    expect(list.textContent).toContain('backup.package');
    expect(list.textContent).toContain('restored 3 entries');
    expect(list.textContent).toContain('Held for your approval');
    expect(list.textContent).toContain('rotated but not redeployed'.replace('r', 'R'));
    expect(list.textContent).toContain('held by rot-1');
    expect(list.querySelector('img')).toBeNull();
    expect(list.textContent).toContain('<img src=x onerror=alert(1)>');
    const cancels = list.querySelectorAll('[data-ops-cancel]');
    expect(cancels).toHaveLength(1);
    expect((cancels[0] as HTMLElement).dataset.opsCancel).toBe('r1');
    expect($('ops-status').textContent).toContain('1 running or waiting');
  });

  it('says what each empty section means instead of leaving it blank', async () => {
    const m = await boot(
      vi.fn(async () =>
        overview({ runs: [], approvals: [], deployments: [], mismatches: [], leases: [] }),
      ),
    );
    await m.refreshOperations();
    const text = $('ops-list').textContent!;
    for (const s of [
      'No runs yet.',
      'Nothing is waiting for you.',
      'Every reported deployment is on the active generation.',
      'No node has reported a deployment yet.',
      'No target is leased.',
    ]) {
      expect(text).toContain(s);
    }
  });

  it('points at the terminal in a plain browser and asks to unlock when the vault is locked', async () => {
    let m = await boot();
    await m.refreshOperations();
    expect($('ops-status').textContent).toContain('desktop app');
    m = await boot(async () => {
      throw new Error('Vault is locked');
    });
    await m.refreshOperations();
    expect($('ops-status').textContent).toBe('Unlock the vault to see its operations.');
    expect($('ops-list').textContent).toBe('');
    m = await boot(async () => {
      throw new Error('disk on fire');
    });
    await m.refreshOperations();
    expect($('ops-status').textContent).toBe('Could not read: disk on fire');
  });

  it('cancels a run only after the person confirms, then refreshes', async () => {
    const invoke = vi.fn(async (cmd: string) =>
      cmd === 'operations_overview' ? overview() : null,
    );
    const m = await boot(invoke);
    await m.refreshOperations();
    confirmAnswer = false;
    $('ops-list').querySelector<HTMLElement>('[data-ops-cancel]')!.click();
    await flush();
    expect(invoke.mock.calls.some((c) => c[0] === 'ops_cancel')).toBe(false);
    confirmAnswer = true;
    $('ops-list').querySelector<HTMLElement>('[data-ops-cancel]')!.click();
    await flush();
    const cancel = invoke.mock.calls.find((c) => c[0] === 'ops_cancel');
    expect((cancel as unknown[] | undefined)?.[1]).toEqual({ id: 'r1' });
    expect(invoke.mock.calls.filter((c) => c[0] === 'operations_overview').length).toBeGreaterThan(
      1,
    );
  });

  it('reports a refused cancel and clicks elsewhere do nothing', async () => {
    const invoke = vi.fn(async (cmd: string) => {
      if (cmd === 'ops_cancel') throw new Error('already finished');
      return overview();
    });
    const m = await boot(invoke);
    await m.refreshOperations();
    $('ops-list').click();
    $('ops-list').querySelector<HTMLElement>('[data-ops-cancel]')!.click();
    await flush();
    expect($('toast').textContent).toContain('Not cancelled: already finished');
  });

  it('does nothing when the markup lacks the pane', async () => {
    loadRealIndexHtml();
    vi.resetModules();
    const m = await import('../src/ts/ops-centre');
    expect(() => m.initOpsCentre()).not.toThrow();
    await expect(m.refreshOperations()).resolves.toBeUndefined();
  });
});
