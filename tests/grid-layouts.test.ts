// SPDX-License-Identifier: PolyForm-Strict-1.0.0
// Copyright Jubair Hasan.
/**
 * How the grid lays entries out: grouped by type, by pool, by bundle, and plain.
 * A layout may reorder and fold entries together but must never lose one: the
 * count in the toolbar and the cards on screen have to agree.
 */
import { describe, it, expect, beforeEach } from 'vitest';
import { bootApp, entry } from './app-boot';

type St = Awaited<ReturnType<typeof bootApp>>['st'];
let st: St;

const headers = () =>
  [...document.querySelectorAll('#card-grid .type-group-header')].map((h) => h.textContent);
const cards = () => document.querySelectorAll('#card-grid .card').length;

describe('group by type', { timeout: 30000 }, () => {
  const mixed = () => [
    entry('a', 'ApiOne'),
    entry('p', 'PassOne', { secretType: 'password' }),
    entry('e', 'EnvOne', { secretType: 'env_var' }),
    entry('s', 'SshOne', { secretType: 'ssh_key' }),
    entry('k', 'CookieOne', { secretType: 'cookie' }),
    entry('c', 'CompositeOne', { secretType: 'composite' }),
    entry('w', 'NoType', { secretType: undefined }),
  ];

  it('puts a heading over each kind, in a fixed order, with the cards under it', async () => {
    ({ st } = await bootApp(mixed(), { settings: { groupByType: true } }));
    expect(headers().slice(0, 4)).toEqual(['API Keys', 'Passwords', 'Env Variables', 'SSH Keys']);
    const grid = document.getElementById('card-grid')!;
    const order = [...grid.children].map((c) =>
      c.classList.contains('type-group-header') ? `# ${c.textContent}` : 'card',
    );
    expect(order.slice(0, 3)).toEqual(['# API Keys', 'card', 'card']);
  });

  it('shows every entry, including kinds that have no heading of their own', async () => {
    ({ st } = await bootApp(mixed(), { settings: { groupByType: true } }));
    // Regression: entries whose type was not one of the seven listed headings vanished
    // from the grid while the count above it still included them.
    expect(cards()).toBe(7);
    expect(document.getElementById('result-count')!.textContent).toBe('7 secrets');
    expect(headers()).toEqual([
      'API Keys',
      'Passwords',
      'Env Variables',
      'SSH Keys',
      'Composite',
      'Cookie',
    ]);
  });
});

const bundleCard = () => document.querySelector<HTMLElement>('.bundle-card-wrap')!;
const bundleBase = (extra_vars: object[], over: object = {}) => [
  entry('bun', 'Discord', {
    secretType: 'bundle',
    api_key: '',
    bundle_primary: 'm1',
    extra_vars,
    ...over,
  }),
  entry('m1', 'Bot', {
    bundle_id: 'bun',
    bundle_slot: 'bot',
    bundle_order: 10,
    api_key: 'bot-token-value',
  }),
  entry('m2', 'Hook', { bundle_id: 'bun', bundle_slot: 'hook', bundle_order: 20, tags: ['edge'] }),
];
const localRows = () =>
  Object.fromEntries(
    [...document.querySelectorAll('.bundle-local-vars .key-row')].map((r) => [
      r.querySelector('.key-label')!.textContent!.trim(),
      r.querySelector('.key-value')!.textContent!.replace(/\s+/g, ' ').replace('×', '').trim(),
    ]),
  );

describe('a bundle card', { timeout: 30000 }, () => {
  beforeEach(() => {
    document.body.innerHTML = '';
  });

  it('masks secret locals and shows public ones, as stored', async () => {
    ({ st } = await bootApp(
      bundleBase([
        { key: 'prefix', value: '!', public: true },
        { key: 'token', value: 'tok-123456789-long', secret: true },
        { key: 'plain', value: 'neither-flag-set' },
        { key: 'shown', value: 'visible-anyway', secret: false },
      ]),
      { settings: { groupBundles: true } },
    ));
    const rows = localRows();
    expect(rows['prefix']).toBe('!');
    expect(rows['token']).not.toContain('tok-123456789-long');
    expect(rows['plain']).not.toContain('neither-flag-set');
    expect(rows['shown']).toBe('visible-anyway');
  });

  it('resolves a template local from a member’s field, masking it when the field is secret', async () => {
    ({ st } = await bootApp(
      bundleBase([
        { key: 'auth', kind: 'template', value: 'Bot {bot.api_key}' },
        { key: 'open', kind: 'template', value: 'user {bot.account}', public: true },
        { key: 'both', kind: 'template', value: 'x {bot.api_key}', public: true },
      ]),
      { settings: { groupBundles: true } },
    ));
    const rows = localRows();
    expect(rows['auth']).not.toContain('bot-token-value');
    expect(rows['auth']).toContain('Bot');
  });

  it('shows why a template cannot be resolved, naming the missing part', async () => {
    ({ st } = await bootApp(
      bundleBase([
        { key: 'a', kind: 'template', value: '{nope}' },
        { key: 'b', kind: 'template', value: '{c}' },
        { key: 'c', kind: 'template', value: '{b}' },
        { key: 'g', kind: 'template', value: '${Nobody/api_key}' },
        { key: 'u', kind: 'template', value: '${unclosed' },
      ]),
      { settings: { groupBundles: true } },
    ));
    const rows = localRows();
    expect(rows['a']).toMatch(/^Unresolved: /);
    expect(rows['b']).toMatch(/^Unresolved: .*→/);
    expect(rows['g']).toContain('Nobody/api_key');
    expect(rows['u']).toMatch(/^Unresolved: /);
  });

  it('resolves a global reference to a vault entry, and treats a public field as readable', async () => {
    ({ st } = await bootApp(
      [
        ...bundleBase([
          { key: 'g', kind: 'template', value: '${Other/api_key}', public: true },
          { key: 'pub', kind: 'template', value: '${Other}', public: true },
          { key: 'hidden', kind: 'template', value: '${Quiet/api_key}', public: true },
        ]),
        entry('o', 'Other', { api_key: 'other-secret-value', primary_public: true }),
        entry('q', 'Quiet', { api_key: 'quiet-secret-value' }),
      ],
      { settings: { groupBundles: true } },
    ));
    const rows = localRows();
    expect(rows['pub']).toBe('other-secret-value');
    expect(rows['g']).toBe('other-secret-value');
    // A local marked public cannot unmask a field the entry keeps secret.
    expect(rows['hidden']).not.toContain('quiet-secret-value');
  });

  it('summarises the projects, categories and tags of its members, with "+N" past three', async () => {
    ({ st } = await bootApp(
      [
        entry('bun', 'Union', { secretType: 'bundle', api_key: '' }),
        entry('m1', 'M1', {
          bundle_id: 'bun',
          bundle_slot: 'one',
          projectIds: ['Universal', 'web'],
          categories: ['a', 'b'],
          tags: ['t1', 't2', 't3', 't4', 't5'],
        }),
      ],
      {
        settings: { groupBundles: true },
        categories: ['a', 'b'],
        projects: [
          { id: 'Universal', name: 'Universal' },
          { id: 'web', name: 'Web' },
        ],
      },
    ));
    const chips = [...document.querySelectorAll('.bundle-union')].map((c) => c.textContent);
    expect(chips).toEqual(['Projects: Web', 'Categories: a, b', 'Tags: t1, t2, t3 +2']);
  });

  it('names the project by its id when the project no longer exists', async () => {
    ({ st } = await bootApp(
      [
        entry('bun', 'Union', { secretType: 'bundle', api_key: '' }),
        entry('m1', 'M1', {
          bundle_id: 'bun',
          bundle_slot: 'one',
          projectIds: ['Universal', 'gone'],
        }),
      ],
      { settings: { groupBundles: true } },
    ));
    expect(document.querySelector('.bundle-union')!.textContent).toBe('Projects: gone');
  });

  it('flags two members that would generate the same variable name', async () => {
    ({ st } = await bootApp(
      [
        entry('bun', 'Clash', { secretType: 'bundle', api_key: '' }),
        entry('m1', 'Same', { bundle_id: 'bun', bundle_slot: 'one' }),
        entry('m2', 'Same', { bundle_id: 'bun', bundle_slot: 'two' }),
      ],
      { settings: { groupBundles: true } },
    ));
    expect(document.querySelector('.bundle-collision')!.textContent).toMatch(/name collision/);
  });

  it('with more than three members shows one at a time behind slot tabs', async () => {
    ({ st } = await bootApp(
      [
        entry('bun', 'Many', { secretType: 'bundle', api_key: '' }),
        ...['a', 'b', 'c', 'd'].map((s, i) =>
          entry(`m${s}`, `Member ${s}`, {
            bundle_id: 'bun',
            bundle_slot: s,
            bundle_order: (i + 1) * 10,
            version: s === 'c' ? 'v3' : undefined,
          }),
        ),
      ],
      { settings: { groupBundles: true } },
    ));
    const tabs = [...document.querySelectorAll('.bundle-slot-tabs [role="tab"]')];
    expect(tabs.map((t) => t.textContent!.trim())).toEqual(['a', 'b', 'v3', 'd']);
    expect(tabs.map((t) => t.getAttribute('aria-selected'))).toEqual([
      'true',
      'false',
      'false',
      'false',
    ]);
    expect(document.querySelectorAll('.bundle-member')).toHaveLength(1);
  });

  it('opens by itself when a search matches only some of its members', async () => {
    ({ st } = await bootApp(bundleBase([]), { settings: { groupBundles: true } }));
    st.searchQ = 'Hook';
    (await import('../src/ts/render')).renderGrid();
    expect(bundleCard().classList.contains('expanded')).toBe(true);
    expect(bundleCard().querySelectorAll('.bundle-member')).toHaveLength(1);
  });

  it('an explicit collapse wins over a filter that would open it', async () => {
    ({ st } = await bootApp(bundleBase([]), { settings: { groupBundles: true } }));
    st.collapsedBundles.add('bun');
    st.searchQ = 'Hook';
    (await import('../src/ts/render')).renderGrid();
    expect(bundleCard().classList.contains('expanded')).toBe(false);
  });

  it('falls back to the first member as primary when the stored primary is gone', async () => {
    ({ st } = await bootApp(bundleBase([], { bundle_primary: 'ghost' }), {
      settings: { groupBundles: true },
    }));
    expect(bundleCard().textContent).toContain('Bot');
  });
});

describe('pool cards and the empty grid', { timeout: 30000 }, () => {
  const pooled = () => [
    entry('p1', 'Alpha one', { pool: 'ring' }),
    entry('p2', 'Alpha two', { pool: 'ring' }),
    entry('s', 'Solo'),
  ];

  it('folds pool members into one card, and a search that matches only some opens it', async () => {
    ({ st } = await bootApp(pooled(), { settings: { groupPools: true } }));
    expect(document.querySelectorAll('.pool-card-wrap')).toHaveLength(1);
    st.searchQ = 'one';
    (await import('../src/ts/render')).renderGrid();
    expect(document.querySelector('.pool-card-wrap')).toBeNull();
    st.searchQ = 'Alpha';
    (await import('../src/ts/render')).renderGrid();
    expect(document.querySelector('.pool-card-wrap')).not.toBeNull();
  });

  it('keeps folding pools when the vault also holds a bundle', async () => {
    ({ st } = await bootApp([...pooled(), ...bundleBase([])], {
      settings: { groupPools: true, groupBundles: true },
    }));
    expect(document.querySelectorAll('.pool-card-wrap:not(.bundle-card-wrap)')).toHaveLength(1);
    expect(document.querySelectorAll('.bundle-card-wrap')).toHaveLength(1);
  });

  it('a search with no match says so, and what to try', async () => {
    ({ st } = await bootApp(pooled()));
    st.searchQ = 'zzz';
    (await import('../src/ts/render')).renderGrid();
    expect(document.querySelector('.empty-state')!.textContent).toContain('No secrets found');
    expect(document.querySelector('.empty-state')!.textContent).toContain('Try a different search');
    st.searchQ = '';
    st.vault.api_keys = [];
    (await import('../src/ts/render')).renderGrid();
    expect(document.querySelector('.empty-state')!.textContent).toContain(
      'Add a secret or import a backup',
    );
  });

  it('the toolbar counts secrets and bundles separately, with singular and plural', async () => {
    ({ st } = await bootApp(bundleBase([]), { settings: { groupBundles: true } }));
    expect(document.getElementById('result-count')!.textContent).toBe('2 secrets · 1 bundle');
  });
});
