// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
/** Branch coverage for filters.ts and entry-timeline.ts. */
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { getFiltered, sorted, getDescendantProjectIds, buildProjectTree } from '../src/ts/filters';
import { st } from '../src/ts/state';
import { makeEntry, makeProject, resetState } from './helpers';

beforeEach(() => resetState(st));

const ids = (a: any[]) => a.map((e) => e.provider);

describe('getFiltered branches', () => {
  beforeEach(() => {
    st.vault.api_keys = [
      makeEntry({
        provider: 'A',
        price_type: 'paid',
        categories: ['ai/llm'],
        environment: 'prod',
        tags: ['t1'],
        env_prefixes: ['P_'],
        pool: ' main ',
        projectIds: ['x'],
      } as any),
      makeEntry({
        provider: 'B',
        price_type: 'free',
        categories: undefined,
        tags: undefined,
        env_prefixes: undefined,
        pool: undefined,
        projectIds: undefined,
        totp_secret: ' ',
      } as any),
      makeEntry({ provider: 'C', secretType: 'ssh_key', totp_secret: 'JBSWY3DP' } as any),
    ];
  });

  it('applies each search filter key', () => {
    st.searchQ = 'price:paid';
    expect(ids(getFiltered())).toEqual(['A']);
    st.searchQ = 'cat:llm';
    expect(ids(getFiltered())).toEqual(['A']);
    st.searchQ = 'env:prod';
    expect(ids(getFiltered())).toEqual(['A']);
  });

  it('applies the sidebar filters', () => {
    st.currentEnvFilter = 'prod';
    expect(ids(getFiltered())).toEqual(['A']);
    st.currentEnvFilter = null as any;
    st.activeTagFilter = 't1';
    expect(ids(getFiltered())).toEqual(['A']);
    st.activeTagFilter = null as any;
    st.activePrefixFilter = 'P_';
    expect(ids(getFiltered())).toEqual(['A']);
    st.activePrefixFilter = null as any;
    st.activePoolFilter = 'main';
    expect(ids(getFiltered())).toEqual(['A']);
    st.activePoolFilter = null;
  });

  it('filters by project, falling back to Universal on an empty selection', () => {
    st.vault.projects.push(makeProject({ id: 'x', name: 'X' }));
    st.currentSelectedProjectIds = ['x'];
    expect(ids(getFiltered())).toEqual(['A']);
    st.currentSelectedProjectIds = [];
    expect(getFiltered()).toHaveLength(3);
  });

  it('text search: plain, regex, flagged regex, malformed regex', () => {
    st.searchQ = 'a';
    expect(getFiltered().length).toBeGreaterThan(0);
    st.searchQ = '/^a /';
    expect(ids(getFiltered())).toContain('A');
    st.searchQ = '/b/i';
    expect(ids(getFiltered())).toContain('B');
    st.searchQ = '/nomatch/';
    expect(getFiltered()).toHaveLength(0);
    st.searchQ = '/(/';
    expect(getFiltered()).toHaveLength(0);
    st.searchQ = '/a(/';
    expect(getFiltered()).toHaveLength(0);
    st.searchQ = 'zzz';
    expect(getFiltered()).toHaveLength(0);
  });

  it('sidebar price and category filters', () => {
    st.filter = { type: 'price', value: 'paid' } as any;
    expect(ids(getFiltered())).toEqual(['A']);
    st.filter = { type: 'category', value: 'ai' } as any;
    expect(ids(getFiltered())).toEqual(['A']);
    st.filter = { type: 'category', value: 'ai/llm' } as any;
    expect(ids(getFiltered())).toEqual(['A']);
    st.filter = { type: 'category', value: 'Nope' } as any;
    expect(getFiltered()).toHaveLength(0);
  });

  it('type chips, including the virtual totp chip', () => {
    st.activeTypeChips.add('__totp');
    expect(ids(getFiltered())).toEqual(['C']);
    st.activeTypeChips.clear();
    st.activeTypeChips.add('api_key');
    expect(ids(getFiltered())).toEqual(['A', 'B']);
    st.activeTypeChips.clear();
    st.activeTypeChips.add('ssh_key');
    expect(ids(getFiltered())).toEqual(['C']);
  });
});

describe('sorted branches', () => {
  const a = makeEntry({
    provider: 'A',
    price_type: 'paid',
    categories: ['z'],
    expires_at: '2030-01-01',
  } as any);
  const b = makeEntry({
    provider: 'B',
    price_type: 'free',
    categories: [],
    expires_at: '2029-01-01',
  } as any);
  const c = makeEntry({
    provider: 'C',
    price_type: 'weird',
    categories: ['z'],
    pinned: true,
  } as any);
  const d = makeEntry({ provider: 'D', price_type: 'paid', categories: undefined } as any);

  it('pins first, then orders by the chosen key', () => {
    st.currentSortBy = 'price';
    expect(ids(sorted([a, b, c, d]))).toEqual(['C', 'B', 'A', 'D']);
    st.currentSortBy = 'category';
    expect(ids(sorted([a, b, c, d]))).toEqual(['C', 'A', 'B', 'D']);
    st.currentSortBy = 'expiry';
    expect(ids(sorted([a, b, c, d]))[0]).toBe('C');
    expect(ids(sorted([a, b, d]))).toEqual(['B', 'A', 'D']);
    st.currentSortBy = 'provider';
    expect(ids(sorted([d, a]))).toEqual(['A', 'D']);
    expect(ids(sorted([a, c]))).toEqual(['C', 'A']);
    st.currentSortBy = 'price';
    expect(ids(sorted([c, a, b].map((e) => ({ ...e, pinned: false }))))).toEqual(['B', 'A', 'C']);
    expect(ids(sorted([b, { ...c, pinned: false }]))).toEqual(['B', 'C']);
    expect(ids(sorted([{ ...c, pinned: false }, b]))).toEqual(['B', 'C']);
  });
});

describe('project helpers', () => {
  it('descendants of a virtual id, an unknown id and a real id', () => {
    st.vault.projects = [
      makeProject({ id: 'p1', name: 'A/B' }),
      makeProject({ id: 'p2', name: 'A/B/C' }),
    ];
    expect(getDescendantProjectIds('virtual:A')).toEqual(['p1', 'p2']);
    expect(getDescendantProjectIds('nope')).toEqual(['nope']);
    expect(getDescendantProjectIds('p1')).toEqual(['p1', 'p2']);
    const tree = buildProjectTree(st.vault.projects);
    expect(tree[0].virtual).toBe(true);
    expect(tree[0].children[0].children[0].name).toBe('A/B/C');
  });
});

describe('entry timeline', () => {
  async function load() {
    vi.resetModules();
    const utils = await import('../src/ts/utils');
    vi.spyOn(utils, 'showToast').mockImplementation(() => {});
    const prompt = vi.spyOn(utils, 'showPromptLarge').mockResolvedValue(null as any);
    return { utils, prompt, mod: await import('../src/ts/entry-timeline') };
  }

  it('asks to save first when the entry has no id', async () => {
    const { mod, utils } = await load();
    await mod.showEntryTimeline({ provider: 'x' } as any);
    expect(utils.showToast).toHaveBeenCalledWith(expect.stringMatching(/Save this entry/), 'info');
  });

  it('explains when neither desktop nor remote', async () => {
    const { mod, utils } = await load();
    await mod.showEntryTimeline({ id: '1', provider: 'x' } as any);
    expect(utils.showToast).toHaveBeenCalledWith(
      expect.stringMatching(/desktop app or a remote/),
      'info',
    );
  });

  it('renders desktop events, with and without detail, and the empty case', async () => {
    const { mod, prompt } = await load();
    const { st: s } = await import('../src/ts/state');
    (window as any).__TAURI__ = { core: { invoke: vi.fn() } };
    const inv = (window as any).__TAURI__.core.invoke;
    inv.mockResolvedValueOnce([
      { kind: 'k', at: 't', at_precision: 'p', source: 's', detail: { a: 1 } },
      { kind: 'k2', at: 't2', at_precision: 'p', source: 's2', detail: {} },
    ]);
    await mod.showEntryTimeline({ id: '1', provider: 'x' } as any);
    expect(prompt.mock.calls[0][1]).toBe('t  k  s  a=1\nt2  k2  s2');
    inv.mockResolvedValueOnce(null);
    await mod.showEntryTimeline({ id: '1', provider: 'x' } as any);
    expect(prompt.mock.calls[1][1]).toBe('No lifecycle events yet.');
    void s;
    delete (window as any).__TAURI__;
  });

  it('reads the remote timeline and reports errors, Error or not', async () => {
    const { mod, utils, prompt } = await load();
    const { st: s, RemoteVaultStore } = await import('../src/ts/state');
    const remote = new RemoteVaultStore('http://localhost:1');
    remote.api = vi
      .fn()
      .mockResolvedValueOnce([])
      .mockRejectedValueOnce(new Error('boom'))
      .mockRejectedValueOnce('str') as any;
    s.store = remote;
    await mod.showEntryTimeline({ id: 'a/b', provider: 'x' } as any);
    expect((remote.api as any).mock.calls[0][0]).toBe('/api/vault/entries/a%2Fb/timeline');
    expect(prompt).toHaveBeenCalled();
    await mod.showEntryTimeline({ id: '1', provider: 'x' } as any);
    expect(utils.showToast).toHaveBeenCalledWith('boom', 'err');
    await mod.showEntryTimeline({ id: '1', provider: 'x' } as any);
    expect(utils.showToast).toHaveBeenCalledWith('str', 'err');
  });
});
