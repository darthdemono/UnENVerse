// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
/**
 * Saving an entry from the add/edit form: what is refused and why, and what a
 * successful save does to the entry and to everything that points at it.
 *
 * Each refusal is a promise that a bad entry never reaches the vault, and each
 * message has to say what to fix (a toast that only says "failed" sends people
 * hunting).
 */
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { st } from '../src/ts/state';
import { saveModal, fillForm, populateProjectSelect, buildCatChips } from '../src/ts/modals';
import { loadRealIndexHtml, makeEntry, makeVault, resetState } from './helpers';

const toasts: string[] = [];
const confirms: string[] = [];
let confirmAnswer = true;
let duringConfirm: (() => void) | null = null;
vi.mock('../src/ts/utils', async (importOriginal) => {
  const real = await importOriginal<typeof import('../src/ts/utils')>();
  return {
    ...real,
    showToast: (m: string) => toasts.push(m),
    showConfirm: async (m: string) => {
      confirms.push(m);
      duringConfirm?.();
      return confirmAnswer;
    },
  };
});

const $ = <T extends HTMLElement = HTMLElement>(id: string) => document.getElementById(id) as T;
const set = (id: string, v: string) => {
  const el = $<HTMLInputElement>(id);
  el.value = v;
  el.dispatchEvent(new Event('input'));
};

/** Open the form on a blank "Add" or on entry `idx` of the vault. */
function form(entry: object | null, others: object[] = []) {
  const entries = entry
    ? [
        makeEntry({ id: 'e0', provider: 'Stripe', api_key: 'sk_live_1', ...entry } as never),
        ...others.map((o) => makeEntry(o as never)),
      ]
    : [];
  st.vault = makeVault({ api_keys: entries });
  st.vaultOpen = true;
  st.store = { save: async () => {}, isRemote: false } as never;
  populateProjectSelect();
  buildCatChips([]);
  if (entry) {
    fillForm(entries[0]);
    $<HTMLInputElement>('edit-index').value = '0';
  } else {
    fillForm({} as never);
    $<HTMLInputElement>('edit-index').value = '-1';
  }
  return entries;
}

beforeEach(() => {
  loadRealIndexHtml();
  resetState(st);
  toasts.length = 0;
  confirms.length = 0;
  confirmAnswer = true;
  duringConfirm = null;
  delete (window as any).__TAURI__;
});

describe('required values', () => {
  it('a new entry needs a provider and a value', async () => {
    form(null);
    await saveModal();
    expect(toasts[0]).toMatch(/is required/);
    set('f-provider', 'GitHub');
    await saveModal();
    expect(toasts[1]).toMatch(/is required/);
    expect(st.vault.api_keys).toHaveLength(0);
  });

  it('refuses a blank provider on edit, leaving the stored entry as it was', async () => {
    form({});
    set('f-provider', '   ');
    await saveModal();
    expect(toasts[0]).toMatch(/is required/);
    expect(st.vault.api_keys[0].provider).toBe('Stripe');
  });

  it('a certificate needs its certificate data; a file entry its path; a composite its template', async () => {
    form(null);
    $<HTMLSelectElement>('f-secret-type').value = 'certificate';
    $('f-secret-type').dispatchEvent(new Event('change'));
    set('f-provider', 'site.test');
    await saveModal();
    expect(toasts).toContain('Certificate data is required');
    $<HTMLSelectElement>('f-secret-type').value = 'file_blob';
    $('f-secret-type').dispatchEvent(new Event('change'));
    await saveModal();
    expect(toasts).toContain('File path/reference is required');
    $<HTMLSelectElement>('f-secret-type').value = 'composite';
    $('f-secret-type').dispatchEvent(new Event('change'));
    await saveModal();
    expect(toasts).toContain('Template is required');
    expect(st.vault.api_keys).toHaveLength(0);
  });

  it('an environment-variable entry may hold named variables instead of a value, but not nothing', async () => {
    form(null);
    $<HTMLSelectElement>('f-secret-type').value = 'env_var';
    $('f-secret-type').dispatchEvent(new Event('change'));
    set('f-provider', 'App');
    await saveModal();
    expect(toasts).toContain('Add a value, or at least one named variable below');
    expect(st.vault.api_keys).toHaveLength(0);
  });
});

describe('names', () => {
  it('a label may only be letters, digits, _ and -, because it becomes part of a variable name', async () => {
    form({});
    for (const bad of ['has space', 'ünï', '-lead', 'x'.repeat(25)]) {
      toasts.length = 0;
      set('f-label', bad);
      await saveModal();
      expect(toasts[0], bad).toContain('Name label: letters, digits');
    }
    set('f-label', 'game-2');
    await saveModal();
    expect(st.vault.api_keys[0].label).toBe('game-2');
  });

  it('an entry from before ids existed cannot be given a label until it has one', async () => {
    const [e] = form({});
    delete (e as any).id;
    set('f-label', 'game');
    await saveModal();
    expect(toasts[0]).toContain('predates stable ids');
    expect(st.vault.api_keys[0].label).toBeUndefined();
  });

  it('renaming something already copied asks first and names the old and new variable', async () => {
    form({ last_copied_name: 'STRIPE' });
    set('f-provider', 'StripeLive');
    confirmAnswer = false;
    await saveModal();
    expect(confirms[0]).toContain('STRIPE  →  STRIPELIVE');
    expect(st.vault.api_keys[0].provider).toBe('Stripe');
    confirmAnswer = true;
    await saveModal();
    expect(st.vault.api_keys[0].provider).toBe('StripeLive');
  });

  it('an entry never copied is renamed without a question', async () => {
    form({});
    set('f-provider', 'StripeLive');
    await saveModal();
    expect(confirms).toHaveLength(0);
    expect(st.vault.api_keys[0].provider).toBe('StripeLive');
  });

  it('a rename that keeps the generated name asks nothing', async () => {
    form({ last_copied_name: 'STRIPE' });
    set('f-keyid', '');
    set('f-key', 'sk_live_2');
    await saveModal();
    expect(confirms).toHaveLength(0);
    expect(st.vault.api_keys[0].api_key).toBe('sk_live_2');
  });
});

describe('two-factor seeds', () => {
  it('a seed that cannot produce a code is refused with the reason, so the entry never shows a permanently blank code', async () => {
    form({});
    set('f-totp', 'not a seed!!');
    await saveModal();
    expect(toasts.some((t) => t.startsWith('Two-factor seed:'))).toBe(true);
    expect(st.vault.api_keys[0].totp_secret).toBeUndefined();
  });

  it('a good seed is stored', async () => {
    form({});
    set('f-totp', 'JBSWY3DPEHPK3PXP');
    await saveModal();
    expect(st.vault.api_keys[0].totp_secret).toBe('JBSWY3DPEHPK3PXP');
  });
});

describe('recovery phrases', () => {
  it('offers to save anyway when a phrase does not validate, and saves nothing if declined', async () => {
    (window as any).__TAURI__ = {
      core: {
        invoke: async (c: string) => {
          if (c === 'bip39_validate') throw new Error('checksum mismatch');
          return null;
        },
      },
    };
    form(null);
    $<HTMLSelectElement>('f-secret-type').value = 'crypto_wallet';
    $('f-secret-type').dispatchEvent(new Event('change'));
    set('f-provider', 'Wallet');
    set('f-key', 'abandon abandon abandon');
    confirmAnswer = false;
    await saveModal();
    expect(confirms[0]).toContain('does not validate (checksum mismatch). Save anyway?');
    expect(st.vault.api_keys).toHaveLength(0);
    confirmAnswer = true;
    await saveModal();
    expect(st.vault.api_keys).toHaveLength(1);
  });
});

describe('what a save does', () => {
  it('a new entry gets an id and a creation date, and the form closes', async () => {
    form(null);
    set('f-provider', 'GitHub');
    set('f-key', 'ghp_token');
    await saveModal();
    const saved = st.vault.api_keys[0];
    expect(saved.id).toBeTruthy();
    expect(Date.parse(saved.created_at!)).not.toBeNaN();
    expect(saved).toMatchObject({ provider: 'GitHub', api_key: 'ghp_token' });
    expect($('modal-overlay').classList.contains('open')).toBe(false);
    expect($('load-banner').style.display).toBe('none');
  });

  it('an edit keeps the id and the fields the form does not show', async () => {
    form({
      created_at: '2026-01-01T00:00:00Z',
      pinned: true,
      last_rotated_at: '2026-02-01',
      version_history: [{ value: 'old', saved_at: '2026-01-02' }],
    });
    set('f-key', 'sk_live_2');
    await saveModal();
    expect(st.vault.api_keys[0]).toMatchObject({
      id: 'e0',
      api_key: 'sk_live_2',
      created_at: '2026-01-01T00:00:00Z',
      pinned: true,
      last_rotated_at: '2026-02-01',
    });
    expect(st.vault.api_keys[0].version_history).toHaveLength(1);
  });

  it('a refused save leaves the form open for correction', async () => {
    form(null);
    $('modal-overlay').classList.add('open');
    await saveModal();
    expect($('modal-overlay').classList.contains('open')).toBe(true);
  });

  it('the Save button refuses once the vault has locked underneath the form', async () => {
    form({});
    st.vaultOpen = false;
    set('f-key', 'x');
    await saveModal();
    expect(toasts).toContain('The vault is locked. Unlock it to save.');
    expect(st.vault.api_keys[0].api_key).toBe('sk_live_1');
  });

  it('an edit whose entry was deleted elsewhere while a question was open is not written over a neighbour', async () => {
    form({ last_copied_name: 'STRIPE' }, [{ id: 'other', provider: 'Other', api_key: 'o' }]);
    set('f-provider', 'StripeLive');
    set('f-key', 'mine');
    duringConfirm = () => {
      st.vault.api_keys = st.vault.api_keys.filter((x) => x.id !== 'e0');
    };
    await saveModal();
    duringConfirm = null;
    expect(toasts.some((t) => t.includes('changed or deleted elsewhere'))).toBe(true);
    expect(st.vault.api_keys.map((x) => x.api_key)).toEqual(['o']);
  });
});
