// SPDX-License-Identifier: PolyForm-Strict-1.0.0
// Copyright Jubair Hasan.
/**
 * The environment-variable names a copy or export writes, and how values are
 * quoted on the way into a .env file.
 *
 * A generated name is an interface: it is pasted into compose files, CI
 * settings and shells, so it must always be a legal identifier, never silently
 * collide with another, and survive a write-then-read round trip with its value
 * unchanged.
 */
import { describe, it, expect, beforeEach } from 'vitest';
import {
  envName,
  envVersionSegment,
  stripCookiePrefix,
  quoteEnvValue,
  unquoteEnvValue,
  disambiguateNames,
  findNameCollisions,
  namesGeneratedBy,
  primaryEnvName,
  secretEnvName,
  dotenvKey,
  entryHasPayload,
  primaryIsOptional,
  ENV_NAME_RE,
  Exporter,
  Settings,
} from '../src/ts/state';
import { makeEntry } from './helpers';

const e = (o: object = {}) => makeEntry({ provider: 'Spotify', ...o } as any);

beforeEach(() => {
  Settings.set('envCopyCase', 'upper');
  Settings.set('envIncludePrefix', false);
});

describe('building a name', () => {
  it('joins provider, key id, version, label and role in that order', () => {
    expect(envName(e({ key_id: 'prod', version: '2', label: 'game' }), { role: 'id' })).toBe(
      'SPOTIFY_PROD_V2_GAME_ID',
    );
  });

  it('tidies punctuation and never doubles or leaves dangling underscores', () => {
    expect(envName(e({ provider: '  open-ai (eu)! ' }))).toBe('OPEN_AI_EU');
    expect(envName(e({ provider: 'a___b', label: '__x__' }))).toBe('A_B_X');
  });

  it('always yields a legal identifier: a leading digit gains an underscore; nothing at all becomes UNKNOWN', () => {
    expect(envName(e({ provider: '1password' }))).toBe('_1PASSWORD');
    expect(envName(e({ provider: '!!!' }))).toBe('UNKNOWN');
    expect(envName(e({ provider: '' }))).toBe('SPOTIFY'.replace('SPOTIFY', 'UNKNOWN'));
    for (const p of ['1x', '--', 'é', 'a b', 'ok'])
      expect(ENV_NAME_RE.test(envName(e({ provider: p })))).toBe(true);
  });

  it('versions: 2, v2 and V2 are the same; 2.0 and dates keep their parts; a word keeps its first letter', () => {
    expect(envVersionSegment('2', true)).toBe('V2');
    expect(envVersionSegment('v2', true)).toBe('V2');
    expect(envVersionSegment('V2', true)).toBe('V2');
    expect(envVersionSegment('2.0', true)).toBe('V2_0');
    expect(envVersionSegment('2026-08-01', true)).toBe('V2026_08_01');
    expect(envVersionSegment('beta', true)).toBe('BETA');
    expect(envVersionSegment('vault', true)).toBe('VAULT');
    expect(envVersionSegment('  ', true)).toBe('');
    expect(envVersionSegment(null, true)).toBe('');
    expect(envVersionSegment('v2', false)).toBe('V2');
    expect(envVersionSegment('beta', false)).toBe('beta');
  });

  it('a cookie’s __Host-/__Secure- prefix is dropped, not turned into a name part', () => {
    expect(stripCookiePrefix('__Host-sid')).toBe('sid');
    expect(stripCookiePrefix('__secure-sid')).toBe('sid');
    expect(stripCookiePrefix('sid')).toBe('sid');
    expect(envName(e({ provider: 'Site' }), { role: '__Host-sid' })).toBe('SITE_SID');
  });

  it('the role "value" adds nothing', () => {
    expect(envName(e(), { role: 'Value' })).toBe('SPOTIFY');
    expect(envName(e(), { role: null })).toBe('SPOTIFY');
  });

  it('case modes: upper, lower, preserve', () => {
    const entry = e({ provider: 'My-App', key_id: 'Prod' });
    expect(envName(entry, { case: 'upper' })).toBe('MY_APP_PROD');
    expect(envName(entry, { case: 'lower' })).toBe('my_app_prod');
    expect(envName(entry, { case: 'preserve' })).toBe('My_App_Prod');
  });

  it('follows the saved case setting, and the prefix only when asked', () => {
    const entry = e({ env_prefixes: ['ND'] });
    Settings.set('envCopyCase', 'lower');
    expect(envName(entry)).toBe('spotify');
    Settings.set('envCopyCase', 'upper');
    expect(envName(entry)).toBe('SPOTIFY');
    expect(envName(entry, { includePrefix: true })).toBe('ND_SPOTIFY');
    Settings.set('envIncludePrefix', true);
    expect(envName(entry)).toBe('ND_SPOTIFY');
    expect(envName(e())).toBe('SPOTIFY');
  });

  it('the primary value keeps the bare name unless a role says otherwise; the secret is SECRET by default', () => {
    expect(primaryEnvName(e())).toBe('SPOTIFY');
    expect(primaryEnvName(e({ primary_role: 'id' }))).toBe('SPOTIFY_ID');
    expect(secretEnvName(e())).toBe('SPOTIFY_SECRET');
    expect(secretEnvName(e({ secret_role: 'auth_token' }))).toBe('SPOTIFY_AUTH_TOKEN');
    expect(dotenvKey(e())).toBe('SPOTIFY');
  });
});

describe('what an entry generates', () => {
  it('lists the primary, secret, URL and every named variable in copy order, skipping empties and unnamed rows', () => {
    const entry = e({
      api_key: 'k',
      api_secret: 's',
      api_url: 'u',
      extra_vars: [
        { key: 'region', value: 'eu' },
        { key: '', value: 'x' },
      ],
    });
    expect(namesGeneratedBy(entry).map((g) => [g.name, g.role])).toEqual([
      ['SPOTIFY', ''],
      ['SPOTIFY_SECRET', 'SECRET'],
      ['SPOTIFY_URL', 'URL'],
      ['SPOTIFY_REGION', 'region'],
    ]);
    expect(namesGeneratedBy(e({ api_key: '' }))).toEqual([]);
  });

  it('finds two entries wanting one name, case-insensitively, including within one entry', () => {
    const a = e({ id: 'a', api_key: '1' });
    const b = e({ id: 'b', provider: 'spotify', api_key: '2' });
    const own = e({
      id: 'c',
      provider: 'Own',
      primary_role: 'id',
      api_key: '3',
      extra_vars: [{ key: 'ID', value: 'x' }],
    });
    const clashes = findNameCollisions([a, b, own]);
    expect(clashes.map((c) => c.name.toUpperCase()).sort()).toEqual(['OWN_ID', 'SPOTIFY']);
    expect(
      clashes.find((c) => c.name.toUpperCase() === 'SPOTIFY')!.sources.map((s) => s.entry.id),
    ).toEqual(['a', 'b']);
    expect(findNameCollisions([a, e({ provider: 'Other', api_key: '1' })])).toEqual([]);
  });
});

describe('keeping names unique', () => {
  const g = (name: string) => ({ name, entry: e(), role: '' });

  it('the first keeps its name, later ones gain an ordinal, and nothing already in use is taken', () => {
    expect(disambiguateNames([g('A'), g('A'), g('A')])).toEqual(['A', 'A_2', 'A_3']);
    expect(disambiguateNames([g('A'), g('A'), g('A_2')])).toEqual(['A', 'A_3', 'A_2']);
    expect(disambiguateNames([g('a'), g('A')])).toEqual(['a', 'A_2']);
    expect(disambiguateNames([g('A'), g('B')])).toEqual(['A', 'B']);
    expect(disambiguateNames([])).toEqual([]);
  });
});

describe('writing values into a .env file', () => {
  it('leaves plain values bare and quotes everything else, escaping what a shell would interpret', () => {
    expect(quoteEnvValue('abc-1.2_x/y:z@w')).toBe('abc-1.2_x/y:z@w');
    expect(quoteEnvValue('')).toBe('""');
    expect(quoteEnvValue('a b')).toBe('"a b"');
    expect(quoteEnvValue('p#ss')).toBe('"p#ss"');
    expect(quoteEnvValue('has "quote" and $VAR and `cmd` and \\')).toBe(
      '"has \\"quote\\" and \\$VAR and \\`cmd\\` and \\\\"',
    );
    expect(quoteEnvValue('line1\nline2\r\tend')).toBe('"line1\\nline2\\r\\tend"');
    expect(quoteEnvValue(undefined as unknown as string)).toBe('""');
  });

  it('reading is the inverse for double quotes; single quotes are literal; bare is untouched', () => {
    expect(unquoteEnvValue('"a\\nb\\tc\\rd\\"e\\\\f\\$g"')).toBe('a\nb\tc\rd"e\\f$g');
    expect(unquoteEnvValue("'a\\nb'")).toBe('a\\nb');
    expect(unquoteEnvValue('bare')).toBe('bare');
    expect(unquoteEnvValue('"')).toBe('"');
    expect(unquoteEnvValue("'")).toBe("'");
    expect(unquoteEnvValue(undefined as unknown as string)).toBe('');
  });

  it('every awkward value survives write then read', () => {
    for (const v of [
      'sid=x; csrf=y',
      'Mozilla/5.0 (X11; Linux) "quoted"',
      'pass#word',
      ' trailing ',
      '',
      'a\\nb',
      'multi\nline\nPEM',
      '$HOME',
      '`id`',
      "it's",
      'tab\there',
      'ünï©ødé',
    ]) {
      expect(unquoteEnvValue(quoteEnvValue(v)), JSON.stringify(v)).toBe(v);
    }
  });
});

describe('exports', () => {
  const entries = [
    e({
      api_key: 'sk 1',
      account_name: 'me',
      api_url: 'https://x',
      extra_vars: [{ key: 'region', value: 'eu' }, { key: 'blank' }],
    }),
    e({ provider: 'Empty', api_key: '' }),
  ];

  it('the .env export quotes values, omits an empty primary and unnamed variables, and labels each block', () => {
    const out = Exporter.dotenv(entries);
    expect(out).toContain('# Spotify — me');
    expect(out).toContain('SPOTIFY="sk 1"');
    expect(out).toContain('SPOTIFY_URL=https://x');
    expect(out).toContain('SPOTIFY_REGION=eu');
    expect(out).toContain('SPOTIFY_BLANK=""');
    expect(out).toMatch(/# Empty$/);
    expect(out).not.toMatch(/^EMPTY=/m);
    expect(Exporter.dotenv([])).toBe('');
  });

  it('YAML keeps awkward values valid, and JSON wraps the entries', () => {
    const yaml = Exporter.yaml([e({ api_key: 'a "b"\nc: d', api_secret: 's' })]);
    expect(yaml).toContain('# UnENVerse Export');
    expect(yaml).toContain('SPOTIFY: |-');
    expect(yaml).toContain('SPOTIFY_SECRET: s');
    expect(JSON.parse(Exporter.json(entries)).api_keys).toHaveLength(2);
  });
});

describe('what counts as having a value', () => {
  it('any stored payload counts: key, secret, seed, certificate, blob, template, bundle, named variable', () => {
    expect(entryHasPayload(e({ api_key: 'k' }))).toBe(true);
    expect(entryHasPayload(e({ api_key: '', api_secret: 's' }))).toBe(true);
    expect(entryHasPayload(e({ api_key: '', totp_secret: 'JBSWY3DP' }))).toBe(true);
    expect(entryHasPayload(e({ api_key: '', certificate_data: 'pem' }))).toBe(true);
    expect(entryHasPayload(e({ api_key: '', cert_key_data: 'pem' }))).toBe(true);
    expect(entryHasPayload(e({ api_key: '', blob_ref: 'ref' }))).toBe(true);
    expect(
      entryHasPayload(e({ api_key: '', secretType: 'composite', composite_template: '{a}' })),
    ).toBe(true);
    expect(entryHasPayload(e({ api_key: '', secretType: 'composite' }))).toBe(false);
    expect(entryHasPayload(e({ api_key: '', secretType: 'bundle' }))).toBe(true);
    expect(entryHasPayload(e({ api_key: '', extra_vars: [{ key: 'A', value: '' }] }))).toBe(true);
    expect(entryHasPayload(e({ api_key: '', extra_vars: [{ key: '', value: 'x' }] }))).toBe(false);
    expect(entryHasPayload(e({ api_key: '' }))).toBe(false);
  });

  it('the form may leave the primary blank for types whose payload lives elsewhere, and not for plain keys', () => {
    for (const t of [
      'certificate',
      'file_blob',
      'composite',
      'bundle',
      'wifi',
      'passkey',
      'secure_note',
      'database',
      'oauth_client',
    ])
      expect(primaryIsOptional(e({ secretType: t })), t).toBe(true);
    expect(
      primaryIsOptional(e({ secretType: 'env_var', extra_vars: [{ key: 'A', value: '1' }] })),
    ).toBe(true);
    expect(primaryIsOptional(e({ secretType: 'env_var' }))).toBe(false);
    expect(primaryIsOptional(e({ secretType: 'api_key' }))).toBe(false);
    expect(primaryIsOptional(e({ secretType: undefined }))).toBe(false);
    expect(primaryIsOptional(e({ secretType: 'password', totp_secret: 'JBSWY3DP' }))).toBe(true);
  });
});
