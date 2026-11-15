// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
// The recipe picker and the "Make from recipe" button in the add and edit form (R41). The generator
// is Rust over IPC; a recording stub stands in for it.
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { st } from '../src/ts/state';
import {
  fillForm,
  openModal,
  populateProjectSelect,
  buildCatChips,
  saveModal,
} from '../src/ts/modals';
import { loadRealIndexHtml, makeEntry, makeVault, resetState } from './helpers';

vi.mock('../src/ts/utils', async (importOriginal) => {
  const real = await importOriginal<typeof import('../src/ts/utils')>();
  return { ...real, showToast: () => {}, showConfirm: async () => true };
});

const $ = <T extends HTMLElement = HTMLElement>(id: string) => document.getElementById(id) as T;
const set = (id: string, v: string) => {
  const el = $<HTMLInputElement>(id);
  el.value = v;
  el.dispatchEvent(new Event('input'));
};
const tick = () => new Promise((r) => setTimeout(r, 0));

let calls: { cmd: string; args: Record<string, unknown> }[] = [];
let answer: (cmd: string, args: Record<string, unknown>) => unknown;

function stub() {
  (window as unknown as { __TAURI__: unknown }).__TAURI__ = {
    core: {
      invoke: async (cmd: string, args: Record<string, unknown>) => {
        calls.push({ cmd, args });
        return answer(cmd, args);
      },
    },
  };
}

function open(entry: object | null, doc: object = {}) {
  const entries = entry
    ? [
        makeEntry({
          id: 'e0',
          provider: 'Instagram',
          api_key: 'old',
          primary_role: 'password',
          ...entry,
        } as never),
      ]
    : [];
  st.vault = makeVault({ api_keys: entries, ...doc } as never);
  st.vaultOpen = true;
  st.store = { save: async () => {}, isRemote: false } as never;
  populateProjectSelect();
  buildCatChips([]);
  openModal(entry ? 'Edit' : 'Add', entry ? 0 : -1);
  fillForm((entry ? entries[0] : {}) as never);
}

const RECIPES = {
  recipes: [
    { name: 'social', template: '{random:20}', options: { max_len: 30 } },
    { name: 'plain', template: '{random:16}' },
  ],
};

beforeEach(() => {
  loadRealIndexHtml();
  resetState(st);
  calls = [];
  answer = () => null;
  delete (window as unknown as { __TAURI__?: unknown }).__TAURI__;
});

afterEach(() => {
  delete (window as unknown as { __TAURI__?: unknown }).__TAURI__;
});

describe('the recipe picker', () => {
  it("offers automatic, none and every saved recipe, and preselects the entry's own pick", () => {
    open({ recipe: 'plain' }, RECIPES);
    const names = [...$<HTMLSelectElement>('f-recipe').options].map((o) => o.value);
    expect(names).toEqual(['', 'none', 'social', 'plain']);
    expect($<HTMLSelectElement>('f-recipe').value).toBe('plain');
  });

  it('falls back to automatic when the picked recipe no longer exists, and says how a password was made', () => {
    open(
      {
        recipe: 'gone',
        generated_by: { recipe: 'social', version: 1, counter: 2, at: '2026-11-15' },
      },
      RECIPES,
    );
    expect($<HTMLSelectElement>('f-recipe').value).toBe('');
    expect($('f-recipe-note').textContent).toBe('Made by recipe social on 2026-11-15.');
  });

  it('saves the choice with the entry and clears it when automatic', async () => {
    open({}, RECIPES);
    $<HTMLSelectElement>('f-recipe').value = 'social';
    await saveModal();
    expect(st.vault.api_keys[0].recipe).toBe('social');
    open({ recipe: 'social' }, RECIPES);
    $<HTMLSelectElement>('f-recipe').value = '';
    await saveModal();
    expect(st.vault.api_keys[0].recipe).toBeUndefined();
  });
});

describe('Make from recipe', () => {
  it('needs the desktop app', async () => {
    open({}, RECIPES);
    $('f-recipe-make').click();
    await tick();
    expect($('f-recipe-note').textContent).toContain('desktop app');
  });

  it("asks Rust for one password from the chosen recipe with the form's own values, and records how it was made", async () => {
    stub();
    answer = (cmd) => (cmd === 'recipe_preview' ? { samples: ['Generated-Pass-1'] } : null);
    open({ generated_by: { recipe: 'social', version: 1, counter: 4, at: '2026-01-01' } }, RECIPES);
    $<HTMLSelectElement>('f-recipe').value = 'social';
    $('f-recipe-make').click();
    await tick();
    const c = calls.find((x) => x.cmd === 'recipe_preview')!;
    expect(c.args).toMatchObject({ template: '{random:20}', options: { max_len: 30 }, count: 1 });
    expect((c.args.entry as { provider: string }).provider).toBe('Instagram');
    expect($<HTMLInputElement>('f-key').value).toBe('Generated-Pass-1');
    expect($('f-recipe-note').textContent).toContain('Made from recipe social');
    await saveModal();
    const saved = st.vault.api_keys[0];
    expect(saved.api_key).toBe('Generated-Pass-1');
    expect(saved.generated_by).toMatchObject({ recipe: 'social', version: 1, counter: 5 });
    expect(saved.generated_by!.at).toMatch(/^\d{4}-\d\d-\d\d$/);
  });

  it('drops the record when the value is edited by hand afterwards', async () => {
    stub();
    answer = (cmd) => (cmd === 'recipe_preview' ? { samples: ['Generated-Pass-1'] } : null);
    open({}, RECIPES);
    $<HTMLSelectElement>('f-recipe').value = 'plain';
    $('f-recipe-make').click();
    await tick();
    set('f-key', 'typed by hand');
    await saveModal();
    expect(st.vault.api_keys[0].generated_by).toBeUndefined();
  });

  it('with automatic, asks Rust which recipe applies and uses it', async () => {
    stub();
    answer = (cmd) =>
      cmd === 'recipe_resolve'
        ? { recipe: 'plain', why: 'vault default' }
        : { samples: ['From-Default-9'] };
    open({}, RECIPES);
    $('f-recipe-make').click();
    await tick();
    expect(calls.map((c) => c.cmd)).toEqual(['recipe_resolve', 'recipe_preview']);
    expect($<HTMLInputElement>('f-key').value).toBe('From-Default-9');
    expect(calls[1].args.template).toBe('{random:16}');
  });

  it('says so when no recipe applies, when recipes are off, and when the name is unknown', async () => {
    stub();
    answer = () => null;
    open({}, RECIPES);
    $('f-recipe-make').click();
    await tick();
    expect($('f-recipe-note').textContent).toContain('No recipe applies');
    $<HTMLSelectElement>('f-recipe').value = 'none';
    $('f-recipe-make').click();
    await tick();
    expect($('f-recipe-note').textContent).toContain('switched off');
    answer = () => ({ recipe: 'ghost', why: 'rule' });
    $<HTMLSelectElement>('f-recipe').value = '';
    $('f-recipe-make').click();
    await tick();
    expect($('f-recipe-note').textContent).toContain('No recipe named ghost');
  });

  it('shows the error Rust gives, for example a floor that is not met', async () => {
    stub();
    answer = (cmd) => {
      if (cmd === 'recipe_preview') throw new Error('cannot fit: the random parts give 15 bits');
      return null;
    };
    open({}, RECIPES);
    $<HTMLSelectElement>('f-recipe').value = 'plain';
    $('f-recipe-make').click();
    await tick();
    expect($('f-recipe-note').textContent).toContain('cannot fit');
    expect($<HTMLInputElement>('f-key').value).not.toContain('cannot fit');
  });
});
