// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
/**
 * Resolving `${...}` references: roles, aliases, bundles, project bindings and
 * the legacy provider_keyid form. A reference that resolves to the wrong secret
 * is worse than one that fails, so each rule is pinned from both sides: what it
 * must find and what it must refuse to guess.
 */
import { describe, it, expect, beforeEach } from 'vitest';
import { st, Settings } from '../src/ts/state';
import {
  resolveFieldRef,
  getEntryFieldValue,
  isEntryFieldPublic,
  findEntryByRef,
} from '../src/ts/chunk-ops';
import { loadRealIndexHtml, makeEntry, makeProject, makeVault, resetState } from './helpers';

const put = (entries: object[], projects: object[] = []) => {
  st.vault = makeVault({
    api_keys: entries.map((e) => makeEntry(e as any)),
    projects: [makeProject({ id: 'Universal', name: 'Universal' }), ...(projects as any[])],
  });
};

beforeEach(() => {
  loadRealIndexHtml();
  resetState(st);
  Settings.set('envCopyField', 'api_key' as never);
});

describe('fields and roles', () => {
  const spotify = {
    id: 's',
    provider: 'Spotify',
    api_key: 'client-id-123',
    api_secret: 'client-secret-456',
    key_id: 'kid-9',
    primary_role: 'id',
    secret_role: 'secret key',
    username: 'me',
    extra_vars: [{ key: 'Region', value: 'eu' }],
  };

  it('a declared role beats the alias table; names compare without punctuation or case', () => {
    put([spotify]);
    const e = st.vault.api_keys[0];
    expect(getEntryFieldValue(e, 'ID')).toBe('client-id-123');
    expect(getEntryFieldValue(e, 'Id')).toBe('client-id-123');
    expect(getEntryFieldValue(e, 'secret-key')).toBe('client-secret-456');
    expect(getEntryFieldValue(e, 'SECRET_KEY')).toBe('client-secret-456');
  });

  it('a declared role whose value is empty falls through to the alias, then to extra variables', () => {
    put([{ ...spotify, api_key: '', key_id: 'kid-9' }]);
    const e = st.vault.api_keys[0];
    expect(getEntryFieldValue(e, 'ID')).toBe('kid-9');
    expect(getEntryFieldValue(e, 'region')).toBeUndefined();
    expect(getEntryFieldValue(e, 'Region')).toBe('eu');
  });

  it('metadata fields are never reachable, even when an extra variable has the same name', () => {
    put([{ ...spotify, extra_vars: [{ key: 'id', value: 'sneaky' }] }]);
    const e = st.vault.api_keys[0];
    for (const f of ['id', 'version_history', 'projectIds', 'categories'])
      expect(getEntryFieldValue(e, f)).toBeUndefined();
  });

  it('VALUE means the plain field lookup, not a role', () => {
    put([{ ...spotify, primary_role: 'value' }]);
    expect(getEntryFieldValue(st.vault.api_keys[0], 'VALUE')).toBeUndefined();
    expect(getEntryFieldValue(st.vault.api_keys[0], 'username')).toBe('me');
  });

  it('public-ness follows the role the name refers to, and unknown names fail closed', () => {
    put([{ ...spotify, primary_public: true }]);
    const e = st.vault.api_keys[0];
    expect(isEntryFieldPublic(e, 'ID')).toBe(true);
    expect(isEntryFieldPublic(e, 'secret key')).toBe(false);
    expect(isEntryFieldPublic(e, 'api_key')).toBe(true);
    expect(isEntryFieldPublic(e, 'Region')).toBe(false);
    expect(isEntryFieldPublic({ ...e, secret_public: true }, 'secret key')).toBe(true);
  });
});

describe('finding an entry by name', () => {
  it('prefers an exact provider, then a unique generated name, and refuses an ambiguous one', () => {
    put([
      { id: 'a', provider: 'Alpha', api_key: 'a-1' },
      { id: 'b', provider: 'Dup', api_key: 'b-1', key_id: 'one' },
      { id: 'c', provider: 'Dup', api_key: 'c-1', key_id: 'two' },
    ]);
    expect(findEntryByRef('Alpha')!.id).toBe('a');
    expect(findEntryByRef('ALPHA')!.id).toBe('a');
    expect(findEntryByRef('Dup_one')!.id).toBe('b');
    expect(findEntryByRef('Nothing')).toBeUndefined();
  });

  it('two entries generating the same name are ambiguous, and a legacy form that disagrees with the generated one is refused', () => {
    put([
      { id: 'a', provider: 'Twin', api_key: 'a-1' },
      { id: 'b', provider: 'twin', api_key: 'b-1' },
      { id: 'x', provider: 'Svc', key_id: 'prod', api_key: 'x-1' },
      { id: 'y', provider: 'Svc_prod', api_key: 'y-1', primary_role: 'ignored' },
    ]);
    expect(findEntryByRef('TWIN')).toBeUndefined();
    expect(findEntryByRef('Svc_prod')!.id).toBe('y');
  });
});

describe('bundles', () => {
  const bundle = [
    {
      id: 'bun',
      provider: 'Discord',
      secretType: 'bundle',
      api_key: '',
      bundle_primary: 'm1',
      extra_vars: [{ key: 'prefix', value: '!' }],
    },
    {
      id: 'm1',
      provider: 'Bot',
      bundle_id: 'bun',
      bundle_slot: 'bot',
      api_key: 'bot-token',
      username: 'botuser',
    },
    {
      id: 'm2',
      provider: 'Hook',
      bundle_id: 'bun',
      bundle_slot: 'hook',
      api_key: '',
      api_secret: 'hook-secret',
    },
  ];

  it('the bundle’s own name gives its primary member’s value', () => {
    put(bundle);
    expect(resolveFieldRef('${bundle:Discord}').resolved).toBe('bot-token');
    expect(resolveFieldRef('${bundle:discord}').resolved).toBe('bot-token');
  });

  it('a slot gives that member’s value, a slot and a field gives that field, and a bare name gives a local variable', () => {
    put(bundle);
    expect(resolveFieldRef('${bundle:Discord/bot}').resolved).toBe('bot-token');
    expect(resolveFieldRef('${bundle:Discord/bot/username}').resolved).toBe('botuser');
    expect(resolveFieldRef('${bundle:Discord/hook/api_secret}').resolved).toBe('hook-secret');
    expect(resolveFieldRef('${bundle:Discord/prefix}').resolved).toBe('!');
  });

  it('a member with no primary value falls back to nothing, unknown slots and fields are unresolved', () => {
    put(bundle);
    expect(resolveFieldRef('${bundle:Discord/hook}').resolved).toBeNull();
    expect(resolveFieldRef('${bundle:Discord/nope}').unresolved).toBe(true);
    expect(resolveFieldRef('${bundle:Discord/bot/nope}').unresolved).toBe(true);
    expect(resolveFieldRef('${bundle:Discord/nope/username}').unresolved).toBe(true);
    expect(resolveFieldRef('${bundle:Missing}').unresolved).toBe(true);
  });

  it('two bundles with one name, a bundle without an id, and a primary that is gone are all unresolved', () => {
    put([...bundle, { id: 'bun2', provider: 'discord', secretType: 'bundle', api_key: '' }]);
    expect(resolveFieldRef('${bundle:Discord}').unresolved).toBe(true);
    put([{ ...bundle[0], id: undefined }, ...bundle.slice(1)]);
    expect(resolveFieldRef('${bundle:Discord}').unresolved).toBe(true);
    put([{ ...bundle[0], bundle_primary: 'ghost' }, ...bundle.slice(1)]);
    expect(resolveFieldRef('${bundle:Discord}').unresolved).toBe(true);
  });

  it('copies use the configured field for the primary, falling back to the key', () => {
    put([bundle[0], { ...bundle[1], username: 'alt-user' }, bundle[2]]);
    Settings.set('envCopyField', 'username' as never);
    expect(resolveFieldRef('${bundle:Discord}', true).resolved).toBe('alt-user');
    Settings.set('envCopyField', 'api_secret' as never);
    expect(resolveFieldRef('${bundle:Discord}', true).resolved).toBe('bot-token');
    expect(resolveFieldRef('${bundle:Discord}', false).resolved).toBe('bot-token');
  });
});

describe('project bindings', () => {
  const entry = {
    id: 'e1',
    provider: 'Old name',
    api_key: 'bound-value',
    username: 'bob',
    extra_vars: [],
  };

  it('a binding keeps resolving after the provider is renamed', () => {
    put([entry], [{ id: 'p', name: 'P', bindings: [{ name: 'DB_PASSWORD', entry_id: 'e1' }] }]);
    expect(resolveFieldRef('${DB_PASSWORD}').resolved).toBe('bound-value');
    st.vault.api_keys[0].provider = 'New name';
    expect(resolveFieldRef('${DB_PASSWORD}').resolved).toBe('bound-value');
  });

  it('a binding to a field gives that field, or nothing when it is empty', () => {
    put(
      [entry],
      [
        {
          id: 'p',
          name: 'P',
          bindings: [
            { name: 'USER', entry_id: 'e1', field: 'username' },
            { name: 'NOPE', entry_id: 'e1', field: 'email' },
          ],
        },
      ],
    );
    expect(resolveFieldRef('${USER}').resolved).toBe('bob');
    expect(resolveFieldRef('${NOPE}').unresolved).toBe(true);
  });

  it('a duplicate binding, or one to a deleted entry, is unresolved rather than guessed', () => {
    put(
      [entry],
      [
        {
          id: 'p',
          name: 'P',
          bindings: [
            { name: 'DUP', entry_id: 'e1' },
            { name: 'GONE', entry_id: 'ghost' },
          ],
        },
        { id: 'q', name: 'Q', bindings: [{ name: 'DUP', entry_id: 'e1' }] },
      ],
    );
    expect(resolveFieldRef('${DUP}').unresolved).toBe(true);
    expect(resolveFieldRef('${GONE}').unresolved).toBe(true);
  });

  it('copies use the configured field for a binding, falling back to the key', () => {
    put([entry], [{ id: 'p', name: 'P', bindings: [{ name: 'B', entry_id: 'e1' }] }]);
    Settings.set('envCopyField', 'username' as never);
    expect(resolveFieldRef('${B}', true).resolved).toBe('bob');
    Settings.set('envCopyField', 'email' as never);
    expect(resolveFieldRef('${B}', true).resolved).toBe('bound-value');
    expect(resolveFieldRef('${B}', false).resolved).toBe('bound-value');
  });
});

describe('plain provider references', () => {
  it('copies use the configured field and fall back to the key when it is empty or not text', () => {
    put([{ id: 'a', provider: 'Alpha', api_key: 'k-1', username: 'alice', rotation_days: 30 }]);
    Settings.set('envCopyField', 'username' as never);
    expect(resolveFieldRef('${Alpha}', true).resolved).toBe('alice');
    Settings.set('envCopyField', 'rotation_days' as never);
    expect(resolveFieldRef('${Alpha}', true).resolved).toBe('30');
    Settings.set('envCopyField', 'email' as never);
    expect(resolveFieldRef('${Alpha}', true).resolved).toBe('k-1');
    Settings.set('envCopyField', 'categories' as never);
    expect(resolveFieldRef('${Alpha}', true).resolved).toBe('k-1');
    expect(resolveFieldRef('${Alpha}', false).resolved).toBe('k-1');
  });

  it('an entry with no value resolves to nothing without being called unresolved', () => {
    put([{ id: 'a', provider: 'Empty', api_key: '' }]);
    expect(resolveFieldRef('${Empty}')).toMatchObject({ resolved: null, unresolved: false });
  });

  it('a chunk reference without a field, or to a chunk that does not exist, is unresolved', () => {
    put([]);
    expect(resolveFieldRef('${chunk:nofield}').unresolved).toBe(true);
    expect(resolveFieldRef('${chunk:ghost/key}').unresolved).toBe(true);
  });

  it('text that is not a reference is returned as it is', () => {
    expect(resolveFieldRef('plain')).toEqual({
      resolved: 'plain',
      refName: null,
      unresolved: false,
      source: null,
    });
  });
});
