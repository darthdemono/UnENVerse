// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
/**
 * Phase 30.1: RemoteVaultStore.saveRows sends a PATCH carrying only the
 * changed entries and the deleted ids, with the same If-Match as a full save.
 */
import { describe, it, expect, vi, afterEach } from 'vitest';
import { RemoteVaultStore } from '../src/ts/state';
import type { VaultEntry } from '../src/ts/types';

afterEach(() => vi.unstubAllGlobals());

describe('saveRows', () => {
  it('PATCHes the delta and adopts the returned version', async () => {
    const seen: { method?: string; body?: string; match?: string }[] = [];
    vi.stubGlobal(
      'fetch',
      vi.fn((url: string, init: RequestInit) => {
        const h = (init.headers ?? {}) as Record<string, string>;
        if (String(url).endsWith('/api/unlock')) {
          return Promise.resolve(new Response(JSON.stringify({ token: 't' }), { status: 200 }));
        }
        seen.push({ method: init.method, body: init.body as string, match: h['If-Match'] });
        return Promise.resolve(new Response(null, { status: 204, headers: { ETag: '7.abc' } }));
      }),
    );
    const store = new RemoteVaultStore('http://localhost:1');
    await store.unlock('pw');
    await store.saveRows({ put: [{ id: 'a', provider: 'P' } as VaultEntry], delete: ['gone'] });
    await store.saveRows({ delete: ['x'] });
    expect(seen[0].method).toBe('PATCH');
    expect(JSON.parse(seen[0].body ?? '')).toEqual({
      put: [{ id: 'a', provider: 'P' }],
      delete: ['gone'],
    });
    expect(seen[1].match).toBe('7.abc');
  });
});

describe('pinned HTTPS proxy', () => {
  it('now surfaces the ETag and the merged marker to a save', async () => {
    const calls: { url: string; headers: Record<string, string> }[] = [];
    (window as unknown as Record<string, unknown>).__TAURI__ = {
      core: {
        invoke: (cmd: string, args: { url: string; headersJson: string }) => {
          if (cmd !== 'remote_request') return Promise.resolve(null);
          calls.push({ url: args.url, headers: JSON.parse(args.headersJson) as never });
          if (args.url.endsWith('/api/unlock')) {
            return Promise.resolve({ status: 200, body: '{"token":"t"}' });
          }
          return Promise.resolve({ status: 204, body: '', etag: '9.abc', merged: true });
        },
      },
    };
    try {
      const store = new RemoteVaultStore('https://vault.example:8743', 'f'.repeat(64));
      await store.unlock('pw');
      await store.saveRows({ delete: ['x'] });
      expect(store.takeMerged()).toBe(true);
      await store.saveRows({ delete: ['y'] });
      expect(calls[calls.length - 1].headers['If-Match']).toBe('9.abc');
    } finally {
      delete (window as unknown as Record<string, unknown>).__TAURI__;
    }
  });
});

describe('plain http in the desktop window', () => {
  it('goes through the Rust proxy too, because the page may connect to no host', async () => {
    // The bug this pins: with the page's connect-src narrowed to itself and IPC, a fetch() to a
    // LAN vault over http would be blocked; routing only https left http remotes unreachable.
    const seen: string[] = [];
    (window as unknown as Record<string, unknown>).__TAURI__ = {
      core: {
        invoke: (cmd: string, args: { url: string }) => {
          if (cmd === 'remote_request') seen.push(args.url);
          return Promise.resolve({ status: 200, body: '{"token":"t"}' });
        },
      },
    };
    const page = vi.fn();
    vi.stubGlobal('fetch', page);
    try {
      const store = new RemoteVaultStore('http://192.168.1.5:8743');
      await store.unlock('pw');
      expect(seen).toEqual(['http://192.168.1.5:8743/api/unlock']);
      expect(page).not.toHaveBeenCalled();
    } finally {
      delete (window as unknown as Record<string, unknown>).__TAURI__;
    }
  });

  it('reaches the bridge through the internals handle when there is no global object', async () => {
    const w = window as unknown as Record<string, unknown>;
    delete w.__TAURI__;
    const calls: string[] = [];
    w.__TAURI_INTERNALS__ = {
      invoke: (cmd: string) => {
        calls.push(cmd);
        return Promise.resolve('ok');
      },
    };
    try {
      vi.resetModules();
      const { isTauri, invokeTauri } = await import('../src/ts/tauri');
      expect(isTauri()).toBe(true);
      expect(await invokeTauri('lock_vault')).toBe('ok');
      expect(calls).toEqual(['lock_vault']);
    } finally {
      delete w.__TAURI_INTERNALS__;
    }
    const { isTauri } = await import('../src/ts/tauri');
    expect(isTauri()).toBe(false);
  });
});

describe('changed (the poll behind live reload of a remote vault)', () => {
  const serve = (etag: string | null) => {
    const seen: { match?: string }[] = [];
    vi.stubGlobal(
      'fetch',
      vi.fn((url: string, init: RequestInit) => {
        const h = (init.headers ?? {}) as Record<string, string>;
        if (String(url).endsWith('/api/unlock'))
          return Promise.resolve(new Response(JSON.stringify({ token: 't' }), { status: 200 }));
        if (init.method === 'PUT')
          return Promise.resolve(new Response(null, { status: 204, headers: { ETag: 'v1' } }));
        seen.push({ match: h['If-None-Match'] });
        // 304 when the caller holds the stored version, the vault with a new ETag otherwise.
        return Promise.resolve(
          h['If-None-Match'] === etag
            ? new Response(null, { status: 304, headers: { ETag: etag ?? '' } })
            : new Response('{}', { status: 200, headers: etag ? { ETag: etag } : {} }),
        );
      }),
    );
    return seen;
  };

  it('is false before sign-in and before any version is known, without asking the server', async () => {
    const seen = serve('v1');
    const store = new RemoteVaultStore('http://localhost:1');
    expect(await store.changed()).toBe(false);
    await store.unlock('pw');
    expect(await store.changed()).toBe(false);
    expect(seen).toEqual([]);
  });

  it('is false on a 304 and true once the stored version has moved', async () => {
    const seen = serve('v1');
    const store = new RemoteVaultStore('http://localhost:1');
    await store.unlock('pw');
    await store.save({} as never); // adopts v1
    expect(await store.changed()).toBe(false);
    expect(seen[0].match).toBe('v1');
    serve('v2');
    expect(await store.changed()).toBe(true);
  });

  it('is false when the answer carries no ETag to compare', async () => {
    const store = new RemoteVaultStore('http://localhost:1');
    serve(null);
    await store.unlock('pw');
    await store.save({} as never);
    expect(await store.changed()).toBe(false);
  });
});
