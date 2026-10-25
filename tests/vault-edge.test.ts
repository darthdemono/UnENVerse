// SPDX-License-Identifier: PolyForm-Strict-1.0.0
// Copyright Jubair Hasan.
/**
 * The grid and shell handlers when the thing they were asked about is no longer
 * there: a card clicked after its entry was deleted, a bundle removed while a
 * dialog was open, a recovery code spent in another window. Each must do
 * nothing visible rather than act on the wrong entry (invariant 1) or throw.
 * Also the few controls whose only job is small: search history, the chunk
 * editor's add-field, the entry form's type selector.
 */
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { bootApp, entry } from './app-boot';

const toasts: [string, string?][] = [];
const copied: string[] = [];
let confirmHook: () => boolean = () => true;
let promptAnswers: (string | null)[] = [];
let clipboardThrows = false;
vi.mock('../src/ts/utils', async (importOriginal) => {
  const real = await importOriginal<typeof import('../src/ts/utils')>();
  return {
    ...real,
    showToast: (m: string, k?: string) => toasts.push([m, k]),
    showConfirm: async () => confirmHook(),
    showPrompt: async () => (promptAnswers.length ? promptAnswers.shift()! : null),
    clipboardWrite: (t: string) => {
      if (clipboardThrows) throw new Error('clipboard denied');
      copied.push(t);
      return Promise.resolve(true);
    },
  };
});

const $ = <T extends HTMLElement = HTMLElement>(id: string) => document.getElementById(id) as T;
const flush = (ms = 40) => new Promise((r) => setTimeout(r, ms));

let st: Awaited<ReturnType<typeof bootApp>>['st'];
let Settings: Awaited<ReturnType<typeof bootApp>>['Settings'];

/** A control the renderer could have drawn, clicked inside the grid. */
async function clickInGrid(attrs: Record<string, string>) {
  const el = document.createElement('button');
  for (const [k, v] of Object.entries(attrs)) el.setAttribute(k, v);
  $('card-grid').appendChild(el);
  el.click();
  await flush();
  el.remove();
}

const codesEntry = (codes: string) =>
  entry('rc', 'Backup codes', {
    secretType: 'recovery_codes',
    api_key: '',
    extra_vars: [{ key: 'codes', value: codes }],
  });

async function boot(extra: object[] = [], settings: object = {}) {
  ({ st, Settings } = await bootApp([entry('a', 'Alpha'), entry('b', 'Beta'), ...extra], {
    settings,
    categories: ['infra'],
  }));
}

beforeEach(async () => {
  toasts.length = 0;
  copied.length = 0;
  confirmHook = () => true;
  promptAnswers = [];
  clipboardThrows = false;
  await boot();
  toasts.length = 0;
  delete (window as never as Record<string, unknown>).__unvIsBulkMode;
  delete (window as never as Record<string, unknown>).__unvBulkToggle;
}, 30000);

describe('clicks on things that are gone', { timeout: 30000 }, () => {
  it('a click on something with no action does nothing', async () => {
    const before = JSON.stringify(st.vault);
    $('card-grid').click();
    await clickInGrid({ class: 'plain' });
    expect(JSON.stringify(st.vault)).toBe(before);
    expect(toasts).toEqual([]);
  });

  it('timeline, verify and pin on an entry that no longer exists do nothing', async () => {
    const before = JSON.stringify(st.vault);
    for (const action of ['timeline', 'verify', 'pin'])
      await clickInGrid({ 'data-action': action, 'data-idx': '99' });
    expect(JSON.stringify(st.vault)).toBe(before);
    expect(toasts).toEqual([]);
  });

  it('the timeline button on a live card says the timeline needs the desktop app or a remote vault', async () => {
    document
      .querySelector<HTMLElement>('#card-grid [data-action="card-more"][data-idx="0"]')!
      .click();
    [...document.querySelectorAll<HTMLElement>('#dropdown .dropdown-item')]
      .find((i) => i.textContent === 'Lifecycle timeline')!
      .click();
    await flush(80);
    expect(toasts).toContainEqual([
      'The lifecycle timeline is available in the desktop app or a remote vault.',
      'info',
    ]);
  });

  it('the bulk-select checkbox toggles that card in bulk mode', async () => {
    const toggled: number[] = [];
    (window as never as Record<string, unknown>).__unvBulkToggle = (i: number) =>
      void toggled.push(i);
    await clickInGrid({ 'data-action': 'bulk-toggle', 'data-idx': '1' });
    expect(toggled).toEqual([1]);
  });

  it('a pool summary in bulk mode ticks every member and does not expand the pool', async () => {
    await bootApp(
      [
        entry('p1', 'PA', { pool: 'main' }),
        entry('p2', 'PB', { pool: 'main' }),
        entry('s', 'Solo'),
      ],
      { settings: { groupPools: true } },
    ).then((s) => (st = s.st));
    const toggled: number[] = [];
    (window as never as Record<string, unknown>).__unvIsBulkMode = () => true;
    (window as never as Record<string, unknown>).__unvBulkToggle = (i: number) =>
      void toggled.push(i);
    // The summary bar is not itself a card, so the card-level shortcut does not catch it.
    document
      .querySelector<HTMLElement>('[data-action="pool-card-toggle"][data-pool="main"]')!
      .click();
    expect(toggled.sort()).toEqual([0, 1]);
    expect(st.expandedPools.has('main')).toBe(false);
  });

  it('copying a pool that has no members does nothing', async () => {
    await clickInGrid({ 'data-action': 'pool-card-copy', 'data-pool': 'ghost' });
    expect(toasts).toEqual([]);
    expect(copied).toEqual([]);
  });

  it('a bundle action naming a bundle that is not there does nothing', async () => {
    const before = JSON.stringify(st.vault);
    for (const action of [
      'bundle-add-var',
      'bundle-var-remove',
      'bundle-add-project',
      'bundle-add-category',
      'bundle-add-tag',
      'bundle-dissolve',
      'bundle-delete-all',
      'bundle-export',
      'bundle-add-member',
      'bundle-remove-member',
      'bundle-rename-slot',
      'bundle-primary',
      'bundle-slot-tab',
      'bundle-order',
    ]) {
      promptAnswers = ['anything'];
      await clickInGrid({
        'data-action': action,
        'data-bundle': 'ghost',
        'data-member': 'a',
        'data-key': 'k',
        'data-direction': '1',
      });
    }
    expect(JSON.stringify(st.vault)).toBe(before);
  });

  it('accepting a bundle suggestion for a provider that no longer has duplicates does nothing', async () => {
    await clickInGrid({ 'data-action': 'bundle-suggest-accept', 'data-provider': 'nobody' });
    expect(st.vault.api_keys.some((e) => e.secretType === 'bundle')).toBe(false);
  });

  it('dismissing a suggestion with no provider records nothing', async () => {
    await clickInGrid({ 'data-action': 'bundle-suggest-dismiss' });
    expect(Settings.get('dismissedBundleSuggestions') ?? []).toEqual([]);
  });

  it('"mark next code used" on an entry that is not a recovery-code entry does nothing', async () => {
    await clickInGrid({ 'data-action': 'codes-use', 'data-idx': '0' });
    expect(toasts).toEqual([]);
  });

  it('right-clicking a card whose index is not a number, or past the end, opens no menu', () => {
    for (const idx of ['x', '99']) {
      const card = document.createElement('div');
      card.dataset.idx = idx;
      $('card-grid').appendChild(card);
      const ev = new MouseEvent('contextmenu', { bubbles: true, cancelable: true });
      card.dispatchEvent(ev);
      expect($('dropdown').style.display).not.toBe('block');
      card.remove();
    }
  });
});

describe('the vault changing while a dialog is open', { timeout: 30000 }, () => {
  const bundleSet = () => [
    entry('bun', 'Discord', { secretType: 'bundle', api_key: '', bundle_primary: 'm1' }),
    entry('m1', 'Bot', { bundle_id: 'bun', bundle_slot: 'bot', bundle_order: 10 }),
    entry('m2', 'Hook', { bundle_id: 'bun', bundle_slot: 'hook', bundle_order: 20 }),
  ];
  const open = async () => {
    await boot(bundleSet(), { groupBundles: true });
    toasts.length = 0;
    if (
      !document.querySelector('[data-action="bundle-var-remove"], [data-action="bundle-add-var"]')
    )
      document
        .querySelector<HTMLElement>('[data-action="bundle-toggle"][data-bundle="bun"]')!
        .click();
  };

  it('deleting a bundle that something else already removed says so and leaves the rest', async () => {
    await open();
    confirmHook = () => {
      st.vault.api_keys = st.vault.api_keys.filter((e) => e.id !== 'bun' && e.bundle_id !== 'bun');
      return true;
    };
    document
      .querySelector<HTMLElement>('[data-action="bundle-delete-all"][data-bundle="bun"]')!
      .click();
    await flush(80);
    expect(toasts).toContainEqual(['That bundle was already removed.', 'err']);
    expect(st.undoStack).toHaveLength(0);
  });

  it('removing a member that was dropped from the bundle meanwhile changes nothing', async () => {
    await open();
    // A template that uses the member makes removing it ask first.
    st.vault.projects[0].chunks = [
      {
        id: 'c',
        name: 'app',
        chunk_type: 'env_file',
        fields: [{ key: 'TOKEN', value: '${bundle:Discord/bot}', field_type: 'var' }],
      },
    ] as never;
    const before = JSON.stringify(st.vault);
    confirmHook = () => {
      st.vault.api_keys.find((e) => e.id === 'm1')!.bundle_id = undefined;
      return true;
    };
    document
      .querySelector<HTMLElement>('[data-action="bundle-remove-member"][data-member="m1"]')!
      .click();
    await flush(80);
    // Only the change the hook made: the dialog's answer was not applied to the stale member.
    expect(st.vault.api_keys.find((e) => e.id === 'bun')!.bundle_primary).toBe('m1');
    expect(st.vault.api_keys.find((e) => e.id === 'm1')!.bundle_slot).toBe('bot');
    expect(JSON.stringify(st.vault).length).toBeLessThan(before.length + 50);
  });

  it('renaming a slot on a member that vanished while the prompt was open changes nothing', async () => {
    await open();
    promptAnswers = ['robot'];
    document
      .querySelector<HTMLElement>('[data-action="bundle-rename-slot"][data-member="m1"]')!
      .click();
    st.vault.api_keys = st.vault.api_keys.filter((e) => e.id !== 'm1');
    await flush(80);
    expect(st.vault.api_keys.some((e) => e.bundle_slot === 'robot')).toBe(false);
  });

  it('a recovery code spent in another window is reported, not marked twice', async () => {
    await boot([codesEntry('one-1111\ntwo-2222')]);
    confirmHook = () => {
      st.vault.api_keys = st.vault.api_keys.filter((e) => e.id !== 'rc');
      return true;
    };
    document.querySelector<HTMLElement>('[data-action="codes-use"]')!.click();
    await flush(80);
    expect(toasts).toContainEqual([
      'That entry changed or was deleted while you were deciding.',
      'err',
    ]);
  });

  it('there is nothing to mark when every code is already used', async () => {
    await boot([codesEntry('one-1111\tUSED 2026-01-01')]);
    const btn = document.querySelector<HTMLButtonElement>('[data-action="codes-use"]')!;
    btn.disabled = false;
    btn.click();
    await flush(80);
    expect(toasts.some(([m]) => m.includes('recovery codes left'))).toBe(false);
  });

  it('a failure while copying a bundle export is reported rather than thrown', async () => {
    await open();
    document
      .querySelector<HTMLElement>('[data-action="bundle-export"][data-bundle="bun"]')!
      .click();
    clipboardThrows = true;
    [...document.querySelectorAll<HTMLElement>('#dropdown .dropdown-item')][0].click();
    await flush();
    expect(toasts[toasts.length - 1]).toEqual(['Export failed: Error: clipboard denied', 'err']);
  });
});

describe('small controls', { timeout: 30000 }, () => {
  it('picking a recent search fills the box, filters the grid and shows the clear button', async () => {
    Settings.set('recentSearches', ['beta']);
    $('search').dispatchEvent(new Event('focus'));
    document.querySelector<HTMLElement>('#search-history [data-recent="beta"]')!.click();
    await flush();
    expect($<HTMLInputElement>('search').value).toBe('beta');
    expect(st.searchQ).toBe('beta');
    expect($('search-clear').classList.contains('visible')).toBe(true);
    expect([...document.querySelectorAll('#card-grid .card')].map((c) => c.textContent)).toEqual([
      expect.stringContaining('Beta'),
    ]);
  });

  it('a sort mode that was saved but no longer exists is ignored', async () => {
    await boot([], { lastSortBy: 'removed-mode' });
    expect(st.currentSortBy).toBe('provider');
  });

  it('the entry form’s type selector shows the fields that type needs', async () => {
    $('add-btn').click();
    await flush();
    const sel = $<HTMLSelectElement>('f-secret-type');
    const before = document.getElementById('modal-overlay')!.innerHTML;
    sel.value = 'bundle';
    sel.dispatchEvent(new Event('change', { bubbles: true }));
    await flush();
    expect(document.getElementById('modal-overlay')!.innerHTML).not.toBe(before);
  });

  it('Save on an empty form refuses and keeps the form open', async () => {
    $('add-btn').click();
    await flush();
    $('modal-save').click();
    await flush();
    expect($('modal-overlay').classList.contains('open')).toBe(true);
    expect(st.vault.api_keys).toHaveLength(2);
  });

  it('the chunk editor closes on a backdrop click, and + Add Field appends an empty row', async () => {
    $('chunk-edit-overlay').classList.add('open');
    $('chunk-edit-add-field').click();
    const rows = () => $('chunk-edit-fields').children.length;
    const one = rows();
    $('chunk-edit-add-field').click();
    expect(rows()).toBe(one + 1);
    $('chunk-edit-overlay').click();
    expect($('chunk-edit-overlay').classList.contains('open')).toBe(false);
  });

  it('exporting authenticator codes in a chosen format reports what happened', async () => {
    $('auth-export-btn').click();
    [...document.querySelectorAll<HTMLElement>('#dropdown .dropdown-item')][0].click();
    await flush(80);
    expect(toasts.length).toBeGreaterThan(0);
  });
});

describe('controls missing the data they need', { timeout: 30000 }, () => {
  it('every grid action with none of its data attributes does nothing and does not throw', async () => {
    const before = JSON.stringify(st.vault);
    for (const action of [
      'toggle-desc',
      'pool-card-toggle',
      'pool-card-copy',
      'bundle-toggle',
      'bundle-slot-tab',
      'bundle-rename-slot',
      'bundle-remove-member',
      'bundle-add-member',
      'bundle-add-var',
      'bundle-var-remove',
      'bundle-add-project',
      'bundle-order',
      'bundle-dissolve',
      'bundle-delete-all',
      'bundle-suggest-accept',
      'bundle-suggest-dismiss',
      'codes-use',
      'bundle-export',
    ]) {
      promptAnswers = [null];
      await clickInGrid({ 'data-action': action });
    }
    // Opening a bundle with no id records an empty id; nothing in the vault changes.
    expect(JSON.stringify(st.vault)).toBe(before);
  });

  it('a sidebar filter button without its value selects nothing', () => {
    for (const cls of ['tag-filter-btn', 'prefix-filter-btn', 'pool-filter-btn']) {
      const btn = document.createElement('button');
      btn.className = `sidebar-item ${cls}`;
      $('sidebar').appendChild(btn);
      btn.click();
      btn.remove();
    }
    expect(st.activeTagFilter || null).toBeNull();
    expect(st.activePrefixFilter || null).toBeNull();
    expect(st.activePoolFilter || null).toBeNull();
    expect(document.querySelectorAll('#card-grid .card')).toHaveLength(2);
  });

  it('a project type button without a type falls back to a generic project', () => {
    $('new-project-btn').click();
    const btn = document.querySelector<HTMLButtonElement>('.project-type-btn')!;
    delete btn.dataset.ptype;
    btn.click();
    expect(btn.getAttribute('aria-checked')).toBe('true');
  });

  it('a click inside the env link dialog does not close it', () => {
    $('env-link-overlay').classList.add('open');
    $('env-link-overlay').firstElementChild!.dispatchEvent(
      new MouseEvent('click', { bubbles: true }),
    );
    expect($('env-link-overlay').classList.contains('open')).toBe(true);
  });

  it('a double-click on an element whose index is not a number does not open the editor', () => {
    const card = document.createElement('div');
    card.dataset.idx = 'abc';
    $('card-grid').appendChild(card);
    card.dispatchEvent(new MouseEvent('dblclick', { bubbles: true }));
    expect($('modal-overlay').classList.contains('open')).toBe(false);
  });

  it('a right-click menu is labelled "Key" when the entry type names no key field', () => {
    const e = st.vault.api_keys[0];
    e.secretType = 'no-such-type' as never;
    document
      .querySelector('#card-grid .card[data-idx="0"]')!
      .dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true }));
    expect(
      [...document.querySelectorAll('#dropdown .dropdown-item')].map((i) => i.textContent?.trim()),
    ).toContain('Copy Key');
  });
});

describe('failing exports and reads', { timeout: 30000 }, () => {
  it('a settings export that cannot be written says why', async () => {
    const utils = await import('../src/ts/utils');
    const spy = vi
      .spyOn(utils, 'saveFile')
      .mockResolvedValueOnce({ ok: false, error: 'disk full' } as never);
    $('settings-export-btn').click();
    await flush();
    expect(toasts).toContainEqual(['Export failed: disk full', 'error']);
    spy.mockResolvedValueOnce({ ok: true } as never);
    $('settings-export-btn').click();
    await flush();
    expect(toasts).toContainEqual(['Exported', 'ok']);
  });
});

describe('the quick key generator', { timeout: 30000 }, () => {
  it('the generate button in the entry form fills the key field', async () => {
    $('add-btn').click();
    await flush();
    $<HTMLInputElement>('f-key').value = '';
    $('f-key-generate').click();
    await flush(80);
    expect($<HTMLInputElement>('f-key').value.length).toBeGreaterThan(8);
  });
});

describe('odd shapes the handlers tolerate', { timeout: 30000 }, () => {
  const win = () => window as never as Record<string, unknown>;

  it('a file read that yields something other than text is handed on as empty text', async () => {
    class FakeReader {
      onload: ((ev: { target: { result: unknown } }) => void) | null = null;
      readAsText() {
        this.onload?.({ target: { result: new ArrayBuffer(4) } });
      }
    }
    vi.stubGlobal('FileReader', FakeReader);
    try {
      $('auth-import-btn').click();
      const inp = document.querySelector<HTMLInputElement>('input[type=file]')!;
      Object.defineProperty(inp, 'files', {
        value: [new File(['x'], 'a.txt')],
        configurable: true,
      });
      inp.dispatchEvent(new Event('change'));
      await flush();
      expect(toasts.length).toBeGreaterThan(0);
    } finally {
      vi.unstubAllGlobals();
    }
  });

  it('an encrypted backup read that yields something other than text is ignored', async () => {
    class FakeReader {
      onload: ((ev: { target: { result: unknown } }) => void) | null = null;
      readAsText() {
        this.onload?.({ target: { result: new ArrayBuffer(4) } });
      }
    }
    vi.stubGlobal('FileReader', FakeReader);
    try {
      $<HTMLInputElement>('settings-backup-pw').value = 'whatever-password';
      $('settings-import-encrypted').click();
      const inp = document.querySelector<HTMLInputElement>('input[type=file]')!;
      Object.defineProperty(inp, 'files', {
        value: [new File(['x'], 'a.vaultbak')],
        configurable: true,
      });
      inp.dispatchEvent(new Event('change'));
      await flush();
      expect(toasts.some(([m]) => /backup|Decryption/i.test(m))).toBe(false);
      expect(document.querySelector('input[type=file]')).toBeNull();
    } finally {
      vi.unstubAllGlobals();
    }
  });

  it('a picker whose input was already removed from the page cancels quietly', () => {
    $('load-banner-browse-btn').click();
    const inp = document.querySelector<HTMLInputElement>('input[type=file]')!;
    inp.remove();
    inp.dispatchEvent(new Event('cancel'));
    expect(document.querySelector('input[type=file]')).toBeNull();
  });

  it('bulk "select visible members" outside any bundle ticks nothing', async () => {
    const toggled: number[] = [];
    win().__unvIsBulkMode = () => true;
    win().__unvBulkToggle = (i: number) => void toggled.push(i);
    await clickInGrid({ 'data-action': 'bundle-bulk-toggle', 'data-bundle': 'x' });
    expect(toggled).toEqual([]);
  });

  it('a pool summary for a pool with no members ticks nothing in bulk mode', async () => {
    const toggled: number[] = [];
    win().__unvIsBulkMode = () => true;
    win().__unvBulkToggle = (i: number) => void toggled.push(i);
    await clickInGrid({ 'data-action': 'pool-card-toggle', 'data-pool': 'ghost' });
    expect(toggled).toEqual([]);
  });

  it('removing a variable from a bundle that has none leaves it with none', async () => {
    await boot([entry('bun', 'NoVars', { secretType: 'bundle', api_key: '' })]);
    await clickInGrid({
      'data-action': 'bundle-var-remove',
      'data-bundle': 'bun',
      'data-key': 'k',
    });
    expect(st.vault.api_keys.find((e) => e.id === 'bun')!.extra_vars).toEqual([]);
  });

  it('an unset dismissed list and an unset saved-remotes list are treated as empty', async () => {
    Settings.set('dismissedBundleSuggestions', undefined as never);
    await clickInGrid({ 'data-action': 'bundle-suggest-dismiss', 'data-provider': 'Zed' });
    expect(Settings.get('dismissedBundleSuggestions')).toEqual(['zed']);
    Settings.set('remoteSaved', undefined as never);
    $('vault-switcher').click();
    expect(
      [...document.querySelectorAll('#dropdown .dropdown-item')].map((i) => i.textContent?.trim()),
    ).toEqual(['⬤ Local Vault', '+ Add Remote Vault', 'Remote settings…']);
  });

  it('choosing the remote you are already connected to does nothing', () => {
    Settings.set('remoteSaved', [{ id: 'r1', name: 'Home', url: 'https://x.test' }] as never);
    st.activeRemoteId = 'r1';
    $('vault-switcher').click();
    [...document.querySelectorAll<HTMLElement>('#dropdown .dropdown-item')]
      .find((i) => i.textContent?.includes('Home'))!
      .click();
    expect(st.activeRemoteId).toBe('r1');
  });

  it('the suggestion banner’s Accept does nothing without a provider to bundle', async () => {
    await bootApp([entry('g1', 'Stripe', { label: 'a' }), entry('g2', 'Stripe', { label: 'b' })], {
      settings: { groupBundles: true },
    }).then((s) => (st = s.st));
    await clickInGrid({ 'data-action': 'bundle-suggest-accept' });
    expect(st.vault.api_keys.some((e) => e.secretType === 'bundle')).toBe(false);
  });

  it('an entry with no type is still offered its key in the right-click menu', () => {
    delete st.vault.api_keys[0].secretType;
    document
      .querySelector('#card-grid .card[data-idx="0"]')!
      .dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true }));
    expect(
      [...document.querySelectorAll('#dropdown .dropdown-item')].map((i) => i.textContent?.trim()),
    ).toContain('Copy API Key');
  });
});
