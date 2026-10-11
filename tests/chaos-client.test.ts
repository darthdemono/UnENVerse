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
  removeBundleLive,
  spendRecoveryCodeLive,
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

describe('overlapping saves', () => {
  /**
   * BUG (C9): two persist() calls overlapped. The first to finish took its "what the store
   * holds" snapshot from the LIVE document, which already contained the second call's edit.
   * When the second save then failed, the snapshot claimed the edit was stored, the next
   * save found no difference and sent nothing: the edit was lost with no error left to see.
   */
  it('does not count an edit as stored because an earlier, overlapping save succeeded', async () => {
    await persist(); // establishes the synced state
    const pending: { p: VaultPatch; done: (ok: boolean) => void }[] = [];
    let failed = false;
    store.saveRows = (p) =>
      new Promise((resolve) => {
        pending.push({
          p,
          done: (ok) => {
            failed = !ok;
            if (ok) applyPatchLocal(store.stored, clone(p));
            resolve(true);
          },
        });
      });
    store.lastSaveFailed = () => failed;
    const until = async (n: number) => {
      for (let i = 0; i < 200 && pending.length < n; i++) await Promise.resolve();
    };

    st.vault.api_keys[0].api_key = 'edit-a';
    const first = persist();
    await until(1);
    st.vault.api_keys[1].api_key = 'edit-b';
    const second = persist();
    pending[0].done(true);
    await until(2);
    pending[1].done(false); // the second save fails on the wire
    await Promise.all([first, second]);

    // Back to a healthy connection: the next save must still carry edit-b.
    store.saveRows = (p) => {
      applyPatchLocal(store.stored, clone(p));
      return Promise.resolve(true);
    };
    failed = false;
    await persist();
    expect(store.stored.api_keys.find((x) => x.id === 'b')?.api_key).toBe('edit-b');
  });

  /**
   * BUG (C9): saves from one client were sent concurrently, so the server could apply the
   * newer one first and the older one over it, and both read one shared failure flag.
   */
  it('sends one save at a time and the newest edit lands last', async () => {
    await persist();
    let inFlight = 0;
    let most = 0;
    const order: string[] = [];
    store.saveRows = async (p) => {
      inFlight++;
      most = Math.max(most, inFlight);
      await new Promise((r) => setTimeout(r, 5));
      order.push(String(p.put?.[0]?.api_key));
      applyPatchLocal(store.stored, clone(p));
      inFlight--;
      return true;
    };
    st.vault.api_keys[0].api_key = 'one';
    const a = persist();
    st.vault.api_keys[0].api_key = 'two';
    const b = persist();
    await Promise.all([a, b]);
    expect(most).toBe(1);
    // Whatever was sent, the newest value is the last one the server saw.
    expect(order[order.length - 1]).toBe('two');
    expect(store.stored.api_keys[0].api_key).toBe('two');
  });
});

describe('objects captured before a dialog', () => {
  /**
   * BUG (C12): "delete bundle and members" collected the entries, awaited the confirm
   * dialog, then filtered by those objects. A reload during the dialog replaced every
   * entry object, so nothing was removed while the toast and undo entry said it was.
   */
  it('deletes the bundle from the vault as it is after the dialog, not before', () => {
    const bundle = { ...e('bun'), secretType: 'bundle' } as VaultEntry;
    const member = { ...e('m1'), bundle_id: 'bun' } as VaultEntry;
    st.vault.api_keys = [e('a'), bundle, member];
    const doomedBefore = st.vault.api_keys.filter((x) => x.id === 'bun' || x.bundle_id === 'bun');
    // A reload while the dialog was open: same data, new objects.
    st.vault.api_keys = clone(st.vault.api_keys);
    const { removed, positions } = removeBundleLive('bun');
    expect(removed.map((x) => x.id)).toEqual(['bun', 'm1']);
    expect(positions).toEqual([1, 2]);
    expect(st.vault.api_keys.map((x) => x.id)).toEqual(['a']);
    expect(doomedBefore.some((x) => st.vault.api_keys.includes(x))).toBe(false);
  });

  it('includes a member another writer added while the dialog was open', () => {
    st.vault.api_keys = [{ ...e('bun'), secretType: 'bundle' } as VaultEntry];
    st.vault.api_keys.push({ ...e('late'), bundle_id: 'bun' } as VaultEntry);
    expect(removeBundleLive('bun').removed.map((x) => x.id)).toEqual(['bun', 'late']);
  });

  /**
   * BUG (C12): "mark the next recovery code used" kept the entry it found before the
   * dialog; after a reload it edited a detached copy, persisted nothing, and still
   * announced "N codes left".
   */
  it('spends the code on the entry that is in the vault now', () => {
    const codes = (v: string) =>
      ({
        ...e('rc'),
        secretType: 'recovery_codes',
        extra_vars: [{ key: 'codes', value: v }],
      }) as VaultEntry;
    st.vault.api_keys = [codes('one\ntwo')];
    const stale = st.vault.api_keys[0];
    st.vault.api_keys = [codes('one\ntwo')]; // reload
    const r = spendRecoveryCodeLive('rc', '2026-10-11');
    expect('entry' in r).toBe(true);
    expect(st.vault.api_keys[0].extra_vars?.[0].value).toContain('one\tUSED 2026-10-11');
    expect(stale.extra_vars?.[0].value).toBe('one\ntwo');
  });

  it('says so when the entry was deleted during the dialog', () => {
    st.vault.api_keys = [];
    expect(spendRecoveryCodeLive('rc', '2026-10-11')).toHaveProperty('error');
  });
});
