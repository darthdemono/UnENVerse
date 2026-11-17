// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
// The Tools, Password recipes pane (R41). The generator is in Rust and reached over IPC; these tests
// stand in for it with a recording stub and check what the pane stores, shows and sends.
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { st, switchTool } from '../src/ts/state';
import {
  initRecipesPane,
  load,
  PRESETS,
  readOptions,
  recipeView,
  renderRecipes,
  writeOptions,
} from '../src/ts/recipes-pane';
import { mountToolsPanes } from '../src/ts/tools-markup';
import { loadRealIndexHtml, makeVault, resetState } from './helpers';

const toasts: string[] = [];
let confirmAnswer = true;
vi.mock('../src/ts/utils', async (importOriginal) => {
  const real = await importOriginal<typeof import('../src/ts/utils')>();
  return {
    ...real,
    showToast: (m: string) => toasts.push(m),
    showConfirm: async () => confirmAnswer,
  };
});

const $ = <T extends HTMLElement = HTMLElement>(id: string) => document.getElementById(id) as T;
const set = (id: string, v: string) => {
  $<HTMLInputElement>(id).value = v;
};
const click = (id: string) => $(id).click();
const tick = () => new Promise((r) => setTimeout(r, 0));

const REGISTRY = {
  tokens: [
    {
      id: 'random',
      group: 'Random',
      kind: 'random',
      summary: 'N random characters from an alphabet',
      args: [
        { name: 'length', default: '16' },
        { name: 'alphabet', default: 'safe' },
      ],
    },
    { id: 'provider', group: 'Entry data', kind: 'data', summary: 'The provider', args: [] },
    { id: 'upper', group: 'Transforms', kind: 'transform', summary: 'UPPER CASE', args: [] },
    {
      id: 'expand',
      group: 'Lengtheners',
      kind: 'derive',
      summary: 'Derive length',
      args: [{ name: 'length', default: '22' }, { name: 'alphabet' }],
    },
  ],
  lists: [{ name: 'prefix', words: 32 }],
};

const PREVIEW = {
  samples: ['Abcdefghijklmnop', 'Qrstuvwxyz012345'],
  bits: 93,
  private_bits: 5,
  length: 16,
  parts: [
    { token: 'random', kind: 'random', bits: 93, private: false },
    { token: 'var', kind: 'typed', bits: 0, private: true },
  ],
  dropped: ['suffix'],
  warnings: ['a list pick adds under 6 bits'],
  rebuildable_from_memory: false,
};

let calls: { cmd: string; args: Record<string, unknown> }[] = [];
let answer: (cmd: string, args: Record<string, unknown>) => unknown;
let saves = 0;

function stubTauri() {
  (window as unknown as { __TAURI__: unknown }).__TAURI__ = {
    core: {
      invoke: async (cmd: string, args: Record<string, unknown>) => {
        calls.push({ cmd, args });
        return answer(cmd, args);
      },
    },
  };
}

beforeEach(async () => {
  resetState(st);
  loadRealIndexHtml();
  mountToolsPanes();
  toasts.length = 0;
  calls = [];
  saves = 0;
  confirmAnswer = true;
  answer = (cmd) =>
    cmd === 'recipe_registry' ? REGISTRY : cmd === 'recipe_preview' ? PREVIEW : null;
  st.vault = makeVault({
    api_keys: [{ id: 'e1', provider: 'Instagram', secretType: 'password' } as never],
  });
  st.vaultOpen = true;
  st.store = {
    save: async () => {
      saves += 1;
    },
    isRemote: false,
  } as never;
  stubTauri();
  initRecipesPane();
  await tick();
  load('');
});

afterEach(() => {
  delete (window as unknown as { __TAURI__?: unknown }).__TAURI__;
});

describe('the pane', () => {
  it('has a nav button and labels on every control', () => {
    expect(document.querySelector('.tool-nav-btn[data-tool="recipes"]')).not.toBeNull();
    const pane = $('tool-recipes');
    for (const c of pane.querySelectorAll<HTMLElement>('input, select, textarea')) {
      const labelled =
        c.getAttribute('aria-label') ||
        (c.id && pane.querySelector(`label[for="${c.id}"]`)) ||
        c.closest('label');
      expect(labelled, c.id).toBeTruthy();
    }
  });

  it('fills the token palette from the registry and offers the presets and rule kinds', () => {
    expect($('rcp-token').querySelectorAll('optgroup').length).toBe(4);
    expect($<HTMLSelectElement>('rcp-preset').options.length).toBe(PRESETS.length);
    expect($<HTMLSelectElement>('rcp-rule-kind').options.length).toBe(6);
    expect($('rcp-lists').textContent).toContain('prefix');
    expect($('rcp-lists').textContent).toContain('32 words');
    initRecipesPane();
    expect($<HTMLSelectElement>('rcp-rule-kind').options.length).toBe(6);
  });

  it('keeps working without the desktop: no registry, and the preview says why', async () => {
    delete (window as unknown as { __TAURI__?: unknown }).__TAURI__;
    initRecipesPane();
    await tick();
    expect($('rcp-token').querySelectorAll('optgroup').length).toBe(0);
    set('rcp-template', '{random:16}');
    click('rcp-preview-btn');
    await tick();
    expect($('rcp-preview').textContent).toContain('need the desktop app');
  });

  it('survives a registry that cannot be read', async () => {
    answer = () => {
      throw new Error('no ipc');
    };
    initRecipesPane();
    await tick();
    expect($('rcp-token').querySelectorAll('optgroup').length).toBe(0);
    expect($('rcp-lists').textContent).not.toContain('built in');
  });
});

describe('options', () => {
  it('round-trips the form and stores only what differs from the defaults', () => {
    expect(readOptions()).toEqual({});
    writeOptions({
      min_len: 12,
      max_len: 30,
      policy: 'refuse',
      require_upper: 2,
      avoid_ambiguous: true,
      forbid: 'xy',
      chunk: 4,
      chunk_sep: '_',
      floor_bits: 80,
    });
    const o = readOptions();
    expect(o).toMatchObject({
      min_len: 12,
      max_len: 30,
      policy: 'refuse',
      require_upper: 2,
      avoid_ambiguous: true,
      forbid: 'xy',
      chunk: 4,
      chunk_sep: '_',
      floor_bits: 80,
    });
    writeOptions();
    expect(readOptions()).toEqual({});
    set('rcp-min', 'abc');
    expect(readOptions()).toEqual({});
  });
});

describe('saving, loading and deleting a recipe', () => {
  it('refuses a recipe with no name', async () => {
    set('rcp-template', '{random:16}');
    click('rcp-save');
    await tick();
    expect($('rcp-status').textContent).toContain('name');
    expect(saves).toBe(0);
  });

  it('checks the template over IPC with the floor off, then stores it with its options', async () => {
    set('rcp-name', 'social');
    set('rcp-template', '{prefix}-{random:16}');
    set('rcp-max', '40');
    click('rcp-save');
    await tick();
    const check = calls.find((c) => c.cmd === 'recipe_preview')!;
    expect((check.args.options as { floor_bits: number }).floor_bits).toBe(0);
    expect(st.vault).toMatchObject({
      recipes: [{ name: 'social', template: '{prefix}-{random:16}', options: { max_len: 40 } }],
    });
    expect(saves).toBeGreaterThan(0);
    expect($('rcp-select').textContent).toContain('social');
    // Saving again under the same name replaces it.
    set('rcp-template', '{random:20}');
    click('rcp-save');
    await tick();
    expect((st.vault as never as { recipes: unknown[] }).recipes).toHaveLength(1);
  });

  it('does not store a template the generator rejects', async () => {
    answer = (cmd) => {
      if (cmd === 'recipe_preview') throw new Error('`{nope}` is not a token');
      return null;
    };
    set('rcp-name', 'bad');
    set('rcp-template', '{nope}');
    click('rcp-save');
    await tick();
    expect($('rcp-status').textContent).toContain('not a token');
    expect((st.vault as never as { recipes?: unknown[] }).recipes).toBeUndefined();
  });

  it('stores without a check when there is no desktop to ask', async () => {
    delete (window as unknown as { __TAURI__?: unknown }).__TAURI__;
    set('rcp-name', 'offline');
    set('rcp-template', '{random:16}');
    click('rcp-save');
    await tick();
    expect((st.vault as never as { recipes: unknown[] }).recipes).toHaveLength(1);
  });

  it('loads a saved recipe into the editor and clears it for a new one', async () => {
    set('rcp-name', 'social');
    set('rcp-template', '{random:16}');
    set('rcp-min', '12');
    click('rcp-save');
    await tick();
    load('');
    expect($<HTMLInputElement>('rcp-name').value).toBe('');
    $<HTMLSelectElement>('rcp-select').value = 'social';
    $('rcp-select').dispatchEvent(new Event('change'));
    expect($<HTMLTextAreaElement>('rcp-template').value).toBe('{random:16}');
    expect($<HTMLInputElement>('rcp-min').value).toBe('12');
  });

  it('deletes a recipe, its rules and the default, after a confirmation', async () => {
    set('rcp-name', 'social');
    set('rcp-template', '{random:16}');
    click('rcp-save');
    await tick();
    const doc = st.vault as never as Record<string, unknown>;
    doc.recipe_rules = [{ kind: 'provider', value: 'x', recipe: 'social' }];
    doc.recipe_default = 'social';
    confirmAnswer = false;
    click('rcp-delete');
    await tick();
    expect(doc.recipes).toHaveLength(1);
    confirmAnswer = true;
    click('rcp-delete');
    await tick();
    expect(doc.recipes).toEqual([]);
    expect(doc.recipe_rules).toEqual([]);
    expect(doc.recipe_default).toBeUndefined();
    click('rcp-delete');
    await tick();
    expect(saves).toBeGreaterThanOrEqual(2);
  });

  it('sets and clears the vault default from the picker', async () => {
    set('rcp-name', 'social');
    set('rcp-template', '{random:16}');
    click('rcp-save');
    await tick();
    $<HTMLSelectElement>('rcp-default').value = 'social';
    $('rcp-default').dispatchEvent(new Event('change'));
    await tick();
    expect((st.vault as never as { recipe_default: string }).recipe_default).toBe('social');
    expect($('rcp-select').textContent).toContain('(default)');
    $<HTMLSelectElement>('rcp-default').value = '';
    $('rcp-default').dispatchEvent(new Event('change'));
    await tick();
    expect((st.vault as never as { recipe_default?: string }).recipe_default).toBeUndefined();
  });
});

describe('the preview', () => {
  it('sends the template, the options and only the part of the vault a recipe may read', async () => {
    set('rcp-template', '{provider}-{random:16}');
    set('rcp-entry', 'instagram');
    click('rcp-preview-btn');
    await tick();
    const c = calls.filter((x) => x.cmd === 'recipe_preview').pop()!;
    expect(c.args.template).toBe('{provider}-{random:16}');
    expect((c.args.entry as { provider: string }).provider).toBe('Instagram');
    expect(Object.keys(c.args.vault as object).sort()).toEqual(
      [
        'projects',
        'recipe_default',
        'recipe_lists',
        'recipe_rules',
        'recipe_vars',
        'recipes',
      ].sort(),
    );
    expect(c.args.count).toBe(5);
  });

  it('masks the samples until the box is ticked, and shows strength and the part table', async () => {
    set('rcp-template', '{random:16}');
    click('rcp-preview-btn');
    await tick();
    const box = $('rcp-preview');
    expect(box.textContent).not.toContain('Abcdefghijklmnop');
    expect(box.textContent).toContain('93 bits');
    expect(box.textContent).toContain('Rebuildable from memory: no');
    expect(box.textContent).toContain('Dropped to fit: suffix');
    expect(box.textContent).toContain('under 6 bits');
    expect(box.querySelectorAll('tr').length).toBe(3);
    expect(box.textContent).toContain('private');
    $<HTMLInputElement>('rcp-reveal').checked = true;
    $('rcp-reveal').dispatchEvent(new Event('change'));
    await tick();
    expect($('rcp-preview').textContent).toContain('Abcdefghijklmnop');
  });

  it('shows the error Rust gives, in an alert', async () => {
    answer = (cmd) => {
      if (cmd === 'recipe_preview') throw new Error('cannot fit: the random parts give 15 bits');
      return null;
    };
    set('rcp-template', '{digits:3}');
    click('rcp-preview-btn');
    await tick();
    expect($('rcp-preview').querySelector('[role="alert"]')!.textContent).toContain('cannot fit');
  });

  it('shows no private-bits line, and no dropped or warning lines, when there are none', async () => {
    answer = (cmd) =>
      cmd === 'recipe_preview'
        ? { ...PREVIEW, private_bits: 0, dropped: [], warnings: [], rebuildable_from_memory: true }
        : REGISTRY;
    set('rcp-template', '{random:16}');
    click('rcp-preview-btn');
    await tick();
    const t = $('rcp-preview').textContent!;
    expect(t).not.toContain('hidden');
    expect(t).not.toContain('Dropped');
    expect(t).toContain('Rebuildable from memory: yes');
  });
});

describe('the token palette and presets', () => {
  it('inserts a token with its default arguments at the cursor, and a transform inside the token before it', () => {
    const box = $<HTMLTextAreaElement>('rcp-template');
    box.value = 'ab-cd';
    box.setSelectionRange(2, 2);
    $<HTMLSelectElement>('rcp-token').value = 'random';
    click('rcp-insert');
    expect(box.value).toBe('ab{random:16:safe}-cd');
    box.value = '{random:8}';
    box.setSelectionRange(10, 10);
    $<HTMLSelectElement>('rcp-token').value = 'upper';
    click('rcp-insert');
    expect(box.value).toBe('{random:8|upper}');
    $<HTMLSelectElement>('rcp-token').value = 'expand';
    click('rcp-insert');
    expect(box.value).toBe('{random:8|upper|expand:22}');
    $<HTMLSelectElement>('rcp-token').value = 'provider';
    click('rcp-insert');
    expect(box.value).toBe('{random:8|upper|expand:22}{provider}');
    $<HTMLSelectElement>('rcp-token').value = 'ghost';
    click('rcp-insert');
    expect(box.value).toBe('{random:8|upper|expand:22}{provider}');
  });

  it('refuses a transform that is not right after a token', () => {
    const box = $<HTMLTextAreaElement>('rcp-template');
    box.value = 'plain';
    box.setSelectionRange(0, 0);
    $<HTMLSelectElement>('rcp-token').value = 'upper';
    click('rcp-insert');
    expect(box.value).toBe('plain');
    expect(toasts[toasts.length - 1]).toContain('right after a token');
  });

  it('copies a preset into the editor', () => {
    $<HTMLSelectElement>('rcp-preset').selectedIndex = 1;
    click('rcp-use-preset');
    expect($<HTMLTextAreaElement>('rcp-template').value).toBe(PRESETS[1].template);
  });
});

describe('your lists, facts and rules', () => {
  it('validates and stores a list, shows its size in bits, and removes it', async () => {
    set('rcp-list-name', 'bad name');
    click('rcp-list-add');
    await tick();
    expect($('rcp-status').textContent).toContain('list name');
    set('rcp-list-name', 'meals');
    set('rcp-list-words', 'only');
    click('rcp-list-add');
    await tick();
    expect($('rcp-status').textContent).toContain('at least 2');
    set('rcp-list-words', 'kimchi, naan\ntacos\nkimchi');
    $<HTMLInputElement>('rcp-list-private').checked = true;
    click('rcp-list-add');
    await tick();
    const doc = st.vault as never as {
      recipe_lists: { name: string; words: string[]; private: boolean }[];
    };
    expect(doc.recipe_lists[0]).toEqual({
      name: 'meals',
      words: ['kimchi', 'naan', 'tacos'],
      private: true,
    });
    expect($('rcp-lists').textContent).toContain('3 words, private');
    expect($('rcp-lists').textContent).toContain('a small list');
    set('rcp-list-words', 'a,b');
    click('rcp-list-add');
    await tick();
    expect(doc.recipe_lists).toHaveLength(1);
    ($('rcp-lists').querySelector('[data-action="rcp-list-rm"]') as HTMLElement).click();
    await tick();
    expect(doc.recipe_lists).toEqual([]);
  });

  it('sorts a list and removes its duplicate words without changing the distinct ones', async () => {
    st.vault = makeVault({
      api_keys: [],
      recipe_lists: [{ name: 'dups', words: ['pear', 'apple', 'pear', 'fig', 'apple'] }],
    } as never);
    renderRecipes();
    const doc = st.vault as never as { recipe_lists: { words: string[] }[] };
    const press = async (action: string, name = 'dups') => {
      const b = document.createElement('button');
      b.dataset.action = action;
      b.dataset.name = name;
      $('rcp-lists').append(b);
      b.click();
      await tick();
    };
    await press('rcp-list-dedupe');
    expect(doc.recipe_lists[0].words).toEqual(['pear', 'apple', 'fig']);
    expect(toasts[toasts.length - 1]).toBe('2 duplicates removed');
    await press('rcp-list-sort');
    expect(doc.recipe_lists[0].words).toEqual(['apple', 'fig', 'pear']);
    expect(toasts[toasts.length - 1]).toBe('Sorted');
    // A name that is not a list changes nothing.
    const saved = saves;
    await press('rcp-list-sort', 'ghost');
    expect(saves).toBe(saved);
  });

  it('validates and stores a fact, hides private values, and removes it', async () => {
    set('rcp-fact-name', 'Pet');
    click('rcp-fact-add');
    await tick();
    expect($('rcp-status').textContent).toContain('fact name');
    set('rcp-fact-name', 'pet');
    click('rcp-fact-add');
    await tick();
    expect($('rcp-status').textContent).toContain('needs a value');
    set('rcp-fact-value', 'rex');
    click('rcp-fact-add');
    await tick();
    expect($('rcp-facts').textContent).toContain('private');
    expect($('rcp-facts').textContent).not.toContain('rex');
    set('rcp-fact-name', 'street');
    set('rcp-fact-value', 'elm');
    $<HTMLInputElement>('rcp-fact-public').checked = true;
    click('rcp-fact-add');
    await tick();
    expect($('rcp-facts').textContent).toContain('elm');
    set('rcp-fact-value', 'oak');
    click('rcp-fact-add');
    await tick();
    expect((st.vault as never as { recipe_vars: unknown[] }).recipe_vars).toHaveLength(2);
    ($('rcp-facts').querySelector('[data-action="rcp-fact-rm"]') as HTMLElement).click();
    await tick();
    expect((st.vault as never as { recipe_vars: unknown[] }).recipe_vars).toHaveLength(1);
  });

  it('adds, tests and removes an assignment rule', async () => {
    set('rcp-name', 'social');
    set('rcp-template', '{random:16}');
    click('rcp-save');
    await tick();
    click('rcp-rule-add');
    await tick();
    expect((st.vault as never as { recipe_rules?: unknown[] }).recipe_rules).toBeUndefined();
    expect($('rcp-rules').textContent).toContain('No rules');
    $<HTMLSelectElement>('rcp-rule-kind').value = 'provider';
    set('rcp-rule-value', 'Instagram');
    $<HTMLSelectElement>('rcp-rule-recipe').value = 'social';
    click('rcp-rule-add');
    await tick();
    expect($('rcp-rules').textContent).toContain('provider Instagram uses social');
    // The test asks Rust which recipe applies.
    answer = (cmd) =>
      cmd === 'recipe_resolve' ? { recipe: 'social', why: 'rule: provider Instagram' } : REGISTRY;
    set('rcp-rule-test-q', 'instagram');
    click('rcp-rule-test');
    await tick();
    expect($('rcp-rule-test-out').textContent).toBe('social (rule: provider Instagram)');
    answer = (cmd) => (cmd === 'recipe_resolve' ? null : REGISTRY);
    click('rcp-rule-test');
    await tick();
    expect($('rcp-rule-test-out').textContent).toContain('plain random');
    set('rcp-rule-test-q', 'nobody');
    click('rcp-rule-test');
    await tick();
    expect($('rcp-rule-test-out').textContent).toContain('No entry');
    delete (window as unknown as { __TAURI__?: unknown }).__TAURI__;
    set('rcp-rule-test-q', 'instagram');
    click('rcp-rule-test');
    await tick();
    expect($('rcp-rule-test-out').textContent).toContain('desktop app');
    ($('rcp-rules').querySelector('[data-action="rcp-rule-rm"]') as HTMLElement).click();
    await tick();
    expect((st.vault as never as { recipe_rules: unknown[] }).recipe_rules).toEqual([]);
    // A click on the table that is not a button changes nothing.
    $('rcp-rules').click();
    ($('rcp-rules') as HTMLElement).dispatchEvent(new MouseEvent('click', { bubbles: true }));
    const unknown = document.createElement('button');
    unknown.dataset.action = 'something-else';
    $('rcp-rules').append(unknown);
    unknown.click();
    await tick();
  });

  it('sends the same view to Rust whatever the pane holds', () => {
    renderRecipes();
    expect(recipeView().recipes).toEqual([]);
    expect(recipeView().projects).toEqual(expect.any(Array));
  });
});

describe('opening the tool and a page without the pane', () => {
  it('wires the pane when its tool is opened', async () => {
    document.getElementById('rcp-token')!.replaceChildren();
    switchTool('recipes');
    await tick();
    await tick();
    expect($('rcp-token').querySelectorAll('optgroup').length).toBe(4);
  });

  it('does nothing, and throws nothing, where the pane is not on the page', () => {
    document.body.innerHTML = '';
    expect(() => initRecipesPane()).not.toThrow();
    expect(() => renderRecipes()).not.toThrow();
    expect(() => writeOptions({ min_len: 3 })).not.toThrow();
    expect(readOptions()).toEqual({});
  });

  it('reads a vault that has no recipe data at all', () => {
    st.vault = makeVault({ api_keys: [] });
    expect(recipeView()).toMatchObject({
      recipes: [],
      recipe_lists: [],
      recipe_vars: [],
      recipe_rules: [],
    });
    renderRecipes();
    expect($('rcp-rules').textContent).toContain('No rules');
  });

  it('removes lists, facts and rules from a vault that holds none', async () => {
    st.vault = makeVault({ api_keys: [] });
    for (const [action, name] of [
      ['rcp-list-rm', 'x'],
      ['rcp-fact-rm', 'x'],
      ['rcp-rule-rm', ''],
    ]) {
      const b = document.createElement('button');
      b.dataset.action = action;
      b.dataset.name = name;
      b.dataset.n = '0';
      $('rcp-lists').append(b);
      b.click();
      await tick();
    }
    expect(toasts.length).toBeGreaterThan(0);
  });

  it('saves in a vault with no recipe list yet, and previews without an entry when none matches', async () => {
    st.vault = makeVault({ api_keys: [] });
    set('rcp-name', 'first');
    set('rcp-template', '{random:16}');
    click('rcp-save');
    await tick();
    expect((st.vault as never as { recipes: unknown[] }).recipes).toHaveLength(1);
    set('rcp-entry', 'nobody');
    click('rcp-preview-btn');
    await tick();
    expect(calls.filter((c) => c.cmd === 'recipe_preview').pop()!.args.entry).toBeNull();
  });
});
