// SPDX-License-Identifier: PolyForm-Strict-1.0.0
// Copyright Jubair Hasan.
/** Branch coverage for bundle-scope.ts: renames, references, resolver errors, composite errors. */
import { describe, it, expect, beforeEach } from 'vitest';
import {
  renameBundleRefs,
  renameBundleSlotRefs,
  renameBundleLocalRefs,
  referencesToBundleMember,
  resolveBundleTemplate,
  renderBundleComposite,
  buildBundle,
  bundleSuggestions,
} from '../src/ts/bundle-scope';
import { st } from '../src/ts/state';
import { makeEntry, makeProject, resetState } from './helpers';

beforeEach(() => resetState(st));

const tmpl = (key: string, value: string) => ({ key, value, kind: 'template' as const });
const project = (fields: string[]) =>
  makeProject({
    id: 'p',
    name: 'Proj',
    chunks: [
      {
        id: 'c',
        name: 'Ch',
        chunk_type: 'custom',
        fields: fields.map((v, i) => ({ key: `k${i}`, value: v })),
      },
      { id: 'd', name: 'Empty', chunk_type: 'custom' },
    ],
  } as any);

describe('rename cascades', () => {
  it('bundle rename: guards, composite, template vars, chunk fields, no chunks', () => {
    expect(renameBundleRefs([], [], '', 'x')).toBe(0);
    expect(renameBundleRefs([], [], 'a', 'a')).toBe(0);
    const e = makeEntry({
      id: 'e',
      composite_template: '${bundle:Old/slot}',
      extra_vars: [tmpl('t', '${bundle:Old/s}'), { key: 'plain', value: '${bundle:Old/s}' }],
    } as any);
    const bare = makeEntry({ id: 'b' } as any);
    delete (bare as any).extra_vars;
    const p = project(['${bundle:Old}']);
    const noChunks = makeProject({ id: 'q', name: 'Q' });
    const n = renameBundleRefs([e, bare], [p, noChunks], 'Old', 'New');
    expect(n).toBe(3);
    expect(e.composite_template).toBe('${bundle:New/slot}');
    expect((e.extra_vars![1] as any).value).toBe('${bundle:Old/s}');
    expect((p.chunks![0].fields![0] as any).value).toBe('${bundle:New}');
  });

  it('slot rename: global and scoped refs only for the right owner', () => {
    const bundle = makeEntry({
      id: 'b',
      provider: 'Bun',
      extra_vars: [tmpl('t', '{old.f} ${bundle:Bun/old/x}'), { key: 'plain', value: '{old.f}' }],
    } as any);
    const member = makeEntry({
      id: 'm',
      bundle_id: 'b',
      composite_template: '{old.f} ${bundle:Bun/old}',
      extra_vars: [tmpl('t', '{old.f}')],
    } as any);
    const outsider = makeEntry({
      id: 'o',
      composite_template: '{old.f} ${bundle:Bun/old}',
      extra_vars: [tmpl('t', '{old.f}')],
    } as any);
    const bare = makeEntry({ id: 'x' } as any);
    delete (bare as any).extra_vars;
    const p = project(['${bundle:Bun/old}']);
    const noChunks = makeProject({ id: 'q', name: 'Q' });
    expect(renameBundleSlotRefs([], [], bundle, '', 'n')).toBe(0);
    expect(renameBundleSlotRefs([], [], bundle, 'a', 'a')).toBe(0);
    const n = renameBundleSlotRefs(
      [bundle, member, outsider, bare],
      [p, noChunks],
      bundle,
      'old',
      'new',
    );
    expect(n).toBe(7);
    expect(member.composite_template).toBe('{new.f} ${bundle:Bun/new}');
    expect(outsider.composite_template).toBe('{old.f} ${bundle:Bun/new}');
    expect((outsider.extra_vars![0] as any).value).toBe('{old.f}');
    expect((bundle.extra_vars![1] as any).value).toBe('{old.f}');
  });

  it('local rename: global and local refs only for the right owner', () => {
    const bundle = makeEntry({
      id: 'b',
      provider: 'Bun',
      extra_vars: [tmpl('t', '{old} ${bundle:Bun/old}'), { key: 'plain', value: '{old}' }],
    } as any);
    const member = makeEntry({
      id: 'm',
      bundle_id: 'b',
      composite_template: '{old} ${bundle:Bun/old}',
      extra_vars: [tmpl('t', '{old}')],
    } as any);
    const outsider = makeEntry({
      id: 'o',
      composite_template: '{old} ${bundle:Bun/old}',
      extra_vars: [tmpl('t', '{old}')],
    } as any);
    const bare = makeEntry({ id: 'x' } as any);
    delete (bare as any).extra_vars;
    const p = project(['${bundle:Bun/old}']);
    const noChunks = makeProject({ id: 'q', name: 'Q' });
    expect(renameBundleLocalRefs([], [], bundle, '', 'n')).toBe(0);
    expect(renameBundleLocalRefs([], [], bundle, 'a', 'a')).toBe(0);
    const n = renameBundleLocalRefs(
      [bundle, member, outsider, bare],
      [p, noChunks],
      bundle,
      'old',
      'new',
    );
    expect(n).toBe(7);
    expect(member.composite_template).toBe('{new} ${bundle:Bun/new}');
    expect(outsider.composite_template).toBe('{old} ${bundle:Bun/new}');
    expect((bundle.extra_vars![1] as any).value).toBe('{old}');
  });
});

describe('referencesToBundleMember', () => {
  it('finds scoped and global references once, in entries, vars and chunk fields', () => {
    const bundle = makeEntry({
      id: 'b',
      provider: 'Bun',
      extra_vars: [tmpl('t', '{m.api_key}')],
    } as any);
    const member = makeEntry({ id: 'm', provider: 'Mem', bundle_slot: undefined } as any);
    const slotted = makeEntry({ id: 's', provider: 'S', bundle_slot: 'm' } as any);
    const other = makeEntry({
      id: 'o',
      provider: 'Oth',
      composite_template: '${bundle:Bun/m} {m.x}',
      extra_vars: [tmpl('v', '${bundle:Bun/m/f}'), { key: 'n', value: '{m.f}' }],
    } as any);
    const inner = makeEntry({
      id: 'i',
      provider: 'In',
      bundle_id: 'b',
      composite_template: '{m.x}',
    } as any);
    const bareE = makeEntry({ id: 'x', provider: 'X' } as any);
    delete (bareE as any).extra_vars;
    const p = project(['${bundle:Bun/m}', 'nothing']);
    (p.chunks![0].fields as any).push({ key: 'num', value: 5 });
    const noChunks = makeProject({ id: 'q', name: 'Q' });
    expect(
      referencesToBundleMember(bundle, slotted, [bundle, other, inner, bareE], [p, noChunks])
        .length,
    ).toBeGreaterThan(3);
    // slot falls back to provider; default projects
    const named = makeEntry({ id: 'n', provider: 'm' } as any);
    expect(referencesToBundleMember(bundle, named, [bundle]).map((s) => s.label)).toEqual([
      'Bun / t',
    ]);
    void member;
  });
});

describe('resolveBundleTemplate', () => {
  const bundle = (extra: any[]) => makeEntry({ id: 'b', provider: 'B', extra_vars: extra } as any);
  const resolve = (b: any, members: any[], t: string, parts: any[] = [], g?: any) =>
    resolveBundleTemplate(b, members, t, parts, g);

  it('rejects duplicate slots', () => {
    const m = makeEntry({ id: '1', provider: 'A' } as any);
    const r = resolve(bundle([]), [m, { ...m, id: '2' }], 'x');
    expect(!r.ok && r.error).toMatchObject({ kind: 'invalid' });
  });

  it('warns when a local is shadowed by a part or slot', () => {
    const r = resolve(
      bundle([
        { key: 'k', value: 'v' },
        { key: 'slot', value: 'v' },
      ]),
      [makeEntry({ id: 'm', provider: 'P', bundle_slot: 'slot' } as any)],
      '{k}',
      [{ key: 'k', value: 'part' }],
    );
    expect(r.ok && r.warnings).toHaveLength(2);
    expect(r.ok && r.value.value).toBe('part');
  });

  it('literal braces, globals and their errors', () => {
    const b = bundle([]);
    expect(resolve(b, [], '{{x}}').ok && (resolve(b, [], '{{x}}') as any).value.value).toBe('{x}');
    expect(resolve(b, [], '${g}', [], () => 'sec')).toMatchObject({
      ok: true,
      value: { value: 'sec', secret: true },
    });
    expect(resolve(b, [], '${g}', [], () => ({ value: 'pub', secret: false }))).toMatchObject({
      value: { secret: false },
    });
    expect(resolve(b, [], '${g}')).toMatchObject({ ok: false, error: { kind: 'unresolved' } });
    expect(resolve(b, [], '${g', [], () => 'x')).toMatchObject({ error: { kind: 'invalid' } });
    expect(resolve(b, [], 'a}b')).toMatchObject({ error: { kind: 'invalid' } });
    expect(resolve(b, [], 'a{b')).toMatchObject({ error: { kind: 'invalid' } });
    expect(resolve(b, [], '{1bad}')).toMatchObject({ error: { kind: 'invalid' } });
    expect(resolve(b, [], '{nope}')).toMatchObject({ error: { kind: 'unresolved' } });
    expect(resolve(b, [], '{nobody.field}')).toMatchObject({ error: { kind: 'unresolved' } });
  });

  it('parts and locals carry their public flags', () => {
    const b = bundle([
      { key: 'pub', value: 'v', public: true },
      { key: 'sec', value: 'v' },
      { key: 'open', value: 'v', secret: false },
    ]);
    const parts = [
      { key: 'pp', value: 'x', public: true },
      { key: 'qq', value: 'y' },
    ];
    expect(resolve(b, [], '{pp}', parts)).toMatchObject({ value: { secret: false } });
    expect(resolve(b, [], '{qq}', parts)).toMatchObject({ value: { secret: true } });
    expect(resolve(b, [], '{pub}')).toMatchObject({ value: { secret: false } });
    expect(resolve(b, [], '{sec}')).toMatchObject({ value: { secret: true } });
    expect(resolve(b, [], '{open}')).toMatchObject({ value: { secret: false } });
  });

  it('nested templates: resolved, tainted, cyclic and too deep', () => {
    const nested = bundle([
      { key: 'a', value: 'x', public: true },
      tmpl('t', '[{a}]'),
      tmpl('u', '{t}!'),
    ]);
    expect(resolve(nested, [], '{u}')).toMatchObject({
      ok: true,
      value: { value: '[x]!', secret: true },
    });
    const cyc = bundle([tmpl('x', '{y}'), tmpl('y', '{x}')]);
    expect(resolve(cyc, [], '{x}')).toMatchObject({ error: { kind: 'cycle' } });
    const deep = bundle([
      tmpl('d1', '{d2}'),
      tmpl('d2', '{d3}'),
      tmpl('d3', '{d4}'),
      tmpl('d4', '{d5}'),
      tmpl('d5', '{d6}'),
      { key: 'd6', value: 'end' },
    ]);
    expect(resolve(deep, [], '{d1}')).toMatchObject({ error: { kind: 'depth' } });
    const failing = bundle([tmpl('bad', '{ghost}')]);
    expect(resolve(failing, [], '{bad}')).toMatchObject({ error: { kind: 'unresolved' } });
  });

  it('member fields: variables, fields, empty values and non-string variable values', () => {
    const m = makeEntry({
      id: 'm',
      provider: 'Svc',
      api_key: 'KEY',
      extra_vars: [
        { key: 'v', value: 'val', public: true },
        { key: 'w', value: 5 as any },
      ],
    } as any);
    const b = bundle([]);
    expect(resolve(b, [m], '{Svc.v}')).toMatchObject({ value: { value: 'val', secret: false } });
    expect(resolve(b, [m], '{Svc.api_key}')).toMatchObject({ value: { value: 'KEY' } });
    expect(resolve(b, [m], '{Svc.w}')).toMatchObject({ error: { kind: 'unresolved' } });
    expect(resolve(b, [m], '{Svc.api_url}')).toMatchObject({ error: { kind: 'unresolved' } });
  });
});

describe('renderBundleComposite', () => {
  const bundle = makeEntry({
    id: 'b',
    provider: 'B',
    extra_vars: [{ key: 'host', value: 'h.test', public: true }],
  } as any);
  const render = (t: string, g?: any, kind: any = 'link') =>
    renderBundleComposite(bundle, [], t, [], kind, g);

  it('renders locals, literal braces and globals', () => {
    expect(render('https://{host}/{{x}}')).toMatchObject({
      ok: true,
      value: 'https://h.test/{x}',
      secret: false,
    });
    expect(render('https://h/?k=${g}', () => 'a b')).toMatchObject({
      ok: true,
      value: 'https://h/?k=a%20b',
      secret: true,
    });
  });

  it('reports each malformed shape', () => {
    expect(render('${g')).toMatchObject({ error: { kind: 'invalid' } });
    expect(render('a}b')).toMatchObject({ error: { kind: 'invalid' } });
    expect(render('a{b')).toMatchObject({ error: { kind: 'invalid' } });
    expect(render('{nope}')).toMatchObject({ error: { kind: 'unresolved' } });
    expect(render('${g}')).toMatchObject({ error: { kind: 'unresolved' } });
    expect(render('{host}', undefined, 'link')).toMatchObject({ ok: true });
  });

  it('passes a render error through', () => {
    const b2 = makeEntry({
      id: 'b2',
      provider: 'B2',
      extra_vars: [{ key: 'nl', value: 'a\nb' }],
    } as any);
    const r = renderBundleComposite(b2, [], 'https://h/{nl}', [], 'link');
    expect(r).toMatchObject({ ok: false, error: { kind: 'control_character_in_part' } });
  });
});

describe('buildBundle and suggestions', () => {
  it('builds slots from labels, repairs odd names and uniquifies', () => {
    const a = makeEntry({ id: 'a', provider: 'Same' } as any);
    const b = makeEntry({ id: 'b', provider: 'Same' } as any);
    const c = makeEntry({ id: 'c', provider: '***' } as any);
    const d = makeEntry({ id: 'd', provider: 'x'.repeat(30), label: 'L' } as any);
    const bun = buildBundle([a, b, c, d], '  Name ');
    expect(bun.provider).toBe('Name');
    expect([a, b, c, d].map((e) => e.bundle_slot)).toEqual(['Same', 'Same_2', 'member_3', 'L']);
    const empty = buildBundle([], 'E');
    expect((empty as any).bundle_primary).toBeUndefined();
    const noId = buildBundle([makeEntry({ id: undefined, provider: 'P' } as any)], 'N');
    expect((noId as any).bundle_primary).toBeUndefined();
  });

  it('suggestions skip bundles, bundled, idless, pooled, non-string, blank and dismissed', () => {
    const mk = (o: object) => makeEntry({ provider: 'Acme', ...o } as any);
    const list = [
      mk({ id: '1' }),
      mk({ id: '2' }),
      mk({ id: '3', pool: ' p ' }),
      mk({ id: '4', secretType: 'bundle' }),
      mk({ id: '5', bundle_id: 'x' }),
      mk({ id: '' }),
      mk({ id: '6', provider: 5 }),
      mk({ id: '7', provider: '  ' }),
      mk({ id: '8', provider: 'Dis' }),
      mk({ id: '9', provider: 'Dis' }),
      mk({ id: '10', pool: '  ' }),
    ];
    const out = bundleSuggestions(list, ['dis']);
    expect(out).toHaveLength(1);
    expect(out[0].entries.map((e) => e.id)).toEqual(['1', '2', '10']);
  });
});
