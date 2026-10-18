// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
/**
 * Key pools in the desktop app, where the swap state (use counts, cooldowns, the
 * rotation cursor) lives in Rust.
 *
 * Properties worth pinning: no secret value ever crosses to Rust, a remote
 * session asks about the remote's pools and not this machine's, Mark limited /
 * Clear / Reset do exactly what they say, and a failing state file degrades to
 * "unavailable" rather than to zeros that look real.
 */
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { loadRealIndexHtml, makeEntry, makeVault, resetState } from './helpers';

const toasts: [string, string?][] = [];
let confirmAnswer = true;
const confirms: string[] = [];
vi.mock('../src/ts/utils', async (importOriginal) => {
  const real = await importOriginal<typeof import('../src/ts/utils')>();
  return {
    ...real,
    showToast: (m: string, k?: string) => toasts.push([m, k]),
    showConfirm: async (m: string) => {
      confirms.push(m);
      return confirmAnswer;
    },
  };
});

const $ = (id: string) => document.getElementById(id)!;
const flush = (ms = 20) => new Promise((r) => setTimeout(r, ms));

type Calls = [string, any][];
let calls: Calls = [];
let backend: (cmd: string, args: any) => unknown;

const member = (over: object = {}) =>
  makeEntry({ pool: 'openai', provider: 'OpenAI', api_key: 'sk-TOP-SECRET', ...over } as any);
const state = (ck: string, over: object = {}) => ({
  ck,
  uses: 0,
  cooling: false,
  cooling_until: null,
  last_used_at: null,
  ...over,
});

async function boot(entries: any[], opts: { remote?: string } = {}) {
  vi.resetModules();
  loadRealIndexHtml();
  calls = [];
  (window as any).__TAURI__ = {
    core: {
      invoke: async (cmd: string, args: any) => {
        calls.push([cmd, args]);
        return backend(cmd, args);
      },
    },
  };
  const s = await import('../src/ts/state');
  (await import('../src/ts/tools-markup')).mountToolsPanes();
  resetState(s.st);
  s.st.vault = makeVault({ api_keys: entries });
  if (opts.remote) {
    const r = new s.RemoteVaultStore(opts.remote);
    s.st.store = r;
  }
  const pools = await import('../src/ts/pools');
  pools.initPoolsPane();
  return { pools, st: s.st };
}

beforeEach(() => {
  toasts.length = 0;
  confirms.length = 0;
  confirmAnswer = true;
  backend = (cmd) => (cmd === 'pool_state_path' ? '/home/me/.config/unv/pools.json' : []);
});

describe('the pane', () => {
  it('shows use counts, last use and cooldowns, and where the state file is', async () => {
    const a = member({ id: 'a', key_id: 'one' });
    const b = member({ id: 'b', key_id: 'two' });
    backend = (cmd) =>
      cmd === 'pool_state'
        ? [
            state('a', {
              uses: 7,
              last_used_at: new Date(Date.now() - 3 * 3600_000).toISOString(),
            }),
            state('b', { cooling: true, cooling_until: '2026-12-01T10:00:00Z' }),
          ]
        : '/home/me/.config/unv/pools.json';
    const { pools } = await boot([a, b]);
    await pools.renderPoolsPane();
    const text = $('pools-body').textContent!.replace(/\s+/g, ' ');
    expect(text).toContain('OpenAI:one');
    expect(text).toContain('7');
    expect(text).toContain('3 hours ago');
    expect(text).toContain('never');
    expect(text).toContain('2 keys · 1 cooling');
    expect(text).toContain('/home/me/.config/unv/pools.json');
    expect($('pools-body').querySelector('.pool-row-cooling')?.textContent).toContain('OpenAI:two');
    expect($('pools-body').querySelector('.badge-cooling')?.getAttribute('title')).toContain(
      '2026-12-01T10:00:00Z',
    );
    expect($('pools-body').querySelector('[data-pool-action="clear"]')).toBeTruthy();
    expect($('pools-body').querySelector('[data-pool-action="limit"]')).toBeTruthy();
  });

  it('a one-key pool reads in the singular; an unnamed entry has a fallback label', async () => {
    const { pools } = await boot([member({ id: 'a', provider: '' })]);
    await pools.renderPoolsPane();
    const flat = $('pools-body').textContent!.replace(/\s+/g, ' ');
    expect(flat).toContain('1 key');
    expect(flat).not.toContain('1 keys');
    expect($('pools-body').textContent).toContain('(unnamed)');
  });

  it('never sends a secret value to Rust, only identity fields', async () => {
    const { pools } = await boot([
      member({ id: 'a', account_name: 'acct', key_id: 'k1', api_secret: 'ALSO-SECRET' }),
    ]);
    await pools.renderPoolsPane();
    const sent = JSON.stringify(calls);
    expect(sent).not.toContain('sk-TOP-SECRET');
    expect(sent).not.toContain('ALSO-SECRET');
    const call = calls.find(([c]) => c === 'pool_state')!;
    expect(call[1].members[0]).toEqual({
      id: 'a',
      provider: 'OpenAI',
      account_name: 'acct',
      key_id: 'k1',
    });
  });

  it("asks about the remote's pools when a remote vault is open", async () => {
    const { pools } = await boot([member({ id: 'a' })], { remote: 'https://vault.example' });
    await pools.renderPoolsPane();
    expect(calls.find(([c]) => c === 'pool_state')![1].remoteBase).toBe('https://vault.example');
    const local = await boot([member({ id: 'a' })]);
    await local.pools.renderPoolsPane();
    expect(calls.find(([c]) => c === 'pool_state')![1].remoteBase).toBeNull();
  });

  it('degrades to "unavailable" when the state file cannot be read, offering no buttons', async () => {
    backend = (cmd) => {
      if (cmd === 'pool_state') throw new Error('unreadable');
      return '';
    };
    const { pools } = await boot([member({ id: 'a' })]);
    await pools.renderPoolsPane();
    const text = $('pools-body').textContent!;
    expect(text).toContain('Could not read');
    expect(text).toContain('—');
    expect($('pools-body').querySelector('[data-pool-action]')).toBeNull();
  });

  it('shows the generic file name when the path lookup fails or is empty', async () => {
    backend = (cmd) => {
      if (cmd === 'pool_state_path') throw new Error('no path');
      return [];
    };
    const { pools } = await boot([member({ id: 'a' })]);
    await pools.renderPoolsPane();
    expect($('pools-body').textContent).toContain('pools.json');
  });

  it('treats a missing answer from Rust as no state yet', async () => {
    backend = (cmd) => (cmd === 'pool_state' ? undefined : null);
    const { pools } = await boot([member({ id: 'a' })]);
    await pools.renderPoolsPane();
    expect($('pools-body').textContent).toContain('available');
  });
});

describe('the buttons', () => {
  const setup = async (cooling = false) => {
    backend = (cmd) => (cmd === 'pool_state' ? [state('a', { cooling })] : '/p');
    const out = await boot([member({ id: 'a', key_id: 'one' })]);
    await out.pools.renderPoolsPane();
    return out;
  };
  const click = async (action: string) => {
    ($('pools-body').querySelector(`[data-pool-action="${action}"]`) as HTMLElement).click();
    await flush();
  };

  it('Mark limited cools that member for fifteen minutes and repaints', async () => {
    await setup();
    await click('limit');
    const call = calls.find(([c]) => c === 'pool_set_cooldown')!;
    expect(call[1]).toMatchObject({ pool: 'openai', seconds: 900, member: { id: 'a' } });
    expect(toasts).toContainEqual(['OpenAI:one cooling for 15m', 'ok']);
    expect(calls.filter(([c]) => c === 'pool_state').length).toBeGreaterThan(1);
  });

  it('Clear lifts the cooldown', async () => {
    await setup(true);
    await click('clear');
    expect(calls.find(([c]) => c === 'pool_set_cooldown')![1].seconds).toBeNull();
    expect(toasts).toContainEqual(['OpenAI:one available again', 'ok']);
  });

  it('Reset asks first, and only then resets this pool', async () => {
    await setup();
    confirmAnswer = false;
    await click('reset');
    expect(calls.some(([c]) => c === 'pool_reset')).toBe(false);
    confirmAnswer = true;
    await click('reset');
    expect(confirms[1]).toContain('Reset cursor, cooldowns and counts for "openai"?');
    expect(calls.find(([c]) => c === 'pool_reset')![1]).toEqual({
      pool: 'openai',
      remoteBase: null,
    });
    expect(toasts).toContainEqual(['Pool "openai" reset', 'ok']);
  });

  it('resolves the member at click time, so a member removed meanwhile changes nothing', async () => {
    const { st } = await setup();
    st.vault.api_keys = [];
    await click('limit');
    expect(calls.some(([c]) => c === 'pool_set_cooldown')).toBe(false);
  });

  it('reports a backend failure and ignores clicks that are not on an action', async () => {
    await setup();
    backend = (cmd) => {
      if (cmd === 'pool_set_cooldown') throw new Error('disk full');
      return [];
    };
    await click('limit');
    expect(
      toasts.some(
        ([m, k]) => m.startsWith('Pool update failed') && m.includes('disk full') && k === 'err',
      ),
    ).toBe(true);
    const before = calls.length;
    ($('pools-body').querySelector('.pool-card h4') as HTMLElement).click();
    $('pools-body').click();
    await flush();
    expect(calls.length).toBe(before);
  });
});

describe('badge and rotation', () => {
  it('summarises ready and cooling members for the collapsed pool card', async () => {
    backend = (cmd) =>
      cmd === 'pool_state' ? [state('a'), state('b', { cooling: true }), state('c')] : null;
    const { pools } = await boot([]);
    const members = [member({ id: 'a' }), member({ id: 'b' }), member({ id: 'c' })];
    expect(await pools.poolBadgeInfo('openai', members)).toEqual({ ready: 2, cooling: 1 });
    backend = () => undefined;
    expect(await pools.poolBadgeInfo('openai', members)).toBeNull();
    backend = () => {
      throw new Error('x');
    };
    expect(await pools.poolBadgeInfo('openai', members)).toBeNull();
  });

  it('badge and next do nothing outside the desktop app', async () => {
    vi.resetModules();
    delete (window as any).__TAURI__;
    const pools = await import('../src/ts/pools');
    expect(await pools.poolBadgeInfo('p', [member()])).toBeNull();
    expect(await pools.poolNext('p', [member()])).toBeNull();
  });

  it('next returns the member Rust picked, or nothing when all are cooling, empty or failing', async () => {
    const members = [member({ id: 'a' }), member({ id: 'b' })];
    backend = () => 1;
    const { pools } = await boot([]);
    expect((await pools.poolNext('openai', members))!.id).toBe('b');
    backend = () => null;
    expect(await pools.poolNext('openai', members)).toBeNull();
    backend = () => 9;
    expect(await pools.poolNext('openai', members)).toBeNull();
    expect(await pools.poolNext('openai', [])).toBeNull();
    backend = () => {
      throw new Error('x');
    };
    expect(await pools.poolNext('openai', members)).toBeNull();
  });
});
