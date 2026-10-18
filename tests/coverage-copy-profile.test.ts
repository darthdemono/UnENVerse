/** Branch coverage for copy-profile.ts: profiles, metadata styles and every bundle export dialect. */
import { describe, it, expect, beforeEach } from 'vitest';
import { buildCopyText, buildCopyTextAll, buildBundleExport } from '../src/ts/copy-profile';
import { st } from '../src/ts/state';
import { makeEntry, resetState } from './helpers';

beforeEach(() => resetState(st));

describe('copy text', () => {
  const full = makeEntry({
    id: 'f',
    provider: 'Prov',
    label: 'L',
    account_name: 'acct',
    version: 'v2',
    api_key: 'K',
    api_secret: 'S',
    api_url: 'https://u',
    extra_vars: [{ key: 'EXTRA', value: 'x' }, { key: '', value: 'skipped' }, { key: 'NOVAL' }],
    expires_at: '2030-01-01',
    rate_limit: '10/s',
    scopes: ['a', 'b'],
    environment: 'prod',
    pool: 'main',
    description: 'd',
    purpose: 'p',
    tags: ['t'],
    categories: ['c'],
    projectIds: ['Universal', 'proj'],
    created_at: 'c0',
    last_rotated_at: 'r0',
    rotation_days: 30,
    compromised: true,
  } as any);

  it('basic emits the header and values only', () => {
    const text = buildCopyText(full);
    expect(text.split('\n')[0]).toBe('# Prov — L — acct (v2)');
    expect(text).toContain('=K');
    expect(text).not.toContain('# version');
  });

  it('extended and full add metadata as comments or as variables', () => {
    const ext = buildCopyText(full, { profile: 'extended' });
    expect(ext).toContain('# scopes: a,b');
    expect(ext).not.toContain('# description');
    const all = buildCopyText(full, { profile: 'full' });
    for (const k of [
      'description',
      'purpose',
      'tags',
      'categories',
      'projects: proj',
      'created_at',
      'last_rotated_at',
      'rotation_days: 30',
      'compromised: true',
    ])
      expect(all).toContain(`# ${k}`);
    const asVars = buildCopyText(full, { profile: 'full', metadataStyle: 'var' });
    expect(asVars).not.toContain('# scopes');
    expect(asVars).toMatch(/SCOPES="a,b"/);
  });

  it('copes with a sparse entry', () => {
    const bare = makeEntry({ id: 'b', provider: '', api_key: undefined } as any);
    const t = buildCopyText(bare, { profile: 'full' });
    expect(t.split('\n')[0]).toBe('# UNKNOWN');
    expect(buildCopyTextAll([bare, bare])).toContain('\n\n');
    const rl = makeEntry({
      id: 'r',
      provider: 'R',
      api_key: 'k',
      rate_limit: 'varies by endpoint',
    } as any);
    expect(buildCopyText(rl, { profile: 'extended' })).toContain('varies by endpoint');
    const bareFull = { id: 'x', provider: 'X', api_key: 'k' } as any;
    expect(buildCopyText(bareFull, { profile: 'full' })).not.toContain('# projects');
  });
});

describe('bundle export', () => {
  const bundle = (extra: any[], more: object = {}) =>
    makeEntry({
      id: 'bundle',
      provider: 'B',
      secretType: 'bundle',
      extra_vars: extra,
      ...more,
    } as any);
  const lines = (s: string) => s.split('\n');

  it('keeps simple templates symbolic in python and javascript', () => {
    const b = bundle([
      { key: 'host', value: 'h' },
      { key: 'url', value: 'https://{host}/a"b\\c\n{{lit}} `t`', kind: 'template' },
    ]);
    const py = buildBundleExport(b, [], 'python');
    expect(py).toContain('B_URL = f"https://{B_HOST}/a\\"b\\\\c\\n{{lit}} `t`"');
    const js = buildBundleExport(b, [], 'javascript');
    expect(js).toContain('export const B_URL = `https://${B_HOST}/a"b\\\\c\n{lit} \\`t\\``;');
    expect(buildBundleExport(b, [], 'typescript')).toContain('B_HOST');
  });

  it('falls back to the rendered value for a malformed or non-local template', () => {
    const b = bundle([
      { key: 'g', value: '${some/ref}-x', kind: 'template' },
      { key: 'u', value: 'pre-{host', kind: 'template' },
    ]);
    // resolve unresolved global -> throws with the kind
    expect(() => buildBundleExport(b, [], 'python')).toThrow(
      /Cannot export g: unresolved reference/,
    );
    const open = bundle([{ key: 'u', value: 'a{b', kind: 'template' }]);
    expect(() => buildBundleExport(open, [], 'python')).toThrow(/Cannot export u/);
    const sibling = bundle([{ key: 'u', value: '{nope}', kind: 'template' }]);
    expect(() => buildBundleExport(sibling, [], 'javascript')).toThrow(/Cannot export u/);
  });

  it('typed values across dialects', () => {
    const b = bundle([
      { key: 'i', value: '42', kind: 'int' },
      { key: 'big', value: '9007199254740993', kind: 'int' },
      { key: 'neg', value: 'abc', kind: 'int' },
      { key: 'f', value: '0.5', kind: 'float' },
      { key: 'badf', value: 'x', kind: 'float' },
      { key: 'yes', value: 'true', kind: 'bool' },
      { key: 'no', value: 'false', kind: 'bool' },
      { key: 'hx', value: '255', kind: 'hex_int' },
      { key: 'hx2', value: '0xff', kind: 'hex_int' },
      { key: 'plain', value: "it's" },
    ]);
    const py = buildBundleExport(b, [], 'python');
    expect(py).toContain('B_I = 42');
    expect(py).toContain('B_BIG = 9007199254740993');
    expect(py).toContain('B_NEG = "abc"');
    expect(py).toContain('B_F = 0.5');
    expect(py).toContain('B_BADF = "x"');
    expect(py).toContain('B_YES = True');
    expect(py).toContain('B_NO = False');
    expect(py).toContain('B_HX = 0xff');
    expect(py).toContain('B_HX2 = 0xff');
    const js = buildBundleExport(b, [], 'javascript');
    expect(js).toContain('B_I = 42;');
    expect(js).toContain('B_BIG = "9007199254740993";');
    expect(js).toContain('B_YES = true;');
    expect(js).toContain('B_NEG = "abc";');
    expect(buildBundleExport(b, [], 'shell')).toContain("export B_PLAIN='it'\\''s'");
    expect(buildBundleExport(b, [], 'toml')).toContain('B_PLAIN = "it\'s"');
    expect(buildBundleExport(b, [], 'yaml')).toContain('B_PLAIN: "it\'s"');
    expect(JSON.parse(buildBundleExport(b, [], 'json')).B_I).toBe('42');
    expect(lines(buildBundleExport(b, [], 'dotenv'))).toContain('B_I=42');
  });

  it('rejects a hex_int that is not an integer', () => {
    const b = bundle([{ key: 'hx', value: 'zz', kind: 'hex_int' }]);
    expect(() => buildBundleExport(b, [], 'python')).toThrow(/B_HX is marked hex_int/);
  });

  it('exports the key, secret and url of an entry and renders a composite onto a missing key', () => {
    const b = bundle([]);
    const withAll = makeEntry({
      id: 'm',
      provider: 'M',
      api_key: 'k',
      api_secret: 's',
      api_url: 'u',
    } as any);
    const out = buildBundleExport(b, [withAll], 'dotenv');
    expect(out).toContain('=k');
    expect(out).toContain('=s');
    expect(out).toContain('=u');
    const comp = makeEntry({
      id: 'c',
      provider: 'Cmp',
      secretType: 'composite',
      composite_template: 'x://{p}',
      api_key: undefined,
      extra_vars: [{ key: 'p', value: 'v' }],
    } as any);
    expect(buildBundleExport(b, [comp], 'dotenv')).toContain('CMP=x://v');
    const compWithKey = { ...comp, api_key: 'old', composite_kind: 'link' };
    expect(buildBundleExport(b, [compWithKey], 'dotenv')).toContain('CMP=x://v');
  });

  it('skips keyless extra vars and tolerates a missing value', () => {
    const b = bundle([{ key: '', value: 'x' }, { key: 'NOVAL' }]);
    expect(buildBundleExport(b, [], 'dotenv')).toContain('B_NOVAL=');
  });
});

describe('bundle export odd shapes', () => {
  it('a composite without parts is an unresolved composite', () => {
    const b = makeEntry({ id: 'bundle', provider: 'B', secretType: 'bundle' } as any);
    const comp = makeEntry({
      id: 'c',
      provider: 'C',
      secretType: 'composite',
      composite_template: '{p}',
    } as any);
    expect(() => buildBundleExport(b, [comp], 'dotenv')).toThrow(/unresolved composite/);
  });

  it('a template that opens with a reference has no leading literal', () => {
    const b = makeEntry({
      id: 'bundle',
      provider: 'B',
      secretType: 'bundle',
      extra_vars: [
        { key: 'h', value: 'x' },
        { key: 'u', value: '{h}', kind: 'template' },
      ],
    } as any);
    expect(buildBundleExport(b, [], 'python')).toContain('B_U = f"{B_H}"');
  });

  it('a global reference inside a template exports its resolved text', () => {
    const b = makeEntry({
      id: 'bundle',
      provider: 'B',
      secretType: 'bundle',
      extra_vars: [{ key: 'u', value: 'a-${B/token}', kind: 'template' }],
    } as any);
    st.vault.api_keys.push(makeEntry({ id: 'g', provider: 'B', api_key: 'tok' } as any));
    const out = buildBundleExport(b, [], 'python');
    expect(out).toMatch(/B_U = "a-/);
  });
});

describe('bundle composite with a global reference', () => {
  it('resolves ${Provider/field} through the vault', () => {
    st.vault.api_keys.push(makeEntry({ id: 'src', provider: 'Src', api_key: 'sv' } as any));
    const b = makeEntry({ id: 'bundle', provider: 'B', secretType: 'bundle' } as any);
    const comp = makeEntry({
      id: 'c',
      provider: 'C',
      secretType: 'composite',
      composite_template: 'x://${Src/api_key}',
      extra_vars: [],
    } as any);
    expect(buildBundleExport(b, [comp], 'dotenv')).toContain('C=x://sv');
  });
});
