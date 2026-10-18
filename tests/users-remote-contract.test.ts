// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
/**
 * The contract between the users panel and a remote server: every management
 * command maps to exactly one owner-only REST route, method and body. An
 * answer of nothing is "no result", never a crash, and an operation the remote
 * does not know is refused by name.
 */
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { st, RemoteVaultStore } from '../src/ts/state';
import { invoke } from '../src/ts/users';
import { loadRealIndexHtml, resetState } from './helpers';

let calls: [string, string | undefined, unknown][] = [];
let answer: unknown = null;

beforeEach(() => {
  loadRealIndexHtml();
  resetState(st);
  calls = [];
  answer = null;
  const store = new RemoteVaultStore('http://localhost:1');
  store.api = vi.fn(async (p: string, m?: string, b?: unknown) => {
    calls.push([p, m, b]);
    return answer;
  }) as never;
  st.store = store;
});

const CASES: [string, Record<string, unknown>, string, string | undefined, unknown][] = [
  ['list_users', {}, '/api/users', undefined, undefined],
  [
    'create_user',
    { username: 'bob', password: 'pw' },
    '/api/users',
    'POST',
    { username: 'bob', password: 'pw' },
  ],
  ['create_user', { username: 'bob' }, '/api/users', 'POST', { username: 'bob', password: null }],
  ['delete_user', { userId: 'u1' }, '/api/users/u1', 'DELETE', undefined],
  [
    'set_strict_write',
    { subjectKind: 'class', subjectId: 'c1', strict: true },
    '/api/classes/c1/strict-write',
    'PUT',
    { strict: true },
  ],
  [
    'set_strict_write',
    { subjectKind: 'user', subjectId: 'u1' },
    '/api/users/u1/strict-write',
    'PUT',
    { strict: false },
  ],
  [
    'set_strict_write',
    { subjectKind: 'user' },
    '/api/users//strict-write',
    'PUT',
    { strict: false },
  ],
  [
    'rename_user',
    { userId: 'u1', newUsername: 'x' },
    '/api/users/u1/rename',
    'PUT',
    { username: 'x' },
  ],
  [
    'set_user_password',
    { userId: 'u1', password: 'p' },
    '/api/users/u1/password',
    'PUT',
    { password: 'p' },
  ],
  ['set_user_password', { userId: 'u1' }, '/api/users/u1/password', 'PUT', { password: null }],
  [
    'assign_user_class',
    { userId: 'u1', classId: 'c1' },
    '/api/users/u1/class',
    'PUT',
    { class_id: 'c1' },
  ],
  ['assign_user_class', { userId: 'u1' }, '/api/users/u1/class', 'PUT', { class_id: null }],
  ['list_user_tokens', { userId: 'u1' }, '/api/users/u1/tokens', undefined, undefined],
  [
    'create_user_token',
    { userId: 'u1', description: 'ci' },
    '/api/users/u1/tokens',
    'POST',
    { description: 'ci' },
  ],
  ['create_user_token', { userId: 'u1' }, '/api/users/u1/tokens', 'POST', { description: '' }],
  [
    'revoke_user_token',
    { userId: 'u1', tokenId: 't1' },
    '/api/users/u1/tokens/t1',
    'DELETE',
    undefined,
  ],
  ['revoke_user_token', { userId: 'u1' }, '/api/users/u1/tokens/', 'DELETE', undefined],
  ['totp_status', { userId: 'u1' }, '/api/users/u1/totp', undefined, undefined],
  ['totp_enroll', { userId: 'u1' }, '/api/users/u1/totp/enroll', 'POST', undefined],
  [
    'totp_confirm',
    { userId: 'u1', code: '123456' },
    '/api/users/u1/totp/confirm',
    'POST',
    { code: '123456' },
  ],
  ['totp_disable', { userId: 'u1' }, '/api/users/u1/totp', 'DELETE', undefined],
  ['get_user_permissions', { userId: 'u1' }, '/api/users/u1/permissions', undefined, undefined],
  [
    'set_user_permissions',
    { userId: 'u1', permissions: { read: 'a', write: 'b' } },
    '/api/users/u1/permissions',
    'PUT',
    { read: 'a', write: 'b' },
  ],
  ['list_user_classes', {}, '/api/classes', undefined, undefined],
  [
    'create_user_class',
    { name: 'ops', description: 'd', capManageUsers: true },
    '/api/classes',
    'POST',
    {
      name: 'ops',
      description: 'd',
      cap_manage_users: true,
      cap_manage_classes: false,
      cap_delete_projects: false,
    },
  ],
  [
    'update_user_class',
    { classId: 'c1', name: 'ops', capManageClasses: true, capDeleteProjects: true },
    '/api/classes/c1',
    'PUT',
    {
      name: 'ops',
      description: '',
      cap_manage_users: false,
      cap_manage_classes: true,
      cap_delete_projects: true,
    },
  ],
  ['delete_user_class', { classId: 'c1' }, '/api/classes/c1', 'DELETE', undefined],
  ['get_class_permissions', { classId: 'c1' }, '/api/classes/c1/permissions', undefined, undefined],
  [
    'set_class_permissions',
    { classId: 'c1', permissions: { read: '', write: 'x' } },
    '/api/classes/c1/permissions',
    'PUT',
    { read: '', write: 'x' },
  ],
];

describe('the remote users contract', () => {
  it.each(CASES)('%s %j → %s %s', async (cmd, args, path, method, body) => {
    answer = { ok: true };
    await invoke(cmd, args);
    expect(calls).toEqual([[path, method, body]]);
  });

  it.each(CASES.map((c) => c[0]).filter((c, i, a) => a.indexOf(c) === i))(
    '%s: a server that answers nothing gives no result',
    async (cmd) => {
      answer = null;
      const result = await invoke(cmd, {
        userId: 'u',
        classId: 'c',
        subjectId: 's',
        permissions: {},
      });
      expect(result === undefined || result === false).toBe(true);
    },
  );

  it('a confirmation answer is read as enabled or not', async () => {
    answer = { enabled: true };
    expect(await invoke('totp_confirm', { userId: 'u1', code: '1' })).toBe(true);
    answer = {};
    expect(await invoke('totp_confirm', { userId: 'u1', code: '1' })).toBe(false);
  });

  it('refuses an operation the remote does not have, by name', async () => {
    await expect(invoke('format_disk')).rejects.toThrow('Unsupported remote user op: format_disk');
  });

  it('without a remote it goes to the desktop when there is one, and gives nothing in a browser', async () => {
    st.store = {} as never;
    expect(await invoke('list_users')).toBeUndefined();
    (window as any).__TAURI__ = { core: { invoke: async (c: string) => `desktop:${c}` } };
    try {
      expect(await invoke('list_users')).toBe('desktop:list_users');
    } finally {
      delete (window as any).__TAURI__;
    }
  });
});
