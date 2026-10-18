// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
/**
 * The two vault stores speak to the outside world: the desktop one over Tauri
 * commands, the remote one over REST. What they send is a contract with the
 * server, so it is pinned here: which credential goes where, that a write
 * carries the version it was made against, and that every failure is named
 * rather than swallowed or mistaken for success.
 */
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import {
  RemoteVaultStore,
  TauriVaultStore,
  TotpRequiredError,
  VaultConflictError,
  conflictEntries,
} from '../src/ts/state';

const toasts: [string, string?][] = [];
vi.mock('../src/ts/utils', async (importOriginal) => {
  const real = await importOriginal<typeof import('../src/ts/utils')>();
  return { ...real, showToast: (m: string, k?: string) => toasts.push([m, k]) };
});

type Reply = { status?: number; body?: unknown; headers?: Record<string, string>; throws?: Error };
let replies: Reply[] = [];
let requests: { url: string; method: string; headers: Record<string, string>; body: unknown }[] =
  [];

function fakeFetch() {
  return vi.fn(
    async (
      url: string,
      init: { method?: string; headers?: Record<string, string>; body?: string } = {},
    ) => {
      requests.push({
        url,
        method: init.method ?? 'GET',
        headers: init.headers ?? {},
        body: init.body === undefined ? undefined : JSON.parse(init.body),
      });
      const r = replies.shift() ?? { status: 200, body: {} };
      if (r.throws) throw r.throws;
      const status = r.status ?? 200;
      return {
        ok: status >= 200 && status < 300,
        status,
        headers: { get: (k: string) => r.headers?.[k.toLowerCase()] ?? null },
        json: async () => {
          if (r.body === undefined) throw new Error('no body');
          return r.body;
        },
      };
    },
  );
}

const BASE = 'http://vault.test';
const reply = (...r: Reply[]) => replies.push(...r);
const authed = async () => {
  const s = new RemoteVaultStore(BASE);
  reply({ body: { token: 'TOK' } });
  await s.unlock('pw');
  requests.length = 0;
  return s;
};

beforeEach(() => {
  replies = [];
  requests = [];
  toasts.length = 0;
  vi.stubGlobal('fetch', fakeFetch());
  delete (window as any).__TAURI__;
});
afterEach(() => {
  vi.unstubAllGlobals();
});

describe('signing in', () => {
  it('sends the master password once and keeps only the token it gets back', async () => {
    const s = new RemoteVaultStore(BASE);
    reply({ body: { token: 'TOK' } });
    expect(await s.unlock('pw')).toBe(true);
    expect(requests[0]).toMatchObject({
      url: `${BASE}/api/unlock`,
      method: 'POST',
      body: { password: 'pw' },
    });
    expect(requests[0].headers.Authorization).toBeUndefined();
    reply({ body: { vault_exists: true } });
    await s.getExpiring(3);
    expect(requests[1].headers.Authorization).toBe('Bearer TOK');
  });

  it('a refused password throws the server’s reason; a reply without a token is not a login', async () => {
    const s = new RemoteVaultStore(BASE);
    reply({ status: 401, body: { error: 'wrong password' } });
    await expect(s.unlock('x')).rejects.toThrow('wrong password');
    reply({ status: 500, body: undefined });
    await expect(s.unlock('x')).rejects.toThrow('Server error 500');
    reply({ body: {} });
    expect(await s.unlock('x')).toBe(false);
    reply({ body: { token: 5 } });
    expect(await s.unlock('x')).toBe(false);
  });

  it('a named user signs in with an optional second-factor code', async () => {
    const s = new RemoteVaultStore(BASE);
    reply({ body: { token: 'T2' } });
    expect(await s.authUser('alice', 'pw')).toBe(true);
    expect(requests[0].body).toEqual({ username: 'alice', password: 'pw', totp: null });
    reply({ body: { token: 'T3' } });
    await s.authUser('alice', 'pw', '123456');
    expect(requests[1].body).toEqual({ username: 'alice', password: 'pw', totp: '123456' });
  });

  it('a correct password that still needs a code is a prompt, not a failure', async () => {
    const s = new RemoteVaultStore(BASE);
    reply({ status: 401, body: { totp_required: true } });
    await expect(s.authUser('alice', 'pw')).rejects.toBeInstanceOf(TotpRequiredError);
    reply({ status: 401, body: { error: 'bad credentials' } });
    await expect(s.authUser('alice', 'pw')).rejects.toThrow('bad credentials');
    reply({ status: 403, body: { totp_required: true } });
    await expect(s.authUser('alice', 'pw')).rejects.not.toBeInstanceOf(TotpRequiredError);
    reply({ status: 502, body: undefined });
    await expect(s.authUser('alice', 'pw')).rejects.toThrow('Auth failed 502');
    reply({ body: {} });
    expect(await s.authUser('alice', 'pw')).toBe(false);
  });

  it('locking ends the session on the server and forgets the token, even if the server cannot be reached', async () => {
    const s = await authed();
    reply({ status: 200, body: {} });
    await s.lock();
    expect(requests[0]).toMatchObject({ url: `${BASE}/api/unlock`, method: 'DELETE' });
    expect(requests[0].headers.Authorization).toBe('Bearer TOK');
    requests.length = 0;
    await s.lock();
    expect(requests).toHaveLength(0);
    const t = await authed();
    reply({ throws: new Error('offline') });
    await t.lock();
    expect(await t.load()).toBeNull();
  });

  it('reports whether the server considers the vault unlocked', async () => {
    const s = new RemoteVaultStore(BASE);
    expect(await s.isUnlocked()).toBe(false);
    const a = await authed();
    reply({ body: { unlocked: true } });
    expect(await a.isUnlocked()).toBe(true);
    reply({ body: { unlocked: false } });
    expect(await a.isUnlocked()).toBe(false);
    reply({ status: 500, body: {} });
    expect(await a.isUnlocked()).toBe(false);
    reply({ throws: new Error('x') });
    expect(await a.isUnlocked()).toBe(false);
  });
});

describe('reading', () => {
  it('without a session nothing is asked of the server', async () => {
    const s = new RemoteVaultStore(BASE);
    expect(await s.load()).toBeNull();
    expect(await s.getExpiring(1)).toEqual([]);
    expect(await s.getAuditLog()).toEqual([]);
    expect(await s.ping()).toBe(false);
    expect(await s.createCalendarFeed('n', [], false)).toBeNull();
    expect(await s.listCalendarFeeds()).toEqual([]);
    expect(await s.revokeCalendarFeed('x')).toBe(false);
    expect(await s.uidRequest('GET', '/api/uid/stats')).toEqual({
      status: 401,
      body: { error: 'Not connected' },
    });
    expect(await s.nodesRequest('GET', '/api/nodes')).toEqual({
      status: 401,
      body: { error: 'Not connected' },
    });
    await expect(s.api('/api/users')).rejects.toThrow('Not authenticated');
    expect(requests).toHaveLength(0);
  });

  it('loads the vault, and a failure or refusal reads as "nothing loaded"', async () => {
    const s = await authed();
    reply({ body: { api_keys: [] }, headers: { etag: 'v1' } });
    expect(await s.load()).toEqual({ api_keys: [] });
    reply({ status: 403, body: {} });
    expect(await s.load()).toBeNull();
    reply({ throws: new Error('offline') });
    expect(await s.load()).toBeNull();
  });

  it('expiring entries, audit rows and pings tolerate bad answers', async () => {
    const s = await authed();
    reply({ body: [{ id: 1 }] });
    expect(await s.getExpiring(7)).toEqual([{ id: 1 }]);
    expect(requests[0].url).toBe(`${BASE}/api/vault/expiring?days=7`);
    reply({ body: { not: 'a list' } });
    expect(await s.getExpiring(7)).toEqual([]);
    reply({ status: 500, body: [] });
    expect(await s.getExpiring(7)).toEqual([]);
    reply({ throws: new Error('x') });
    expect(await s.getExpiring(7)).toEqual([]);
    reply({ body: [{ id: 1 }] });
    expect(await s.getAuditLog()).toEqual([{ id: 1 }]);
    reply({ body: 'nope' });
    expect(await s.getAuditLog()).toEqual([]);
    reply({ throws: new Error('x') });
    expect(await s.getAuditLog()).toEqual([]);
    reply({ status: 200 });
    expect(await s.ping()).toBe(true);
    reply({ status: 401 });
    expect(await s.ping()).toBe(false);
    reply({ throws: new Error('x') });
    expect(await s.ping()).toBe(false);
  });

  it('status needs no login and reports only what it can prove', async () => {
    const s = new RemoteVaultStore(BASE);
    reply({ body: { unlocked: true, vault_exists: true, cert_fingerprint: 'AB' } });
    expect(await s.getStatus()).toEqual({
      unlocked: true,
      vault_exists: true,
      cert_fingerprint: 'AB',
    });
    expect(requests[0].headers.Authorization).toBeUndefined();
    reply({ body: { unlocked: 'yes', vault_exists: 1, cert_fingerprint: 5 } });
    expect(await s.getStatus()).toEqual({ unlocked: false, vault_exists: false });
    reply({ status: 500, body: {} });
    expect(await s.getStatus()).toEqual({ unlocked: false, vault_exists: false });
    reply({ throws: new Error('x') });
    expect(await s.getStatus()).toEqual({ unlocked: false, vault_exists: false });
  });

  it('identifies itself by its address', () => {
    const s = new RemoteVaultStore(BASE);
    expect(s.vaultId).toBe(BASE);
    expect(s.isRemote).toBe(true);
  });
});

describe('writing', () => {
  it('a save carries the version it was made against, and the next one carries the version it was given', async () => {
    const s = await authed();
    reply({ body: {}, headers: { etag: 'v1' } });
    await s.load();
    reply({ body: {}, headers: { etag: 'v2' } });
    await s.save({ api_keys: [] } as never);
    expect(requests[1]).toMatchObject({ method: 'PUT', url: `${BASE}/api/vault` });
    expect(requests[1].headers['If-Match']).toBe('v1');
    expect(requests[1].headers.Authorization).toBe('Bearer TOK');
    reply({ body: {} });
    await s.save({ api_keys: [] } as never);
    expect(requests[2].headers['If-Match']).toBe('v2');
    expect(s.lastSaveFailed()).toBe(false);
  });

  it('a save made before any load is unconditional', async () => {
    const s = await authed();
    reply({ body: {} });
    await s.save({} as never);
    expect(requests[0].headers['If-Match']).toBeUndefined();
  });

  it('reports when the server folded in another writer’s changes, exactly once', async () => {
    const s = await authed();
    reply({ body: {}, headers: { etag: 'v9', 'x-vault-merged': '1' } });
    await s.save({} as never);
    expect(s.takeMerged()).toBe(true);
    expect(s.takeMerged()).toBe(false);
  });

  it('a conflict says which entries both writers changed, or that the vault changed, and the save did not happen', async () => {
    const s = await authed();
    reply({ status: 409, body: { error: 'conflict, both changed: Stripe, GitHub' } });
    await s.save({} as never);
    expect(toasts[0][0]).toBe(
      'Conflict: you and another writer both changed Stripe, GitHub. Reload before saving.',
    );
    expect(s.lastSaveFailed()).toBe(true);
    reply({ status: 409, body: undefined });
    await s.save({} as never);
    expect(toasts[1][0]).toContain('vault changed on server since you loaded it');
  });

  it('other failures name the status and the reason, or the network problem', async () => {
    const s = await authed();
    reply({ status: 500, body: { error: 'disk full' } });
    await s.save({} as never);
    expect(toasts[0]).toEqual(['Remote save failed (500): disk full', 'err']);
    reply({ status: 503, body: undefined });
    await s.save({} as never);
    expect(toasts[1][0]).toBe('Remote save failed (503)');
    reply({ throws: new Error('connection reset') });
    await s.save({} as never);
    expect(toasts[2][0]).toBe('Remote save failed: connection reset');
    reply({ throws: 'weird' as unknown as Error });
    await s.save({} as never);
    expect(toasts[3][0]).toBe('Remote save failed: network error');
    expect(s.lastSaveFailed()).toBe(true);
    reply({ body: {} });
    await s.save({} as never);
    expect(s.lastSaveFailed()).toBe(false);
  });

  it('saving without a session fails without a request', async () => {
    const s = new RemoteVaultStore(BASE);
    await s.save({} as never);
    expect(s.lastSaveFailed()).toBe(true);
    expect(requests).toHaveLength(0);
  });

  it('a delta goes as a PATCH and is "refused" (so the caller saves whole) for 403, 404 and 405', async () => {
    const s = await authed();
    reply({ body: {}, headers: { etag: 'v3' } });
    expect(await s.saveRows({ put: [], delete: ['x'] })).toBe(true);
    expect(requests[0]).toMatchObject({ method: 'PATCH', body: { put: [], delete: ['x'] } });
    for (const status of [403, 404, 405]) {
      reply({ status, body: {} });
      expect(await s.saveRows({}), String(status)).toBe(false);
    }
    expect(toasts).toEqual([]);
    reply({ status: 500, body: {} });
    expect(await s.saveRows({})).toBe(true);
    expect(s.lastSaveFailed()).toBe(true);
  });

  it('a copy known to be stale cannot be saved over the store', async () => {
    const s = await authed();
    s.markStale();
    reply({ body: {} });
    await s.save({} as never);
    expect(requests[0].headers['If-Match']).toBe('stale-copy');
  });
});

describe('authenticated calls for the management panels', () => {
  it('api returns the parsed answer, null for no content, and throws with the server’s reason', async () => {
    const s = await authed();
    reply({ body: [1] });
    expect(await s.api('/api/users')).toEqual([1]);
    reply({ status: 204, body: undefined });
    expect(await s.api('/api/users/x', 'DELETE')).toBeNull();
    reply({ status: 200, body: undefined });
    expect(await s.api('/api/users')).toBeNull();
    reply({ status: 403, body: { error: 'owner only' } });
    await expect(s.api('/api/users')).rejects.toThrow('owner only');
    reply({ status: 500, body: undefined });
    await expect(s.api('/api/users')).rejects.toThrow('Server error 500');
    reply({ body: {} });
    await s.api('/api/users', 'POST', { a: 1 });
    const post = requests[requests.length - 1];
    expect(post.headers['Content-Type']).toBe('application/json');
    expect(post.body).toEqual({ a: 1 });
    expect(requests[0].headers['Content-Type']).toBeUndefined();
  });

  it('registry and node calls hand back the status as part of the answer', async () => {
    const s = await authed();
    reply({ status: 429, body: { error: 'slow down' } });
    expect(await s.uidRequest('POST', '/api/uid/mint', { length: 8 })).toEqual({
      status: 429,
      body: { error: 'slow down' },
    });
    expect(requests[0].body).toEqual({ length: 8 });
    reply({ status: 204, body: undefined });
    expect(await s.nodesRequest('DELETE', '/api/nodes/x')).toEqual({ status: 204, body: null });
    expect(requests[1].headers['Content-Type']).toBeUndefined();
    reply({ status: 404, body: undefined });
    expect((await s.uidRequest('GET', '/api/uid/stats')).status).toBe(404);
  });

  it('calendar feeds: mint returns a full URL, list keeps only well-formed rows, revoke reports success', async () => {
    const s = await authed();
    reply({ body: { path: '/cal/abc.ics' } });
    expect(await s.createCalendarFeed('UnENVerse', ['created'], true)).toBe(`${BASE}/cal/abc.ics`);
    expect(requests[0].body).toEqual({
      name: 'UnENVerse',
      kinds: ['created'],
      include_account_names: true,
    });
    reply({ body: { path: 5 } });
    expect(await s.createCalendarFeed('n', [], false)).toBeNull();
    reply({ status: 500, body: {} });
    expect(await s.createCalendarFeed('n', [], false)).toBeNull();
    reply({ throws: new Error('x') });
    expect(await s.createCalendarFeed('n', [], false)).toBeNull();
    reply({
      body: {
        feeds: [
          { id: 'a', name: 'N', kinds: ['created'], revoked_at: null },
          { id: 'b', kinds: [1], revoked_at: 7 },
          { name: 'no id' },
          'junk',
        ],
      },
    });
    expect(await s.listCalendarFeeds()).toEqual([
      { id: 'a', name: 'N', kinds: ['created'], revoked_at: null },
      { id: 'b' },
    ]);
    reply({ body: { feeds: 'x' } });
    expect(await s.listCalendarFeeds()).toEqual([]);
    reply({ status: 500, body: {} });
    expect(await s.listCalendarFeeds()).toEqual([]);
    reply({ throws: new Error('x') });
    expect(await s.listCalendarFeeds()).toEqual([]);
    reply({ status: 204 });
    expect(await s.revokeCalendarFeed('a/b')).toBe(true);
    expect(requests[requests.length - 1].url).toBe(`${BASE}/api/calendar/feeds/a%2Fb`);
    reply({ status: 404 });
    expect(await s.revokeCalendarFeed('a')).toBe(false);
    reply({ throws: new Error('x') });
    expect(await s.revokeCalendarFeed('a')).toBe(false);
  });
});

describe('an https server reached from the desktop app', () => {
  it('goes through the pinned proxy command with the fingerprint, and returns its status, etag and merge marker', async () => {
    const invoke = vi.fn(async (cmd: string, _args?: unknown): Promise<any> => {
      if (cmd === 'remote_request')
        return { status: 200, body: '{"token":"PT"}', etag: 'e1', merged: true };
      return null;
    });
    (window as any).__TAURI__ = { core: { invoke } };
    const s = new RemoteVaultStore('https://vault.test', 'aabb');
    expect(await s.unlock('pw')).toBe(true);
    expect(invoke).toHaveBeenCalledWith('remote_request', {
      url: 'https://vault.test/api/unlock',
      method: 'POST',
      headersJson: JSON.stringify({ 'Content-Type': 'application/json' }),
      body: JSON.stringify({ password: 'pw' }),
      fingerprint: 'aabb',
    });
    expect(requests).toHaveLength(0);
    invoke.mockResolvedValueOnce({ status: 200, body: '{}', etag: 'e2', merged: true });
    await s.save({} as never);
    expect(s.takeMerged()).toBe(true);
    invoke.mockResolvedValueOnce({ status: 200, body: '{}' });
    await s.save({} as never);
    expect(s.takeMerged()).toBe(false);
    invoke.mockResolvedValueOnce({ status: 200, body: '{}' });
    await s.save({} as never);
    const last = invoke.mock.calls[invoke.mock.calls.length - 1][1] as {
      headersJson: string;
      fingerprint: unknown;
    };
    expect(JSON.parse(last.headersJson)['If-Match']).toBeUndefined();
    const unpinned = new RemoteVaultStore('https://other.test');
    invoke.mockResolvedValueOnce({ status: 200, body: '{"unlocked":true}' });
    await unpinned.getStatus();
    expect(
      (invoke.mock.calls[invoke.mock.calls.length - 1][1] as { fingerprint: unknown }).fingerprint,
    ).toBeNull();
    invoke.mockResolvedValueOnce({ status: 500, body: '{"error":"bad"}' });
    await expect(unpinned.unlock('x')).rejects.toThrow('bad');
  });

  it('plain http still uses ordinary fetch even in the desktop app', async () => {
    (window as any).__TAURI__ = { core: { invoke: vi.fn() } };
    const s = new RemoteVaultStore(BASE);
    reply({ body: { token: 'T' } });
    await s.unlock('pw');
    expect(requests).toHaveLength(1);
  });
});

describe('the desktop store', () => {
  let invoke: ReturnType<typeof vi.fn>;
  const store = () => {
    invoke = vi.fn(async () => null);
    (window as any).__TAURI__ = { core: { invoke } };
    return new TauriVaultStore();
  };

  it('maps each action to its command', async () => {
    const s = store();
    invoke.mockResolvedValueOnce(true);
    expect(await s.unlock('pw')).toBe(true);
    expect(invoke).toHaveBeenLastCalledWith('unlock_vault', { password: 'pw' });
    await s.lock();
    expect(invoke).toHaveBeenLastCalledWith('lock_vault', undefined);
    await s.isUnlocked();
    expect(invoke).toHaveBeenLastCalledWith('vault_is_unlocked', undefined);
    await s.exists();
    expect(invoke).toHaveBeenLastCalledWith('vault_exists', undefined);
    await s.reset();
    expect(invoke).toHaveBeenLastCalledWith('reset_vault', undefined);
    invoke.mockResolvedValueOnce('/home/me/vault.db');
    expect(await s.vaultFilePath()).toBe('/home/me/vault.db');
    invoke.mockRejectedValueOnce(new Error('x'));
    expect(await s.vaultFilePath()).toBe('');
    expect(s.isRemote).toBe(false);
    expect(s.vaultId).toBe('local-native');
  });

  it('loads data and remembers its version; an empty vault is null', async () => {
    const s = store();
    invoke.mockResolvedValueOnce({ data: { api_keys: [] }, version: 'v1' });
    expect(await s.load()).toEqual({ api_keys: [] });
    invoke.mockResolvedValueOnce('v2');
    await s.save({ api_keys: [] } as never);
    expect(invoke).toHaveBeenLastCalledWith('save_vault', {
      data: { api_keys: [] },
      expectVersion: 'v1',
    });
    invoke.mockResolvedValueOnce(null);
    expect(await s.load()).toBeNull();
    invoke.mockResolvedValueOnce('v3');
    await s.save({} as never);
    expect(invoke).toHaveBeenLastCalledWith('save_vault', { data: {}, expectVersion: null });
  });

  it('a save returning +merged says the copy is behind, and keeps the bare version', async () => {
    const s = store();
    invoke.mockResolvedValueOnce('v5+merged');
    await s.save({} as never);
    expect(s.takeMerged()).toBe(true);
    expect(s.takeMerged()).toBe(false);
    invoke.mockResolvedValueOnce('v6');
    await s.save({} as never);
    expect(invoke).toHaveBeenLastCalledWith('save_vault', { data: {}, expectVersion: 'v5' });
  });

  it('a version conflict becomes a VaultConflictError naming the entries', async () => {
    const s = store();
    invoke.mockRejectedValueOnce(new Error('VAULT_CONFLICT both changed: Stripe, GitHub'));
    await expect(s.save({} as never)).rejects.toMatchObject({
      name: 'VaultConflictError',
      entries: 'Stripe, GitHub',
    });
    invoke.mockRejectedValueOnce('VAULT_CONFLICT');
    const err = await s.saveRows({}).catch((e: unknown) => e);
    expect(err).toBeInstanceOf(VaultConflictError);
    expect((err as VaultConflictError).entries).toBeNull();
    invoke.mockRejectedValueOnce(new Error('disk full'));
    await expect(s.save({} as never)).rejects.toThrow('disk full');
    invoke.mockRejectedValueOnce(new Error('disk full'));
    await expect(s.saveRows({})).rejects.toThrow('disk full');
  });

  it('saves a delta with the same version check, and a stale copy cannot match any real version', async () => {
    const s = store();
    invoke.mockResolvedValueOnce({ data: {}, version: 'v1' });
    await s.load();
    invoke.mockResolvedValueOnce('v2');
    expect(await s.saveRows({ put: [], delete: ['x'] })).toBe(true);
    expect(invoke).toHaveBeenLastCalledWith('save_vault_rows', {
      patch: { put: [], delete: ['x'] },
      expectVersion: 'v1',
    });
    s.markStale();
    invoke.mockResolvedValueOnce('v3');
    await s.save({} as never);
    expect(invoke).toHaveBeenLastCalledWith('save_vault', {
      data: {},
      expectVersion: 'stale-copy',
    });
  });
});

describe('conflict messages', () => {
  it('lists the entries after "both changed:", or none', () => {
    expect(conflictEntries('x both changed: A, B ')).toBe('A, B');
    expect(conflictEntries('nothing here')).toBeNull();
    expect(conflictEntries('both changed: ')).toBeNull();
  });
});
