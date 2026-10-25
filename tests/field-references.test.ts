// SPDX-License-Identifier: PolyForm-Strict-1.0.0
// Copyright Jubair Hasan.
/**
 * `${...}` references: how a placeholder in a config becomes a value, and how a
 * config file is written from chunks.
 *
 * A reference that resolves to the wrong value deploys the wrong secret; one
 * that cannot resolve must say so rather than guess. The cases are the ones the
 * design notes call out: ambiguity is refused, roles beat aliases, cycles stop,
 * public and secret fields are told apart, and an exported file never contains
 * a literal `${...}`.
 */
import { describe, it, expect, beforeEach } from 'vitest';
import { st, Settings } from '../src/ts/state';
import {
  resolveFieldRef,
  getEntryFieldValue,
  isEntryFieldPublic,
  findEntryByRef,
  exportApache,
  exportHaproxy,
  exportAnsible,
  exportPostgres,
  activeChunks,
} from '../src/ts/chunk-ops';
import { makeEntry, makeProject, makeVault, resetState } from './helpers';

const entry = (o: object) => makeEntry(o as never);
const chunk = (o: object) =>
  ({ id: crypto.randomUUID(), fields: [], ...o }) as never as import('../src/ts/types').SecretChunk;
const f = (key: string, value: string, field_type = 'var') => ({ key, value, field_type });

function vault(entries: object[], projects: object[] = []) {
  st.vault = makeVault({
    api_keys: entries.map(entry),
    projects: [
      makeProject({ id: 'Universal', name: 'Universal' }),
      ...projects.map((p) => makeProject(p as never)),
    ],
  });
}

beforeEach(() => {
  resetState(st);
  Settings.set('envCopyField', 'api_key');
});

describe('plain values and entry references', () => {
  it('a value that is not a reference comes back untouched', () => {
    expect(resolveFieldRef('plain')).toEqual({
      resolved: 'plain',
      refName: null,
      unresolved: false,
      source: null,
    });
    expect(resolveFieldRef('${open')).toMatchObject({ resolved: '${open', refName: null });
  });

  it('${Provider} is the primary value, ${Provider/field} a named field', () => {
    vault([
      {
        id: 'a',
        provider: 'Stripe',
        api_key: 'sk_1',
        api_secret: 'whsec_1',
        username: 'me',
        api_url: 'https://api.stripe',
        key_id: 'prod',
      },
    ]);
    expect(resolveFieldRef('${Stripe}').resolved).toBe('sk_1');
    expect(resolveFieldRef('${Stripe/key}')).toMatchObject({
      resolved: 'sk_1',
      source: 'vault',
      unresolved: false,
    });
    expect(resolveFieldRef('${Stripe/token}').resolved).toBe('sk_1');
    expect(resolveFieldRef('${Stripe/secret}').resolved).toBe('whsec_1');
    expect(resolveFieldRef('${Stripe/username}').resolved).toBe('me');
    expect(resolveFieldRef('${Stripe/api_url}').resolved).toBe('https://api.stripe');
  });

  it('an unknown entry or an empty field is unresolved, not empty text', () => {
    vault([{ id: 'a', provider: 'Stripe', api_key: 'sk_1' }]);
    expect(resolveFieldRef('${Nope}')).toMatchObject({ resolved: null, unresolved: true });
    expect(resolveFieldRef('${Nope/key}')).toMatchObject({
      resolved: null,
      unresolved: true,
      source: null,
    });
    expect(resolveFieldRef('${Stripe/username}')).toMatchObject({
      resolved: null,
      unresolved: true,
      source: 'vault',
    });
  });

  it('falls through to a named variable when the built-in field is empty', () => {
    vault([
      {
        id: 'a',
        provider: 'Spotify',
        api_key: 'k',
        extra_vars: [{ key: 'ID', value: 'client-123' }],
      },
    ]);
    expect(resolveFieldRef('${Spotify/ID}').resolved).toBe('client-123');
  });

  it('a declared role wins over the alias table', () => {
    vault([
      {
        id: 'a',
        provider: 'Spotify',
        api_key: 'the-client-id',
        key_id: 'label',
        primary_role: 'id',
        secret_role: 'auth_token',
        api_secret: 'the-token',
      },
    ]);
    expect(resolveFieldRef('${Spotify/ID}').resolved).toBe('the-client-id');
    expect(resolveFieldRef('${Spotify/auth-token}').resolved).toBe('the-token');
  });

  it('the legacy Provider_keyid spelling works, but only when unambiguous', () => {
    vault([
      { id: 'a', provider: 'AWS', key_id: 'prod', api_key: 'prod-key' },
      { id: 'b', provider: 'AWS', key_id: 'dev', api_key: 'dev-key' },
    ]);
    expect(resolveFieldRef('${AWS_prod}').resolved).toBe('prod-key');
    expect(resolveFieldRef('${AWS_dev/key}').resolved).toBe('dev-key');
    expect(findEntryByRef('AWS')?.key_id).toBe('prod');
  });

  it('a name that two entries could claim is refused rather than guessed', () => {
    vault([
      { id: 'a', provider: 'SPOTIFY', key_id: 'V2', api_key: 'one' },
      { id: 'b', provider: 'SPOTIFY', version: '2', api_key: 'two' },
    ]);
    expect(findEntryByRef('SPOTIFY_V2')).toBeUndefined();
    expect(resolveFieldRef('${SPOTIFY_V2}')).toMatchObject({ resolved: null, unresolved: true });
  });

  it('a generated-name reference finds the entry whose generated name it is', () => {
    vault([{ id: 'a', provider: 'Spotify', version: '2', api_key: 'v2-key' }]);
    expect(findEntryByRef('SPOTIFY_V2')?.id).toBe('a');
    expect(resolveFieldRef('${SPOTIFY_V2}').resolved).toBe('v2-key');
  });

  it('with the env-copy field set, the plain form follows it and falls back to the key', () => {
    vault([
      { id: 'a', provider: 'Svc', api_key: 'KEY', api_secret: 'SECRET' },
      { id: 'b', provider: 'Bare', api_key: 'K2' },
    ]);
    Settings.set('envCopyField', 'api_secret');
    expect(resolveFieldRef('${Svc}', true).resolved).toBe('SECRET');
    expect(resolveFieldRef('${Svc}', false).resolved).toBe('KEY');
    expect(resolveFieldRef('${Bare}', true).resolved).toBe('K2');
  });

  it('falls back to a variable of an env-file chunk of that name', () => {
    vault(
      [],
      [
        {
          id: 'p',
          name: 'P',
          chunks: [chunk({ name: 'app', chunk_type: 'env_file', fields: [f('PORT', '8080')] })],
        },
      ],
    );
    expect(resolveFieldRef('${PORT}')).toMatchObject({ resolved: '8080', source: 'env_file' });
    expect(resolveFieldRef('${MISSING}')).toMatchObject({ unresolved: true });
  });
});

describe('fields that must not be reachable by reference', () => {
  it('history, categories, project ids and the like are never returned', () => {
    const e = entry({
      provider: 'S',
      api_key: 'k',
      version_history: [{ value: 'old' }],
      categories: ['x'],
      projectIds: ['p'],
    });
    for (const field of ['version_history', 'categories', 'projectIds'])
      expect(getEntryFieldValue(e as never, field)).toBeUndefined();
  });
});

describe('what counts as public', () => {
  it('the key and secret are private unless marked, identifiers and addresses are public, and unknown fields fail closed', () => {
    const e = entry({
      provider: 'S',
      api_key: 'k',
      api_secret: 's',
      key_id: 'id',
      username: 'u',
      api_url: 'x',
      email: 'e',
      mount_path: '/m',
      extra_vars: [
        { key: 'region', value: 'eu', public: true },
        { key: 'token', value: 't' },
        { key: 'flag', value: 'on', secret: false },
      ],
    });
    const pub = (field: string) => isEntryFieldPublic(e as never, field);
    expect(pub('key')).toBe(false);
    expect(pub('secret')).toBe(false);
    expect(pub('key_id')).toBe(true);
    for (const field of ['username', 'api_url', 'email', 'mount_path', 'expires_at'])
      expect(pub(field), field).toBe(true);
    expect(pub('region')).toBe(true);
    expect(pub('flag')).toBe(true);
    expect(pub('token')).toBe(false);
    expect(pub('whatever')).toBe(false);
  });

  it('marking the key or secret public makes it public, including through a declared role', () => {
    const e = entry({
      provider: 'S',
      api_key: 'k',
      api_secret: 's',
      primary_public: true,
      secret_public: true,
      primary_role: 'id',
      secret_role: 'tok',
    });
    expect(isEntryFieldPublic(e as never, 'key')).toBe(true);
    expect(isEntryFieldPublic(e as never, 'secret')).toBe(true);
    expect(isEntryFieldPublic(e as never, 'ID')).toBe(true);
    expect(isEntryFieldPublic(e as never, 'tok')).toBe(true);
    const priv = entry({
      provider: 'S',
      api_key: 'k',
      api_secret: 's',
      primary_role: 'id',
      secret_role: 'tok',
    });
    expect(isEntryFieldPublic(priv as never, 'ID')).toBe(false);
    expect(isEntryFieldPublic(priv as never, 'tok')).toBe(false);
  });
});

describe('references between chunks', () => {
  const projects = [
    {
      id: 'p',
      name: 'P',
      chunks: [
        chunk({
          name: 'db',
          chunk_type: 'env_file',
          fields: [
            f('HOST', 'db.local'),
            f('ALIAS', '${chunk:db/HOST}'),
            f('LOOP', '${chunk:db/LOOP}'),
            f('KEY', '${Stripe/key}'),
            f('BAD', '${chunk:db/NOPE}'),
          ],
        }),
        chunk({ name: 'web', chunk_type: 'env_file', fields: [f('BACK', '${chunk:db/ALIAS}')] }),
      ],
    },
  ];

  it('${chunk:Name/Field} reads a field of any chunk, following references in it', () => {
    vault([{ id: 'a', provider: 'Stripe', api_key: 'sk_1' }], projects);
    expect(resolveFieldRef('${chunk:db/HOST}')).toMatchObject({
      resolved: 'db.local',
      source: 'chunk',
    });
    expect(resolveFieldRef('${chunk:db/ALIAS}').resolved).toBe('db.local');
    expect(resolveFieldRef('${chunk:web/BACK}').resolved).toBe('db.local');
    expect(resolveFieldRef('${chunk:db/KEY}')).toMatchObject({ resolved: 'sk_1', source: 'vault' });
  });

  it('a reference to itself stops instead of looping; a missing chunk or field is unresolved', () => {
    vault([], projects);
    expect(resolveFieldRef('${chunk:db/LOOP}').unresolved).toBe(true);
    expect(resolveFieldRef('${chunk:nope/HOST}')).toMatchObject({
      resolved: null,
      unresolved: true,
    });
    expect(resolveFieldRef('${chunk:db/MISSING}')).toMatchObject({
      resolved: null,
      unresolved: true,
    });
    expect(resolveFieldRef('${chunk:nofieldpart}')).toMatchObject({
      resolved: null,
      unresolved: true,
    });
    expect(resolveFieldRef('${chunk:db/BAD}').unresolved).toBe(true);
  });
});

describe('bundle references', () => {
  const setup = () =>
    vault([
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
        api_key: 'bot-token',
        bundle_id: 'bun',
        bundle_slot: 'bot',
        extra_vars: [{ key: 'app', value: '123' }],
      },
      { id: 'm2', provider: 'Hook', api_key: 'hook-url', bundle_id: 'bun', bundle_slot: 'hook' },
    ]);

  it('${bundle:Name} is the primary member, /slot a member, /slot/field one of its fields, /local a bundle variable', () => {
    setup();
    expect(resolveFieldRef('${bundle:Discord}').resolved).toBe('bot-token');
    expect(resolveFieldRef('${bundle:Discord/hook}').resolved).toBe('hook-url');
    expect(resolveFieldRef('${bundle:Discord/bot/app}').resolved).toBe('123');
    expect(resolveFieldRef('${bundle:Discord/prefix}').resolved).toBe('!');
    expect(resolveFieldRef('${bundle:discord/bot}').resolved).toBe('bot-token');
  });

  it('an unknown bundle, slot or field is unresolved, and a bundle with no id or primary cannot be read', () => {
    setup();
    for (const ref of [
      '${bundle:Nope}',
      '${bundle:Discord/ghost}',
      '${bundle:Discord/bot/ghost}',
      '${bundle:Discord/ghost/app}',
    ]) {
      expect(resolveFieldRef(ref), ref).toMatchObject({ resolved: null, unresolved: true });
    }
    st.vault.api_keys[0].bundle_primary = 'gone';
    expect(resolveFieldRef('${bundle:Discord}').unresolved).toBe(true);
    st.vault.api_keys.push(entry({ provider: 'Discord', secretType: 'bundle' }));
    expect(resolveFieldRef('${bundle:Discord/bot}').unresolved).toBe(true);
  });
});

describe('project bindings', () => {
  const bound = (binding: object, extra: object[] = []) =>
    vault(
      [
        {
          id: 'e1',
          provider: 'RealName',
          api_key: 'bound-key',
          api_secret: 'bound-secret',
          extra_vars: [{ key: 'region', value: 'eu' }],
        },
        ...extra,
      ],
      [{ id: 'p', name: 'P', bindings: [binding] }],
    );

  it('a binding points a name at an entry by id, so a rename of the provider does not break it', () => {
    bound({ name: 'DB', entry_id: 'e1' });
    expect(resolveFieldRef('${DB}')).toMatchObject({ resolved: 'bound-key', source: 'vault' });
  });

  it('a binding can name a field, follow the env-copy field setting, or point at a deleted entry', () => {
    bound({ name: 'DB', entry_id: 'e1', field: 'region' });
    expect(resolveFieldRef('${DB}').resolved).toBe('eu');
    bound({ name: 'DB', entry_id: 'e1', field: 'missing' });
    expect(resolveFieldRef('${DB}').resolved).toBeNull();
    bound({ name: 'DB', entry_id: 'e1' });
    Settings.set('envCopyField', 'api_secret');
    expect(resolveFieldRef('${DB}', true).resolved).toBe('bound-secret');
    expect(resolveFieldRef('${DB}', false).resolved).toBe('bound-key');
    bound({ name: 'DB', entry_id: 'gone' });
    expect(resolveFieldRef('${DB}')).toMatchObject({ resolved: null, unresolved: true });
  });

  it('two bindings with one name are refused rather than picking one', () => {
    vault(
      [
        { id: 'e1', provider: 'A', api_key: 'one' },
        { id: 'e2', provider: 'B', api_key: 'two' },
      ],
      [
        { id: 'p', name: 'P', bindings: [{ name: 'DB', entry_id: 'e1' }] },
        { id: 'q', name: 'Q', bindings: [{ name: 'DB', entry_id: 'e2' }] },
      ],
    );
    expect(resolveFieldRef('${DB}')).toMatchObject({ resolved: null, unresolved: true });
  });
});

describe('writing config files from chunks', () => {
  const proj = (chunks: unknown[]) => makeProject({ id: 'p', name: 'P', chunks } as never);

  it('disabled chunks are left out of every export', () => {
    expect(
      activeChunks(
        proj([
          chunk({ name: 'on', chunk_type: 'x' }),
          chunk({ name: 'off', chunk_type: 'x', disabled: true }),
        ]),
      ).map((c) => c.name),
    ).toEqual(['on']);
    expect(activeChunks(makeProject({ id: 'p', name: 'P' } as never))).toEqual([]);
  });

  it('Apache: a vhost on 80, or 443 when SSLEngine is set, with references resolved and empty values skipped', () => {
    vault([{ id: 'a', provider: 'Cert', api_key: '/etc/ssl/cert.pem' }]);
    const out = exportApache(
      proj([
        chunk({
          name: 'v1',
          chunk_type: 'apache_vhost',
          fields: [f('ServerName', 'a.test'), f('DocumentRoot', '')],
        }),
        chunk({
          name: 'v2',
          chunk_type: 'apache_vhost',
          fields: [f('SSLEngine', 'on'), f('SSLCertificateFile', '${Cert}')],
        }),
        chunk({
          name: 'd',
          chunk_type: 'apache_directory',
          fields: [f('path', '/var/www'), f('Require', 'all granted')],
        }),
        chunk({ name: 'd2', chunk_type: 'apache_directory', fields: [f('Require', 'local')] }),
        chunk({
          name: 'off',
          chunk_type: 'apache_vhost',
          disabled: true,
          fields: [f('ServerName', 'hidden.test')],
        }),
      ]),
    );
    expect(out).toContain('<VirtualHost *:80>\n    ServerName a.test\n</VirtualHost>');
    expect(out).toContain(
      '<VirtualHost *:443>\n    SSLEngine on\n    SSLCertificateFile /etc/ssl/cert.pem\n</VirtualHost>',
    );
    expect(out).toContain('<Directory /var/www>\n    Require all granted\n</Directory>');
    expect(out).toContain('<Directory />');
    expect(out).not.toContain('hidden.test');
    expect(out).not.toContain('${');
    expect(out.startsWith('# Generated by UnENVerse')).toBe(true);
  });

  it('HAProxy: global, frontend and backend sections named after their chunks', () => {
    const out = exportHaproxy(
      proj([
        chunk({ name: 'g', chunk_type: 'haproxy_global', fields: [f('maxconn', '2000')] }),
        chunk({
          name: 'web',
          chunk_type: 'haproxy_frontend',
          fields: [f('bind', '*:80'), f('default_backend', 'app')],
        }),
        chunk({
          name: 'app',
          chunk_type: 'haproxy_backend',
          fields: [f('server', 'a 1.2.3.4:80 check'), f('mode', '')],
        }),
        chunk({
          name: 'x',
          chunk_type: 'haproxy_global',
          disabled: true,
          fields: [f('nope', '1')],
        }),
      ]),
    );
    expect(out).toContain('global\n    maxconn 2000\n');
    expect(out).toContain('frontend web\n    bind *:80\n    default_backend app\n');
    expect(out).toContain('backend app\n    server a 1.2.3.4:80 check\n');
    expect(out).not.toContain('nope');
    expect(out).not.toMatch(/mode\s*$/m);
  });

  it('Ansible: variables alone are a bare mapping; with tasks they become a playbook with the variables folded in', () => {
    vault([{ id: 'a', provider: 'Db', api_key: 'pw' }]);
    const vars = chunk({
      name: 'vars-1',
      chunk_type: 'ansible_vars',
      fields: [f('db_pass', '${Db}'), f('', 'ignored')],
    });
    const bare = exportAnsible(proj([vars]));
    expect(bare).toBe('# Generated by UnENVerse\n# vars-1\ndb_pass: "pw"');
    const task = chunk({
      name: 'task-1',
      chunk_type: 'ansible_task',
      fields: [f('name', 'Say hi'), f('module', 'ansible.builtin.debug'), f('msg', 'hello "x"')],
    });
    const play = exportAnsible(proj([vars, task]));
    expect(play).toBe(
      [
        '# Generated by UnENVerse',
        '- hosts: all',
        '  vars:',
        '    # vars-1',
        '    db_pass: "pw"',
        '  tasks:',
        '    - name: Say hi',
        '      ansible.builtin.debug:',
        '        msg: "hello \\"x\\""',
      ].join('\n'),
    );
    const plain = exportAnsible(
      proj([chunk({ name: 't', chunk_type: 'ansible_task', fields: [] })]),
    );
    expect(plain).toContain('- name: t');
    expect(plain).toContain('debug:');
    expect(exportAnsible(proj([]))).toBe('# Generated by UnENVerse');
  });

  it('Postgres: one .pgpass line per connection, port defaulting to 5432', () => {
    vault([{ id: 'a', provider: 'DbPw', api_key: 's3cret' }]);
    const out = exportPostgres(
      proj([
        chunk({
          name: 'db',
          chunk_type: 'pg_connection',
          fields: [
            f('host', 'db.local'),
            f('dbname', 'app'),
            f('user', 'me'),
            f('password', '${DbPw}'),
          ],
        }),
        chunk({
          name: 'db2',
          chunk_type: 'pg_connection',
          fields: [
            f('host', 'h2'),
            f('port', '6543'),
            f('dbname', 'd'),
            f('user', 'u'),
            f('password', 'p'),
          ],
        }),
        chunk({
          name: 'off',
          chunk_type: 'pg_connection',
          disabled: true,
          fields: [f('host', 'hidden')],
        }),
      ]),
    );
    expect(out.split('\n')).toEqual([
      '# Generated by UnENVerse — .pgpass format: host:port:dbname:user:password',
      'db.local:5432:app:me:s3cret',
      'h2:6543:d:u:p',
    ]);
  });
});
