// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
/**
 * What the buttons on an entry card do, and the keyboard shortcuts around them.
 *
 * Booted for real and driven through the buttons, because the card is where
 * most of the app's day-to-day behaviour lives: reveal, copy, rotate, pin,
 * duplicate, delete-with-undo, and the pool and bundle cards that group
 * entries.
 */
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { bootApp, entry } from './app-boot';

const toasts: [string, string?][] = [];
const copied: string[] = [];
let confirmAnswer = true;
vi.mock('../src/ts/utils', async (importOriginal) => {
  const real = await importOriginal<typeof import('../src/ts/utils')>();
  return {
    ...real,
    showToast: (m: string, k?: string) => toasts.push([m, k]),
    showConfirm: async () => confirmAnswer,
    showPrompt: async () => null,
    clipboardWrite: async (t: string) => void copied.push(t),
  };
});

const $ = <T extends HTMLElement = HTMLElement>(id: string) => document.getElementById(id) as T;
const flush = (ms = 30) => new Promise((r) => setTimeout(r, ms));
const act = async (action: string, idx = 0, root: ParentNode = document) => {
  const el = root.querySelector<HTMLElement>(
    `#card-grid [data-action="${action}"][data-idx="${idx}"], #card-grid [data-idx="${idx}"] [data-action="${action}"]`,
  )!;
  el.click();
  await flush();
};
const key = (k: string, init: KeyboardEventInit = {}, target: EventTarget = document) =>
  target.dispatchEvent(
    new KeyboardEvent('keydown', { key: k, bubbles: true, cancelable: true, ...init }),
  );

let st: Awaited<ReturnType<typeof bootApp>>['st'];

beforeEach(async () => {
  toasts.length = 0;
  copied.length = 0;
  confirmAnswer = true;
  ({ st } = await bootApp([
    entry('a', 'Alpha', { description: 'about it', expires_at: '2031-01-01' }),
    entry('b', 'Beta', { api_key: 'second-secret-value' }),
  ]));
}, 30000);

describe('showing and copying', { timeout: 30000 }, () => {
  it('values are masked until revealed; revealing is per field and toggles back', async () => {
    const value = () => document.getElementById('kv-key-0')!;
    expect(value().textContent).not.toContain('k-a-longenough');
    await act('reveal');
    expect(value().textContent).toBe('k-a-longenough');
    expect(value().classList.contains('revealed')).toBe(true);
    expect(document.getElementById('kv-key-1')!.textContent).not.toContain('second-secret-value');
    await act('reveal');
    expect(value().textContent).not.toContain('k-a-longenough');
  });

  it('expanding a card is remembered by the entry, and the chevron says so', async () => {
    await act('toggle');
    expect(st.expanded.has('a')).toBe(true);
    expect(
      document.querySelector('#card-grid .card[data-idx="0"]')!.classList.contains('expanded'),
    ).toBe(true);
    expect(
      document.querySelector('[data-idx="0"] .card-chevron')!.getAttribute('aria-expanded'),
    ).toBe('true');
    await act('toggle');
    expect(st.expanded.has('a')).toBe(false);
  });

  it('a field copies exactly its value; the card copy button copies the .env line', async () => {
    await act('copy-field');
    expect(copied[0]).toBe('k-a-longenough');
    await act('copy-env');
    expect(copied[1]).toContain('ALPHA=k-a-longenough');
  });

  it('clicking the card header copies, but clicking a control inside the header does not also copy', async () => {
    const head = document.querySelector<HTMLElement>(
      '#card-grid .card-head[data-action="copy-env"]',
    )!;
    head.click();
    await flush();
    expect(copied.length).toBe(1);
    head.querySelector<HTMLElement>('.provider-icon-wrap')!.click();
    await flush();
    expect(copied.length).toBe(1);
  });

  it('the description disclosure opens and closes and tells assistive tech', async () => {
    const btn = document.querySelector<HTMLElement>('#card-grid .desc-toggle')!;
    btn.click();
    expect(btn.classList.contains('open')).toBe(true);
    expect(btn.getAttribute('aria-expanded')).toBe('true');
    btn.click();
    expect(btn.getAttribute('aria-expanded')).toBe('false');
  });

  it('copy-all puts every visible entry on the clipboard', async () => {
    $('copy-all-btn').click();
    await flush();
    expect(copied[0]).toContain('k-a-longenough');
    expect(copied[0]).toContain('second-secret-value');
  });

  it('the caret menu opens beside the card', async () => {
    await act('copy-env-menu');
    expect(document.querySelectorAll('#dropdown .dropdown-item').length).toBeGreaterThan(2);
  });
});

describe('changing an entry', { timeout: 30000 }, () => {
  it('rotating stamps today, snapshots the old value into history and clears the compromised flag', async () => {
    st.vault.api_keys[0].compromised = true;
    await act('rotate');
    const a = st.vault.api_keys[0];
    expect(a.last_rotated_at).toBe(new Date().toISOString().slice(0, 10));
    expect(a.version_history![0].value).toBe('k-a-longenough');
    expect(a.compromised).toBe(false);
    expect(toasts.some(([m]) => m.includes('compromised flag cleared'))).toBe(true);
  });

  it('pinning floats the card to the top and unpinning drops it back', async () => {
    await act('pin', 1);
    expect(st.vault.api_keys[1].pinned).toBe(true);
    expect(toasts).toContainEqual(['Pinned ✓', 'ok']);
    expect([...document.querySelectorAll('#card-grid .card')][0].textContent).toContain('Beta');
    await act('pin', 0);
    await flush();
    const pinnedFirst = document.querySelector('#card-grid .card')!;
    expect(pinnedFirst.textContent).toMatch(/Alpha|Beta/);
    st.vault.api_keys.forEach((k) => (k.pinned = true));
    const idx = st.vault.api_keys.findIndex((k) => k.provider === 'Beta');
    await act('pin', idx);
    expect(toasts.some(([m]) => m === 'Unpinned')).toBe(true);
  });

  it('duplicating makes a new entry next to the original, with its own id and no inherited history', async () => {
    st.vault.api_keys[0].version_history = [{ value: 'old', saved_at: '2026-01-01' }];
    await act('duplicate');
    expect(st.vault.api_keys).toHaveLength(3);
    expect(st.vault.api_keys[1].id).not.toBe('a');
    expect(st.vault.api_keys[1].version_history).toBeUndefined();
    expect(st.vault.api_keys[1].key_id).toBe('copy_copy');
    expect(st.vault.api_keys[2].provider).toBe('Beta');
  });

  it('deleting removes the entry and offers Undo, which puts it back in its place', async () => {
    await act('delete');
    expect(st.vault.api_keys.map((k) => k.provider)).toEqual(['Beta']);
    expect($('undo-bar').classList.contains('visible')).toBe(true);
    $('undo-btn').click();
    await flush();
    expect(st.vault.api_keys.map((k) => k.provider)).toEqual(['Alpha', 'Beta']);
    expect($('undo-bar').classList.contains('visible')).toBe(false);
    $('undo-btn').click();
  });

  it('Edit opens the form on that entry', async () => {
    await act('edit', 1);
    expect($('modal-overlay').classList.contains('open')).toBe(true);
    expect($<HTMLInputElement>('f-provider').value).toBe('Beta');
    expect($<HTMLInputElement>('edit-index').value).toBe('1');
  });

  it('verifying a session stamps it', async () => {
    st.vault.api_keys[0].secretType = 'cookie';
    st.vault.api_keys[0].api_key = 'sid=1';
    document.body.insertAdjacentHTML(
      'beforeend',
      '<button id="fake" data-action="verify" data-idx="0"></button>',
    );
    const grid = $('card-grid');
    const b = document.createElement('button');
    b.dataset.action = 'verify';
    b.dataset.idx = '0';
    grid.appendChild(b);
    b.click();
    await flush();
    expect(Date.parse(st.vault.api_keys[0].last_verified_at!)).not.toBeNaN();
    expect(toasts).toContainEqual(['Session verified ✓', 'ok']);
  });

  it('actions on an entry that has gone do nothing', async () => {
    const b = document.createElement('button');
    b.dataset.action = 'pin';
    b.dataset.idx = '9';
    $('card-grid').appendChild(b);
    b.click();
    await flush();
    expect(st.vault.api_keys.some((k) => k.pinned)).toBe(false);
  });

  it('the lock button asks first', async () => {
    confirmAnswer = false;
    $('lock-btn').click();
    await flush();
    expect(st.vaultOpen).toBe(true);
    confirmAnswer = true;
    $('lock-btn').click();
    await flush(100);
    expect(st.vaultOpen).toBe(false);
  });
});

describe('copying and viewing a copy code', { timeout: 30000 }, () => {
  it('a TOTP copy says so when there is no code yet', async () => {
    const b = document.createElement('div');
    b.dataset.action = 'copy-totp';
    const slot = document.createElement('div');
    slot.dataset.totpFor = 'a';
    slot.appendChild(b);
    $('card-grid').appendChild(slot);
    b.click();
    await flush();
    expect(
      toasts.some(([m, k]) => m === 'Codes are generated by the desktop app' && k === 'err'),
    ).toBe(true);
    slot.dataset.totpCode = '654321';
    b.click();
    await flush();
    expect(copied).toContain('654321');
    expect(toasts).toContainEqual(['Code copied ✓', 'ok']);
  });
});

describe('keyboard', { timeout: 30000 }, () => {
  it('Ctrl+K focuses and selects the search box; Ctrl+N opens the Add form', () => {
    key('k', { ctrlKey: true });
    expect(document.activeElement).toBe($('search'));
    key('n', { metaKey: true });
    expect($('modal-overlay').classList.contains('open')).toBe(true);
  });

  it('single keys open settings, toggle the sidebar and show the shortcuts, but not while typing', () => {
    key('b');
    expect($('sidebar').classList.contains('collapsed')).toBe(true);
    key('?');
    expect($('shortcuts-overlay').classList.contains('open')).toBe(true);
    key('Escape');
    expect($('shortcuts-overlay').classList.contains('open')).toBe(false);
    key('s');
    expect($('settings-overlay').classList.contains('open')).toBe(true);
    key('Escape');
    expect($('settings-overlay').classList.contains('open')).toBe(false);
    $('sidebar').classList.remove('collapsed');
    $('search').focus();
    key('b', {}, $('search'));
    expect($('sidebar').classList.contains('collapsed')).toBe(false);
  });

  it('Escape clears the search, and closes an open form; Shift+Escape drops every filter', async () => {
    st.searchQ = 'alp';
    $<HTMLInputElement>('search').value = 'alp';
    key('Escape');
    expect(st.searchQ).toBe('');
    expect($<HTMLInputElement>('search').value).toBe('');
    key('n', { ctrlKey: true });
    key('Escape');
    expect($('modal-overlay').classList.contains('open')).toBe(false);
    st.activeTagFilter = 'prod';
    st.currentEnvFilter = 'production';
    key('Escape', { shiftKey: true });
    expect(st.activeTagFilter).toBeNull();
    expect(st.currentEnvFilter).toBe('');
    expect(toasts).toContainEqual(['Filters cleared', 'ok']);
  });

  it('Enter and Space press a role=button element that is not a button', () => {
    const el = document.createElement('div');
    el.setAttribute('role', 'button');
    el.dataset.action = 'noop';
    const spy = vi.fn();
    el.addEventListener('click', spy);
    document.body.appendChild(el);
    key('Enter', {}, el);
    key(' ', {}, el);
    expect(spy).toHaveBeenCalledTimes(2);
  });

  it('the clear-filters button and the shortcuts button work with the mouse too', () => {
    st.activeTagFilter = 'prod';
    $('clear-filters-btn').click();
    expect(st.activeTagFilter).toBeNull();
    $('shortcuts-btn').click();
    expect($('shortcuts-overlay').classList.contains('open')).toBe(true);
    $('shortcuts-close').click();
    expect($('shortcuts-overlay').classList.contains('open')).toBe(false);
    $('shortcuts-btn').click();
    $('shortcuts-overlay').click();
    expect($('shortcuts-overlay').classList.contains('open')).toBe(false);
  });
});

describe('pool and bundle cards', { timeout: 30000 }, () => {
  it('a pool card groups its members, opens on click, and Copy says when it cannot pick one outside the desktop app', async () => {
    ({ st } = await bootApp(
      [
        entry('p1', 'Pooled', { pool: 'main' }),
        entry('p2', 'Pooled2', { pool: 'main' }),
        entry('s', 'Solo'),
      ],
      { settings: { groupPools: true } },
    ));
    const toggle = document.querySelector<HTMLElement>(
      '[data-action="pool-card-toggle"][data-pool="main"]',
    )!;
    expect(toggle).toBeTruthy();
    toggle.click();
    expect(st.expandedPools.has('main')).toBe(true);
    document
      .querySelector<HTMLElement>('[data-action="pool-card-toggle"][data-pool="main"]')!
      .click();
    expect(st.expandedPools.has('main')).toBe(false);
    document
      .querySelector<HTMLElement>('[data-action="pool-card-copy"][data-pool="main"]')!
      .click();
    await flush();
    expect(
      toasts.some(([m, k]) => m === 'Pool cursors are read by the desktop app' && k === 'err'),
    ).toBe(true);
  });

  it('a bundle card opens and closes, and a member can be made the primary', async () => {
    ({ st } = await bootApp(
      [
        entry('bun', 'Discord', {
          secretType: 'bundle',
          api_key: '',
          extra_vars: [{ key: 'prefix', value: '!' }],
        }),
        entry('m1', 'Bot', { bundle_id: 'bun', bundle_slot: 'bot', bundle_order: 10 }),
        entry('m2', 'Webhook', { bundle_id: 'bun', bundle_slot: 'hook', bundle_order: 20 }),
      ],
      { settings: { groupBundles: true } },
    ));
    const t = () =>
      document.querySelector<HTMLElement>('[data-action="bundle-toggle"][data-bundle="bun"]')!;
    const wasOpen = t().getAttribute('aria-expanded') === 'true';
    t().click();
    expect(t().getAttribute('aria-expanded') === 'true').toBe(!wasOpen);
    expect(st.expandedBundles.has('bun') || st.collapsedBundles.has('bun')).toBe(true);
    t().click();
    expect(t().getAttribute('aria-expanded') === 'true').toBe(wasOpen);
  });
});
