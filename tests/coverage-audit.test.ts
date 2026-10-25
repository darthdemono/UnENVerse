// SPDX-License-Identifier: PolyForm-Strict-1.0.0
// Copyright Jubair Hasan.
/** Branch coverage for audit.ts: loading, the table, verify and export buttons, chain edge rows. */
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { loadRealIndexHtml, resetState } from './helpers';

const toast = vi.fn();
vi.mock('../src/ts/utils', async (importOriginal) => {
  const real = await importOriginal<typeof import('../src/ts/utils')>();
  return { ...real, showToast: (...a: unknown[]) => toast(...a), clipboardWrite: async () => true };
});

const $ = (id: string) => document.getElementById(id)!;
const flush = (ms = 20) => new Promise((r) => setTimeout(r, ms));

async function sha(input: string) {
  const d = await crypto.subtle.digest('SHA-256', new TextEncoder().encode(input));
  return [...new Uint8Array(d)].map((b) => b.toString(16).padStart(2, '0')).join('');
}

type Handler = (cmd: string) => unknown;
async function boot(
  handler: Handler,
  remote?: { getAuditLog: () => Promise<unknown>; api: () => Promise<unknown> },
) {
  vi.resetModules();
  loadRealIndexHtml();
  (window as any).__TAURI__ = { core: { invoke: async (c: string) => handler(c) } };
  const state = await import('../src/ts/state');
  (await import('../src/ts/tools-markup')).mountToolsPanes();
  resetState(state.st);
  if (remote) {
    const r = new state.RemoteVaultStore('http://localhost:1');
    Object.assign(r, remote);
    state.st.store = r;
  }
  const audit = await import('../src/ts/audit');
  audit.initAuditPanel();
  return audit;
}

beforeEach(() => {
  toast.mockClear();
});

const row = (over: object = {}) => ({
  id: 1,
  action: 'add',
  entry_provider: 'GH',
  actor: null,
  timestamp: 't1',
  details: 'd',
  entry_hash: 'abcdefghijklmnop',
  prev_hash: null,
  ...over,
});

describe('loading and rendering', () => {
  it('reads over IPC, shows names for known actors, falls back to short ids, and handles odd users', async () => {
    await boot((cmd) =>
      cmd === 'list_users'
        ? [{ id: 'u1', username: 'alice' }, null, 'str', { id: 1, username: 2 }]
        : [
            row({ actor: 'u1' }),
            row({
              id: 2,
              action: 'weird',
              actor: 'zzzzzzzzzzzz',
              entry_provider: null,
              details: null,
              entry_hash: null,
            }),
            row({ id: 3, actor: null }),
          ],
    );
    $('audit-refresh').click();
    await flush();
    const text = $('audit-results').textContent!;
    expect(text).toContain('alice');
    expect(text).toContain('zzzzzzzz');
    expect($('audit-count').textContent).toBe('3 entries');
    expect($('audit-status').textContent).toBe('Loaded 3 entries.');
  });

  it('an IPC that answers null shows the empty state; a non-array user list is ignored', async () => {
    await boot((cmd) => (cmd === 'list_users' ? { not: 'array' } : null));
    $('audit-refresh').click();
    await flush();
    expect($('audit-results').textContent).toContain('No audit entries yet');
    expect($('audit-count').textContent).toBe('');
  });

  it('reads from a remote store and survives a user-list failure', async () => {
    const audit = await boot(() => [], {
      getAuditLog: async () => [row({ actor: 'x' })],
      api: async () => Promise.reject(new Error('403')),
    });
    $('audit-refresh').click();
    await flush();
    expect($('audit-results').textContent).toContain('GH');
    expect((await audit.loadAuditRows()).length).toBe(1);
    const ok = await boot(() => [], {
      getAuditLog: async () => [row({ actor: 'u9' })],
      api: async () => [{ id: 'u9', username: 'bob' }],
    });
    void ok;
    $('audit-refresh').click();
    await flush();
    expect($('audit-results').textContent).toContain('bob');
  });

  it('reports a load failure', async () => {
    await boot(() => {
      throw new Error('locked');
    });
    $('audit-refresh').click();
    await flush();
    expect($('audit-status').textContent).toContain('Could not load audit log: locked');
  });
});

describe('verify and export', () => {
  it('verify loads rows first, reports a failure to load, and shows intact and broken chains', async () => {
    const good = row({ entry_hash: await sha('add|GH|t1|genesis') });
    let mode: 'good' | 'bad' | 'fail' = 'fail';
    await boot((cmd) => {
      if (cmd === 'list_users') return [];
      if (mode === 'fail') throw new Error('nope');
      return mode === 'good' ? [good] : [{ ...good, entry_hash: 'f'.repeat(64) }];
    });
    $('audit-verify').click();
    await flush();
    expect($('audit-status').textContent).toContain('Could not load audit log: nope');
    mode = 'good';
    $('audit-verify').click();
    await flush();
    expect($('audit-status').textContent).toContain('Chain intact');
    // _rows is now populated: a second verify does not reload.
    mode = 'bad';
    $('audit-refresh').click();
    await flush();
    $('audit-verify').click();
    await flush();
    expect($('audit-status').textContent).toContain('Chain broken');
  });

  it('export needs loaded rows, then copies them', async () => {
    await boot(() => [row()]);
    $('audit-export').click();
    expect(toast).toHaveBeenCalledWith('Load the log first', 'err');
    $('audit-refresh').click();
    await flush();
    $('audit-export').click();
    await flush();
    expect(toast).toHaveBeenCalledWith('Audit log copied ✓', 'ok', 1500);
  });

  it('reset clears the table, count and status, even when they are absent', async () => {
    const audit = await boot(() => [row()]);
    $('audit-refresh').click();
    await flush();
    audit.resetAuditPanel();
    expect($('audit-results').textContent).toBe('');
    expect(($('audit-status') as HTMLElement).style.display).toBe('none');
    $('audit-results').remove();
    $('audit-count').remove();
    $('audit-status').remove();
    audit.resetAuditPanel();
  });
});

describe('verifyChain rows without a provider', () => {
  it('hashes a missing provider as empty in both chain formats', async () => {
    const { verifyChain } = await import('../src/ts/audit');
    const a = {
      id: 1,
      action: 'add',
      entry_provider: null,
      actor: null,
      timestamp: 't',
      details: null,
      prev_hash: null,
    } as any;
    a.entry_hash = await sha('add||t|genesis');
    const b = {
      id: 2,
      action: 'add',
      entry_provider: null,
      actor: 'u',
      timestamp: 't2',
      details: null,
      prev_hash: a.entry_hash,
    } as any;
    b.entry_hash = await sha(`add||t2|u|${a.entry_hash}`);
    expect((await verifyChain([b, a])).ok).toBe(true);
  });
});
