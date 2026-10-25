// SPDX-License-Identifier: PolyForm-Strict-1.0.0
// Copyright Jubair Hasan.
/**
 * Saving the vault: whole-document and delta saves, a save that folded in
 * someone else's changes, and a save refused because another writer got there
 * first. The risk throughout is quiet loss, so each branch is pinned by what
 * survives in the in-memory vault and in the store, and by what the person is told.
 */
import { describe, it, expect, beforeEach, vi } from 'vitest';
import {
  st,
  persist,
  forgetSynced,
  currentPatch,
  applyPatchLocal,
  VaultConflictError,
  conflictEntries,
} from '../src/ts/state';
import { loadRealIndexHtml, makeEntry, makeProject, makeVault, resetState } from './helpers';

const toasts: [string, string?][] = [];
let confirmAnswer = true;
vi.mock('../src/ts/utils', async (importOriginal) => {
  const real = await importOriginal<typeof import('../src/ts/utils')>();
  return {
    ...real,
    showToast: (m: string, k?: string) => toasts.push([m, k]),
    showConfirm: async () => confirmAnswer,
  };
});

interface FakeStore {
  isRemote: boolean;
  saves: any[];
  rows: any[];
  merged: boolean;
  failWith: unknown;
  loadResult: any;
  stale: number;
  rowsAccepted: boolean;
  failed: boolean;
  load: () => Promise<any>;
  save: (d: any) => Promise<void>;
  saveRows?: (p: any) => Promise<boolean>;
  takeMerged: () => boolean;
  markStale: () => void;
  lastSaveFailed: () => boolean;
}

function fakeStore(over: Partial<FakeStore> = {}): FakeStore {
  const s: FakeStore = {
    isRemote: false,
    saves: [],
    rows: [],
    merged: false,
    failWith: null,
    loadResult: null,
    stale: 0,
    rowsAccepted: true,
    failed: false,
    load: async () => s.loadResult,
    save: async (d) => {
      if (s.failWith) {
        const e = s.failWith;
        s.failWith = null;
        throw e as Error;
      }
      s.saves.push(JSON.parse(JSON.stringify(d)));
    },
    saveRows: async (p) => {
      if (s.failWith) {
        const e = s.failWith;
        s.failWith = null;
        throw e as Error;
      }
      s.rows.push(p);
      return s.rowsAccepted;
    },
    takeMerged: () => {
      const m = s.merged;
      s.merged = false;
      return m;
    },
    markStale: () => void s.stale++,
    lastSaveFailed: () => s.failed,
    ...over,
  };
  return s;
}

let store: FakeStore;
const entries = () => [
  makeEntry({ id: 'a', provider: 'A', api_key: 'key-a-value' } as any),
  makeEntry({ id: 'b', provider: 'B', api_key: 'key-b-value' } as any),
];

beforeEach(() => {
  loadRealIndexHtml();
  resetState(st);
  forgetSynced();
  toasts.length = 0;
  confirmAnswer = true;
  store = fakeStore();
  st.store = store as never;
  st.vault = makeVault({
    api_keys: entries(),
    user_categories: ['infra'],
    projects: [
      makeProject({ id: 'Universal', name: 'Universal' }),
      makeProject({ id: 'web', name: 'Web' }),
    ],
  });
});

describe('what a save sends', () => {
  it('sends the whole document the first time, then only what changed', async () => {
    await persist();
    expect(store.saves).toHaveLength(1);
    st.vault.api_keys[0].purpose = 'changed';
    await persist();
    expect(store.rows).toEqual([
      expect.objectContaining({ put: [expect.objectContaining({ id: 'a', purpose: 'changed' })] }),
    ]);
    expect(store.saves).toHaveLength(1);
  });

  it('sends nothing at all when nothing changed', async () => {
    await persist();
    await persist();
    expect(store.rows).toEqual([]);
    expect(store.saves).toHaveLength(1);
  });

  it('names deleted entries and projects, changed projects and changed categories', async () => {
    await persist();
    st.vault.api_keys.splice(0, 1);
    st.vault.projects[1] = { ...st.vault.projects[1], description: 'new' } as never;
    st.vault.user_categories.push('extra');
    st.vault.api_keys.push(makeEntry({ id: 'c', provider: 'C', api_key: 'key-c-value' } as any));
    const patch = currentPatch()!;
    expect(patch.delete).toEqual(['a']);
    expect(patch.put!.map((e) => e.id)).toEqual(['c']);
    expect(patch.projects_put!.map((p) => p.id)).toEqual(['web']);
    expect(patch.categories).toEqual(['infra', 'extra']);
    st.vault.projects.splice(1, 1);
    expect(currentPatch()!.projects_delete).toEqual(['web']);
  });

  it('falls back to a whole save when the order changed, an id repeats, or the array was replaced', async () => {
    await persist();
    st.vault.api_keys = [...st.vault.api_keys].reverse();
    expect(currentPatch()).toBeNull();
    await persist();
    expect(store.saves).toHaveLength(2);
    st.vault.api_keys.push(makeEntry({ id: 'a', provider: 'Dup' } as any));
    expect(currentPatch()).toBeNull();
    forgetSynced();
    expect(currentPatch()).toBeNull();
  });

  it('a store that cannot take rows gets the whole document', async () => {
    store.rowsAccepted = false;
    await persist();
    st.vault.api_keys[0].purpose = 'x';
    await persist();
    expect(store.rows).toHaveLength(1);
    expect(store.saves).toHaveLength(2);
  });

  it('a store without delta support always gets the whole document', async () => {
    store = fakeStore();
    delete store.saveRows;
    st.store = store as never;
    await persist();
    st.vault.api_keys[0].purpose = 'x';
    await persist();
    expect(store.saves).toHaveLength(2);
  });

  it('a save that did not reach the store is sent again next time', async () => {
    await persist();
    st.vault.api_keys[0].purpose = 'x';
    store.failed = true;
    await persist();
    store.failed = false;
    await persist();
    expect(store.rows).toHaveLength(2);
  });
});

describe('a save that failed', () => {
  it('says why when the store threw an error, or something else', async () => {
    store.failWith = new Error('disk full');
    await persist();
    expect(toasts).toContainEqual(['Save failed: disk full', 'err']);
    store.failWith = 'plain string';
    await persist();
    expect(toasts).toContainEqual(['Save failed: plain string', 'err']);
  });

  it('keeps queued saves in order even when one fails', async () => {
    store.failWith = new Error('first fails');
    const a = persist();
    const b = persist();
    await Promise.all([a, b]);
    expect(store.saves.length).toBe(1);
    expect(toasts.filter(([m]) => m.startsWith('Save failed'))).toHaveLength(1);
  });
});

describe('a save that folded in someone else’s changes', () => {
  it('reloads the merged vault and says so', async () => {
    store.merged = true;
    store.loadResult = {
      api_keys: [makeEntry({ id: 'z', provider: 'Merged', api_key: 'merged-key-1' } as any)],
      user_categories: [],
      projects: [{ id: 'Universal', name: 'Universal' }],
    };
    await persist();
    expect(st.vault.api_keys.map((e) => e.id)).toEqual(['z']);
    expect(toasts).toContainEqual(['Saved, and merged with changes someone else made', 'ok']);
  });

  it('when the reload fails, marks the store stale so the next save is refused rather than overwriting the merge', async () => {
    store.merged = true;
    store.loadResult = null;
    await persist();
    expect(store.stale).toBe(1);
    expect(toasts[toasts.length - 1][0]).toContain('could not be reloaded');
  });

  it('a reloaded vault with missing lists gets empty ones', async () => {
    store.merged = true;
    store.loadResult = { api_keys: [] };
    await persist();
    expect(st.vault.user_categories).toEqual([]);
    expect(st.vault.projects.map((p) => p.id)).toEqual(['Universal']);
  });
});

describe('a save refused because another writer got there first', () => {
  const settle = async () => {
    await persist();
    st.vault.api_keys[0].purpose = 'mine';
    store.failWith = new VaultConflictError('A');
  };
  const theirs = () => ({
    api_keys: [
      makeEntry({ id: 'a', provider: 'A', api_key: 'key-a-value', purpose: 'theirs' } as any),
      makeEntry({ id: 'b', provider: 'B', api_key: 'key-b-value' } as any),
      makeEntry({ id: 'n', provider: 'New', api_key: 'their-new-1' } as any),
    ],
    user_categories: ['infra'],
    projects: [{ id: 'Universal', name: 'Universal' }],
  });

  it('keeping mine puts my change on top of theirs and keeps what they added', async () => {
    await settle();
    store.loadResult = theirs();
    await persist();
    const a = st.vault.api_keys.find((e) => e.id === 'a')!;
    expect(a.purpose).toBe('mine');
    expect(st.vault.api_keys.map((e) => e.id)).toContain('n');
    expect(toasts).toContainEqual(['Your changes were applied on top of the other writer’s', 'ok']);
  });

  it('keeping mine without a reloadable store reports that the change was discarded', async () => {
    await settle();
    store.loadResult = null;
    await persist();
    expect(toasts[toasts.length - 1][0]).toContain('your change was discarded');
  });

  it('discarding reloads theirs and says my change is gone, or that it could not reload', async () => {
    confirmAnswer = false;
    await settle();
    store.loadResult = theirs();
    await persist();
    expect(st.vault.api_keys.find((e) => e.id === 'a')!.purpose).toBe('theirs');
    expect(toasts).toContainEqual([
      'Reloaded the other version — your unsaved change was discarded',
      'err',
    ]);
    st.vault.api_keys[0].purpose = 'again';
    store.failWith = new VaultConflictError();
    store.loadResult = null;
    await persist();
    expect(toasts[toasts.length - 1][0]).toContain('your change is still unsaved');
  });

  it('the conflict message names the entries both writers changed', () => {
    expect(conflictEntries('conflict; both changed: A, B')).toBe('A, B');
    expect(conflictEntries('nothing here')).toBeNull();
  });
});

describe('applying a patch to the in-memory vault', () => {
  it('replaces by id, appends the new, deletes, and swaps categories', () => {
    const v = makeVault({
      api_keys: entries(),
      user_categories: ['x'],
      projects: [
        makeProject({ id: 'Universal', name: 'U' }),
        makeProject({ id: 'web', name: 'W' }),
      ],
    });
    applyPatchLocal(v, {
      put: [
        makeEntry({ id: 'a', provider: 'A2' } as any),
        makeEntry({ id: 'n', provider: 'N' } as any),
      ],
      delete: ['b'],
      projects_put: [makeProject({ id: 'web', name: 'W2' }), makeProject({ id: 'ops', name: 'O' })],
      projects_delete: ['Universal'],
      categories: ['y'],
    });
    expect(v.api_keys.map((e) => `${e.id}:${e.provider}`)).toEqual(['a:A2', 'n:N']);
    expect(v.projects.map((p) => `${p.id}:${p.name}`)).toEqual(['web:W2', 'ops:O']);
    expect(v.user_categories).toEqual(['y']);
    applyPatchLocal(v, {});
    expect(v.user_categories).toEqual(['y']);
  });
});
