/**
 * Several writers, one vault (chaos testing, desktop client level). The store is a fake
 * with a "server" document behind it, so another writer can act between this client's
 * read and its save. Each test names the bug it was written for; revert the fix and the
 * test must fail.
 */
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { describe, it, expect, beforeEach, vi } from 'vitest';

vi.mock('../src/ts/utils', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../src/ts/utils')>();
  return { ...actual, showConfirm: vi.fn(() => Promise.resolve(true)) };
});

import {
  st,
  persist,
  forgetSynced,
  resetViewState,
  dropPreviousVault,
  applyPatchLocal,
  VaultConflictError,
  type VaultStore,
  type VaultPatch,
} from '../src/ts/state';
import { showConfirm } from '../src/ts/utils';
import { loadRealIndexHtml } from './helpers';
import { resolveEditTarget } from '../src/ts/modals';
import type { VaultData, VaultEntry } from '../src/ts/types';

const clone = <T>(v: T): T => JSON.parse(JSON.stringify(v)) as T;
const e = (id: string, key = 'k') => ({ id, provider: id, api_key: key }) as VaultEntry;
const UNIVERSAL = { id: 'Universal', name: 'Universal', description: '' };

/** A store backed by a "server" document that other writers can change. */
class FakeStore implements VaultStore {
  isRemote = true;
  vaultId = 'fake';
  stored: VaultData;
  conflictNext = false;
  mergedNext = false;
  loadFails = false;
  stale = 0;
  constructor(doc: VaultData) {
    this.stored = clone(doc);
  }
  load(): Promise<VaultData | null> {
    return Promise.resolve(this.loadFails ? null : clone(this.stored));
  }
  save(d: VaultData): Promise<void> {
    if (this.conflictNext) {
      this.conflictNext = false;
      throw new VaultConflictError('a');
    }
    this.stored = clone(d);
    return Promise.resolve();
  }
  saveRows(p: VaultPatch): Promise<boolean> {
    if (this.conflictNext) {
      this.conflictNext = false;
      throw new VaultConflictError('a');
    }
    applyPatchLocal(this.stored, clone(p));
    return Promise.resolve(true);
  }
  takeMerged(): boolean {
    const m = this.mergedNext;
    this.mergedNext = false;
    return m;
  }
  markStale(): void {
    this.stale++;
  }
  lastSaveFailed = () => false;
}

let store: FakeStore;
beforeEach(() => {
  loadRealIndexHtml();
  forgetSynced();
  vi.mocked(showConfirm).mockReset();
  vi.mocked(showConfirm).mockResolvedValue(true);
  const doc = { api_keys: [e('a'), e('b')], user_categories: [], projects: [{ ...UNIVERSAL }] };
  store = new FakeStore(doc);
  st.store = store;
  st.vault = clone(doc);
  st.undoStack = [];
});

describe('the conflict dialog', () => {
  /**
   * BUG: "keep yours" force-saved the whole local copy with no expected version, which
   * deletes every row the other writer added (this copy never had them), and a remote store
   * had no such write at all, so the choice silently discarded the user's edit.
   */
  it('keeps my change on top of theirs and loses nothing the other writer added', async () => {
    await persist(); // first save: whole document, establishes the synced state
    st.vault.api_keys[0].api_key = 'my-edit';
    // Meanwhile another writer adds an entry; my next save conflicts.
    store.stored.api_keys.push(e('theirs', 'their-key'));
    store.conflictNext = true;
    await persist();
    const ids = store.stored.api_keys.map((x) => x.id);
    expect(ids).toContain('theirs');
    expect(store.stored.api_keys.find((x) => x.id === 'a')?.api_key).toBe('my-edit');
  });

  it('on cancel adopts theirs and drops my unsaved change', async () => {
    vi.mocked(showConfirm).mockResolvedValue(false);
    await persist();
    st.vault.api_keys[0].api_key = 'my-edit';
    store.stored.api_keys.push(e('theirs'));
    store.conflictNext = true;
    await persist();
    expect(st.vault.api_keys.map((x) => x.id)).toContain('theirs');
    expect(st.vault.api_keys.find((x) => x.id === 'a')?.api_key).toBe('k');
  });
});

describe('after a save that merged another writer in', () => {
  /**
   * BUG: the store adopts the new version after a merged save, and the client reloaded to
   * catch up but ignored a failed reload. Its copy was then behind the version it would
   * save against, so its next whole-document save deleted what the merge had just added.
   */
  it('makes the next save fail its version check when the reload failed', async () => {
    await persist();
    st.vault.api_keys[1].api_key = 'edit';
    store.mergedNext = true;
    store.loadFails = true;
    await persist();
    expect(store.stale).toBe(1);
  });

  it('does not mark the copy stale when the reload worked', async () => {
    await persist();
    st.vault.api_keys[1].api_key = 'edit';
    store.mergedNext = true;
    await persist();
    expect(store.stale).toBe(0);
  });
});

describe('replacing the vault', () => {
  /**
   * BUG: the undo stack survived a vault switch. Deleting an entry and switching vault
   * within five seconds left an Undo whose closure re-inserted the entry into whichever
   * vault was open and saved it there. Only a lock cleared it.
   */
  it('drops pending undos (resetViewState runs on every replacement)', () => {
    const undo = vi.fn();
    st.undoStack.push({ fn: undo, t: setTimeout(() => undefined, 5000) });
    resetViewState();
    expect(st.undoStack).toHaveLength(0);
    expect(undo).not.toHaveBeenCalled();
  });

  /**
   * BUG: when the new store could not load (404 for a vault that does not exist yet, a
   * network error), the previous vault stayed in `st.vault`, painted, and the next edit
   * saved it into the new store.
   */
  it('drops the previous vault when the new store cannot load', () => {
    st.vaultOpen = true;
    dropPreviousVault();
    expect(st.vault.api_keys).toEqual([]);
    expect(st.vault.projects).toEqual([UNIVERSAL]);
    expect(st.vaultOpen).toBe(false);
  });
});

describe('boot wiring', () => {
  // finishInit cannot be driven without the whole app, so pin the part that matters: both
  // ways a store can fail to load (a null result, a thrown error) drop the previous vault.
  it('finishInit drops the previous vault on a null load and on a thrown one', () => {
    const src = readFileSync(join(__dirname, '../src/ts/vault.ts'), 'utf8');
    const body = src.slice(src.indexOf('async function finishInit()'));
    const elseBranch = body.indexOf(
      "document.getElementById('load-banner')!.style.display = 'flex';",
    );
    expect(body.slice(elseBranch - 120, elseBranch)).toContain('dropPreviousVault();');
    const catchAt = body.indexOf('} catch {', elseBranch);
    expect(body.slice(catchAt, catchAt + 160)).toContain('dropPreviousVault();');
  });
});

describe('the entry editor', () => {
  /**
   * BUG: saveModal held an array index across awaits. A reload in between (a merged save,
   * the vault watcher) replaced or reordered the array and the form's contents overwrote a
   * different entry, keeping that entry's id.
   */
  it('finds the edited entry again by identity after the array was replaced', () => {
    const old = e('target');
    const reloaded = [e('other'), clone(old), e('third')]; // fresh objects, new order
    expect(resolveEditTarget(reloaded, old)).toBe(1);
    expect(resolveEditTarget([old, e('x')], old)).toBe(0);
  });

  it('reports an entry another client deleted instead of writing somewhere else', () => {
    expect(resolveEditTarget([e('other')], e('target'))).toBe(-1);
    expect(resolveEditTarget([], { ...e('x'), id: undefined } as VaultEntry)).toBe(-1);
  });

  it('writes through the re-resolved position, never the index it started with', () => {
    const src = readFileSync(join(__dirname, '../src/ts/modals.ts'), 'utf8');
    const save = src.slice(src.indexOf('export async function saveModal'));
    const write = save.slice(save.indexOf('resolveEditTarget(st.vault.api_keys, old)'));
    expect(write).toContain('st.vault.api_keys[at] = {');
    expect(write.slice(0, write.indexOf('\n}\n'))).not.toMatch(/api_keys\[idx\]/);
  });
});
