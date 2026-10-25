// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
/**
 * Everything you can do to a bundle card and a recovery-code card from the
 * grid: reorder, rename a slot, add and remove members and variables, assign a
 * project, category or tag to every member, export, dissolve, delete (with Undo),
 * and the "bundle these?" suggestion. Booted for real; the buttons are the ones
 * the renderer draws.
 */
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { bootApp, entry } from './app-boot';

const toasts: [string, string?][] = [];
const copied: string[] = [];
let confirmAnswer = true;
let promptAnswers: (string | null)[] = [];
let passwordAnswer: string | null = null;
vi.mock('../src/ts/utils', async (importOriginal) => {
  const real = await importOriginal<typeof import('../src/ts/utils')>();
  return {
    ...real,
    showToast: (m: string, k?: string) => toasts.push([m, k]),
    showConfirm: async () => confirmAnswer,
    showPrompt: async () => (promptAnswers.length ? promptAnswers.shift()! : null),
    showPasswordPrompt: async () => passwordAnswer,
    clipboardWrite: async (t: string) => void copied.push(t),
  };
});

const flush = (ms = 40) => new Promise((r) => setTimeout(r, ms));
const grid = () => document.getElementById('card-grid')!;
const act = async (sel: string) => {
  const el = document.querySelector<HTMLElement>(sel);
  if (!el) throw new Error(`no element for ${sel}`);
  el.click();
  await flush();
};
const bundleBtn = (action: string, extra = '') =>
  `[data-action="${action}"][data-bundle="bun"]${extra}`;

let st: Awaited<ReturnType<typeof bootApp>>['st'];
const bundle = () => st.vault.api_keys.find((e) => e.id === 'bun')!;
const member = (id: string) => st.vault.api_keys.find((e) => e.id === id)!;

async function boot(extra: object[] = []) {
  ({ st } = await bootApp(
    [
      entry('bun', 'Discord', {
        secretType: 'bundle',
        api_key: '',
        bundle_primary: 'm1',
        extra_vars: [{ key: 'prefix', value: '!' }],
      }),
      entry('m1', 'Bot', { bundle_id: 'bun', bundle_slot: 'bot', bundle_order: 10 }),
      entry('m2', 'Webhook', { bundle_id: 'bun', bundle_slot: 'hook', bundle_order: 20 }),
      entry('loose', 'Loose key'),
      entry('loose2', 'Loose two', { label: 'twin' }),
      entry('loose3', 'Loose three', { label: 'twin' }),
      ...extra,
    ],
    {
      settings: { groupBundles: true },
      categories: ['infra', 'extra'],
      projects: [
        { id: 'Universal', name: 'Universal' },
        { id: 'web', name: 'Web' },
      ],
    },
  ));
  // Open the bundle so its member controls are drawn.
  if (document.querySelector(bundleBtn('bundle-toggle'))?.getAttribute('aria-expanded') !== 'true')
    await act(bundleBtn('bundle-toggle'));
}

beforeEach(async () => {
  toasts.length = 0;
  copied.length = 0;
  confirmAnswer = true;
  promptAnswers = [];
  passwordAnswer = null;
  await boot();
}, 30000);

describe('bundle members', { timeout: 30000 }, () => {
  it('with more than three members the slot tabs pick which one is shown, and ignore a member from another bundle', async () => {
    await boot([
      entry('m3', 'Third', { bundle_id: 'bun', bundle_slot: 'third', bundle_order: 30 }),
      entry('m4', 'Fourth', { bundle_id: 'bun', bundle_slot: 'fourth', bundle_order: 40 }),
    ]);
    await act(bundleBtn('bundle-slot-tab', '[data-member="m2"]'));
    expect(st.bundleSlotTabs['bun']).toBe('m2');
    const el = document.querySelector<HTMLElement>(
      bundleBtn('bundle-slot-tab', '[data-member="m2"]'),
    )!;
    el.dataset.member = 'loose';
    el.click();
    await flush();
    expect(st.bundleSlotTabs['bun']).toBe('m2');
  });

  it('making a member the primary records it on the bundle', async () => {
    await act(bundleBtn('bundle-primary', '[data-member="m2"]'));
    expect(bundle().bundle_primary).toBe('m2');
  });

  it('a primary request for a member that is not in the bundle changes nothing', async () => {
    const el = document.querySelector<HTMLElement>(
      bundleBtn('bundle-primary', '[data-member="m2"]'),
    )!;
    el.dataset.member = 'loose';
    el.click();
    await flush();
    expect(bundle().bundle_primary).toBe('m1');
  });

  it('moving a member down swaps its order with the next, and the last one cannot move down', async () => {
    await act(bundleBtn('bundle-order', '[data-member="m1"][data-direction="1"]'));
    expect(member('m1').bundle_order).toBe(20);
    expect(member('m2').bundle_order).toBe(10);
    await act(bundleBtn('bundle-order', '[data-member="m1"][data-direction="1"]'));
    expect(member('m1').bundle_order).toBe(20);
    expect(member('m2').bundle_order).toBe(10);
  });

  it('a move that would leave the list does nothing', async () => {
    const el = document.querySelector<HTMLElement>(
      bundleBtn('bundle-order', '[data-member="m1"]'),
    )!;
    el.dataset.direction = '-1';
    el.click();
    await flush();
    expect(member('m1').bundle_order).toBe(10);
  });

  it('removing a member after confirming takes it out and hands the primary to another member', async () => {
    await act(bundleBtn('bundle-remove-member', '[data-member="m1"]'));
    expect(member('m1').bundle_id).toBeUndefined();
    expect(member('m1').bundle_slot).toBeUndefined();
    expect(bundle().bundle_primary).toBe('m2');
  });

  it('removing a member that is not the primary leaves the primary alone', async () => {
    await act(bundleBtn('bundle-remove-member', '[data-member="m2"]'));
    expect(member('m2').bundle_id).toBeUndefined();
    expect(bundle().bundle_primary).toBe('m1');
  });

  it('declining the confirmation keeps a member that templates still refer to', async () => {
    st.vault.projects[0].chunks = [
      {
        id: 'c',
        name: 'app',
        chunk_type: 'env_file',
        fields: [{ key: 'TOKEN', value: '${bundle:Discord/bot}', field_type: 'var' }],
      },
    ] as never;
    confirmAnswer = false;
    await act(bundleBtn('bundle-remove-member', '[data-member="m1"]'));
    expect(member('m1').bundle_id).toBe('bun');
  });

  it('adding a member by name takes an unbundled entry in, with a slot named after it', async () => {
    promptAnswers = ['Loose key'];
    await act(bundleBtn('bundle-add-member'));
    expect(member('loose').bundle_id).toBe('bun');
    expect(member('loose').bundle_slot).toBe('Loose_key');
    expect(member('loose').bundle_order).toBe(30);
  });

  it('adding a name that matches nothing, or matches two, says so and adds nothing', async () => {
    promptAnswers = ['nothing like it'];
    await act(bundleBtn('bundle-add-member'));
    expect(toasts[toasts.length - 1]).toEqual(['No matching unbundled entry', 'err']);
    promptAnswers = ['twin'];
    await act(bundleBtn('bundle-add-member'));
    expect(toasts[toasts.length - 1]).toEqual([
      'That name is ambiguous; choose a unique label',
      'err',
    ]);
    expect(member('loose2').bundle_id).toBeUndefined();
  });

  it('cancelling the name prompt adds nothing', async () => {
    promptAnswers = [null];
    await act(bundleBtn('bundle-add-member'));
    expect(st.vault.api_keys.filter((e) => e.bundle_id === 'bun')).toHaveLength(2);
  });

  it('adding a member whose pool is split across another bundle is refused', async () => {
    await boot([
      entry('o', 'Other', { secretType: 'bundle', api_key: '' }),
      entry('p1', 'PoolA', { pool: 'ring' }),
      entry('p2', 'PoolB', { pool: 'ring', bundle_id: 'o', bundle_slot: 'x', bundle_order: 10 }),
    ]);
    promptAnswers = ['PoolA'];
    await act(bundleBtn('bundle-add-member'));
    expect(toasts[toasts.length - 1]).toEqual([
      'All entries in a key pool must belong to the same bundle',
      'err',
    ]);
    expect(member('p1').bundle_id).toBeUndefined();
  });

  it('adding a pooled member offers the whole pool, and takes it only when agreed', async () => {
    await boot([entry('p1', 'PoolA', { pool: 'ring' }), entry('p2', 'PoolB', { pool: 'ring' })]);
    promptAnswers = ['PoolA'];
    confirmAnswer = false;
    await act(bundleBtn('bundle-add-member'));
    expect(member('p1').bundle_id).toBeUndefined();
    promptAnswers = ['PoolA'];
    confirmAnswer = true;
    await act(bundleBtn('bundle-add-member'));
    expect(member('p1').bundle_id).toBe('bun');
    expect(member('p2').bundle_id).toBe('bun');
    expect(member('p1').bundle_slot).not.toBe(member('p2').bundle_slot);
  });

  it('two members with the same name get distinct slots, and a name with no usable characters gets a numbered one', async () => {
    await boot([entry('d1', 'Dup', {}), entry('d2', '***', {})]);
    promptAnswers = ['Dup'];
    await act(bundleBtn('bundle-add-member'));
    member('d1').bundle_id = undefined;
    delete member('d1').bundle_id;
    member('d1').provider = 'bot';
    promptAnswers = ['bot'];
    await act(bundleBtn('bundle-add-member'));
    expect(member('d1').bundle_slot).toBe('bot_2');
    promptAnswers = ['***'];
    await act(bundleBtn('bundle-add-member'));
    expect(member('d2').bundle_slot).toMatch(/^_|member_/);
  });
});

describe('renaming a slot', { timeout: 30000 }, () => {
  const rename = async (to: string | null) => {
    promptAnswers = [to];
    await act(bundleBtn('bundle-rename-slot', '[data-member="m1"]'));
  };

  it('renames the slot', async () => {
    await rename('robot');
    expect(member('m1').bundle_slot).toBe('robot');
  });

  it('keeps the old name for an empty answer, a cancelled prompt and an unchanged name', async () => {
    await rename('  ');
    await rename(null);
    await rename('bot');
    expect(member('m1').bundle_slot).toBe('bot');
  });

  it('refuses a name with illegal characters, and says what is allowed', async () => {
    await rename('no spaces');
    expect(member('m1').bundle_slot).toBe('bot');
    expect(toasts[toasts.length - 1][0]).toContain('Slot names must start');
  });

  it('refuses a name another member or a variable already uses', async () => {
    await rename('hook');
    expect(toasts[toasts.length - 1][0]).toBe('That slot name is already used in this bundle');
    await rename('prefix');
    expect(member('m1').bundle_slot).toBe('bot');
  });

  it('rewrites references to the old slot and says how many', async () => {
    st.vault.projects[0].chunks = [
      {
        id: 'c',
        name: 'app',
        chunk_type: 'env_file',
        fields: [{ key: 'TOKEN', value: '${bundle:Discord/bot}', field_type: 'var' }],
      },
    ] as never;
    await rename('robot');
    expect(JSON.stringify(st.vault.projects)).toContain('Discord/robot');
    expect(toasts.some(([m]) => /^Updated \d+ bundle reference/.test(m))).toBe(true);
  });
});

describe('bundle variables', { timeout: 30000 }, () => {
  it('adds a masked variable after asking for its name and value', async () => {
    promptAnswers = ['token'];
    passwordAnswer = 's3cret-value';
    await act(bundleBtn('bundle-add-var'));
    expect(bundle().extra_vars).toContainEqual({
      key: 'token',
      value: 's3cret-value',
      secret: true,
    });
  });

  it('refuses a name that already exists, and a cancelled value adds nothing', async () => {
    promptAnswers = ['prefix'];
    await act(bundleBtn('bundle-add-var'));
    expect(toasts[toasts.length - 1]).toEqual(['That variable already exists', 'err']);
    promptAnswers = ['fresh'];
    passwordAnswer = null;
    await act(bundleBtn('bundle-add-var'));
    expect(bundle().extra_vars!.some((v) => v.key === 'fresh')).toBe(false);
    promptAnswers = [''];
    await act(bundleBtn('bundle-add-var'));
    expect(bundle().extra_vars).toHaveLength(1);
  });

  it('removes a variable only when confirmed', async () => {
    confirmAnswer = false;
    await act(bundleBtn('bundle-var-remove', '[data-key="prefix"]'));
    expect(bundle().extra_vars).toHaveLength(1);
    confirmAnswer = true;
    await act(bundleBtn('bundle-var-remove', '[data-key="prefix"]'));
    expect(bundle().extra_vars).toEqual([]);
  });
});

describe('assigning to every member', { timeout: 30000 }, () => {
  it('a project, a category or a tag is added to each member once', async () => {
    promptAnswers = ['Web'];
    await act(bundleBtn('bundle-add-project'));
    expect(member('m1').projectIds).toContain('web');
    expect(member('m2').projectIds).toContain('web');
    expect(toasts[toasts.length - 1]).toEqual(['Project assigned to 2 members', 'ok']);
    promptAnswers = ['extra'];
    await act(bundleBtn('bundle-add-category'));
    expect(member('m1').categories).toContain('extra');
    promptAnswers = ['fresh-tag'];
    await act(bundleBtn('bundle-add-tag'));
    expect(member('m2').tags).toContain('fresh-tag');
    // Again: no duplicates.
    promptAnswers = ['fresh-tag'];
    await act(bundleBtn('bundle-add-tag'));
    expect(member('m2').tags!.filter((t) => t === 'fresh-tag')).toHaveLength(1);
    promptAnswers = ['Web'];
    await act(bundleBtn('bundle-add-project'));
    expect(member('m1').projectIds.filter((p) => p === 'web')).toHaveLength(1);
    promptAnswers = ['extra'];
    await act(bundleBtn('bundle-add-category'));
    expect(member('m1').categories.filter((c) => c === 'extra')).toHaveLength(1);
  });

  it('a project or category that does not exist is refused, and an empty answer does nothing', async () => {
    promptAnswers = ['Nowhere'];
    await act(bundleBtn('bundle-add-project'));
    expect(toasts[toasts.length - 1]).toEqual(['Choose an existing project name or ID', 'err']);
    promptAnswers = ['nope'];
    await act(bundleBtn('bundle-add-category'));
    expect(toasts[toasts.length - 1]).toEqual(['Choose an existing category', 'err']);
    toasts.length = 0;
    promptAnswers = [''];
    await act(bundleBtn('bundle-add-tag'));
    expect(toasts).toEqual([]);
  });
});

describe('whole-bundle actions', { timeout: 30000 }, () => {
  it('Export offers eight formats and copies the bundle in the chosen one', async () => {
    await act(bundleBtn('bundle-export'));
    const items = [...document.querySelectorAll<HTMLElement>('#dropdown .dropdown-item')];
    const labels = items.map((i) => i.textContent?.trim()).filter(Boolean);
    expect(labels).toEqual(
      expect.arrayContaining(['Copy all as .env', 'Copy all as Python', 'Copy all as TOML']),
    );
    items.find((i) => i.textContent?.trim() === 'Copy all as .env')!.click();
    await flush();
    expect(copied[0]).toContain('k-m1-longenough');
    expect(toasts[toasts.length - 1]).toEqual(['Bundle copied as .env', 'ok']);
  });

  it('Dissolve keeps every entry and turns the bundle back into an ordinary entry', async () => {
    await act(bundleBtn('bundle-dissolve'));
    expect(bundle().secretType).toBe('env_var');
    expect(member('m1').bundle_id).toBeUndefined();
    expect(member('m2').bundle_slot).toBeUndefined();
    expect(toasts[toasts.length - 1]).toEqual(['Bundle dissolved; entries preserved', 'ok']);
  });

  it('Delete all asks, removes the bundle and its members, and Undo puts them back in place', async () => {
    confirmAnswer = false;
    await act(bundleBtn('bundle-delete-all'));
    expect(st.vault.api_keys.some((e) => e.id === 'bun')).toBe(true);
    confirmAnswer = true;
    await act(bundleBtn('bundle-delete-all'));
    expect(st.vault.api_keys.map((e) => e.id)).toEqual(['loose', 'loose2', 'loose3']);
    document.getElementById('undo-btn')!.click();
    await flush(80);
    expect(st.vault.api_keys.map((e) => e.id)).toEqual([
      'bun',
      'm1',
      'm2',
      'loose',
      'loose2',
      'loose3',
    ]);
  });

  it('Select visible members does nothing but explain, until bulk select is on', async () => {
    await act(bundleBtn('bundle-bulk-toggle'));
    expect(toasts[toasts.length - 1][0]).toBe('Choose Select first to bulk-select bundle members');
  });

  it('with bulk select on, it ticks each member and a card click toggles that card', async () => {
    const toggled: number[] = [];
    (window as never as Record<string, unknown>).__unvIsBulkMode = () => true;
    (window as never as Record<string, unknown>).__unvBulkToggle = (i: number) =>
      void toggled.push(i);
    await act(bundleBtn('bundle-bulk-toggle'));
    expect(toggled.length).toBe(2);
    toggled.length = 0;
    grid().querySelector<HTMLElement>('.card[data-idx] [data-action="toggle"]')!.click();
    expect(toggled).toHaveLength(1);
  });
});

describe('the "bundle these?" suggestion', { timeout: 30000 }, () => {
  const suggest = async () => {
    await bootApp(
      [
        entry('g1', 'Stripe', { label: 'live', api_key: 'sk-live-aaaaaaaaaa' }),
        entry('g2', 'Stripe', { label: 'test', api_key: 'sk-test-bbbbbbbbbb' }),
      ],
      { settings: { groupBundles: true } },
    ).then((s) => (st = s.st));
  };

  it('Accept bundles the entries and Dismiss hides the banner for that provider for good', async () => {
    await suggest();
    document.querySelector<HTMLElement>('[data-action="bundle-suggest-accept"]')!.click();
    await flush(80);
    const made = st.vault.api_keys.find((e) => e.secretType === 'bundle')!;
    expect(made.provider).toBe('Stripe');
    expect(toasts[toasts.length - 1]).toEqual(['Bundled 2 entries', 'ok']);
  });

  it('Dismiss is remembered by provider, lower-cased, once', async () => {
    await suggest();
    document.querySelector<HTMLElement>('[data-action="bundle-suggest-dismiss"]')!.click();
    await flush();
    const { Settings } = await import('../src/ts/state');
    expect(Settings.get('dismissedBundleSuggestions')).toContain('stripe');
  });
});

describe('recovery codes', { timeout: 30000 }, () => {
  it('marks the next code used only after confirming, and reports how many are left', async () => {
    await boot([
      entry('rc', 'Backup codes', {
        secretType: 'recovery_codes',
        api_key: '',
        extra_vars: [{ key: 'codes', value: 'aaaa-1111\nbbbb-2222\ncccc-3333' }],
      }),
    ]);
    const btn = () => document.querySelector<HTMLElement>('[data-action="codes-use"]')!;
    confirmAnswer = false;
    btn().click();
    await flush();
    expect(toasts.some(([m]) => m.includes('recovery codes left'))).toBe(false);
    confirmAnswer = true;
    btn().click();
    await flush(80);
    expect(toasts.some(([m]) => m === '2 recovery codes left')).toBe(true);
    expect(member('rc').extra_vars![0].value).toMatch(/^aaaa-1111\tUSED \d{4}-\d\d-\d\d\n/);
  });
});

describe('bundles written by older builds and unusual shapes', { timeout: 30000 }, () => {
  const lone = () => [
    entry('bun', 'Solo bundle', { secretType: 'bundle', api_key: '', bundle_primary: 'only' }),
    entry('only', 'OnlyOne', { bundle_id: 'bun' }),
  ];

  it('removing the only member leaves the bundle with no primary', async () => {
    await bootApp(lone(), { settings: { groupBundles: true } }).then((s) => (st = s.st));
    await act('[data-action="bundle-toggle"][data-bundle="bun"]');
    await act('[data-action="bundle-remove-member"][data-member="only"]');
    expect(bundle().bundle_primary).toBeNull();
    expect(member('only').bundle_id).toBeUndefined();
  });

  it('a member with no slot name is named after its provider when asking for a new one, and moves keep working without an order', async () => {
    await bootApp(
      [
        entry('bun', 'B', { secretType: 'bundle', api_key: '', bundle_primary: 'x1' }),
        entry('x1', 'First', { bundle_id: 'bun' }),
        entry('x2', 'Second', { bundle_id: 'bun' }),
      ],
      { settings: { groupBundles: true } },
    ).then((s) => (st = s.st));
    await act('[data-action="bundle-toggle"][data-bundle="bun"]');
    const asked: string[] = [];
    const utils = await import('../src/ts/utils');
    vi.spyOn(utils, 'showPrompt').mockImplementation(async (m: string) => {
      asked.push(m);
      return null;
    });
    await act('[data-action="bundle-rename-slot"][data-member="x1"]');
    expect(asked).toEqual(['New slot name for First']);
    await act('[data-action="bundle-order"][data-member="x1"][data-direction="1"]');
    expect(member('x1').bundle_order).toBe(20);
    expect(member('x2').bundle_order).toBe(10);
  });

  it('assigning to a bundle with one member says "member", and a rename touching one reference says "reference"', async () => {
    await bootApp(lone(), { settings: { groupBundles: true } }).then((s) => (st = s.st));
    await act('[data-action="bundle-toggle"][data-bundle="bun"]');
    promptAnswers = ['solo-tag'];
    await act('[data-action="bundle-add-tag"][data-bundle="bun"]');
    expect(toasts[toasts.length - 1]).toEqual(['Tag assigned to 1 member', 'ok']);
    st.vault.projects[0].chunks = [
      {
        id: 'c',
        name: 'app',
        chunk_type: 'env_file',
        fields: [{ key: 'K', value: '${bundle:Solo bundle/OnlyOne}', field_type: 'var' }],
      },
    ] as never;
    promptAnswers = ['renamed'];
    await act('[data-action="bundle-rename-slot"][data-member="only"]');
    expect(toasts[toasts.length - 1]).toEqual(['Updated 1 bundle reference', 'ok']);
  });

  it('adding to an empty bundle numbers the first member 10', async () => {
    await bootApp(
      [entry('bun', 'Empty', { secretType: 'bundle', api_key: '' }), entry('l', 'Loose')],
      { settings: { groupBundles: true } },
    ).then((s) => (st = s.st));
    await act('[data-action="bundle-toggle"][data-bundle="bun"]');
    promptAnswers = ['Loose'];
    await act('[data-action="bundle-add-member"][data-bundle="bun"]');
    expect(member('l').bundle_order).toBe(10);
    expect(bundle().bundle_primary).toBe('l');
  });

  it('dismissing a suggestion twice records the provider once', async () => {
    await bootApp([entry('g1', 'Stripe', { label: 'a' }), entry('g2', 'Stripe', { label: 'b' })], {
      settings: { groupBundles: true, dismissedBundleSuggestions: ['other'] },
    }).then((s) => (st = s.st));
    const { Settings } = await import('../src/ts/state');
    const btn = document.querySelector<HTMLElement>('[data-action="bundle-suggest-dismiss"]')!;
    btn.click();
    await flush();
    expect(Settings.get('dismissedBundleSuggestions')).toEqual(['other', 'stripe']);
    const again = document.createElement('button');
    again.dataset.action = 'bundle-suggest-dismiss';
    again.dataset.provider = 'Stripe';
    grid().appendChild(again);
    again.click();
    expect(Settings.get('dismissedBundleSuggestions')).toEqual(['other', 'stripe']);
  });

  it('a rename that touches several references says "references"; members without an order still export in a stable order', async () => {
    await bootApp(
      [
        entry('bun', 'Multi', { secretType: 'bundle', api_key: '', bundle_primary: 'a1' }),
        entry('a1', 'One', { bundle_id: 'bun', bundle_slot: 'one' }),
        entry('a2', 'Two', { bundle_id: 'bun', bundle_slot: 'two' }),
      ],
      { settings: { groupBundles: true } },
    ).then((s) => (st = s.st));
    st.vault.projects[0].chunks = [
      {
        id: 'c',
        name: 'app',
        chunk_type: 'env_file',
        fields: [
          { key: 'A', value: '${bundle:Multi/one}', field_type: 'var' },
          { key: 'B', value: '${bundle:Multi/one}', field_type: 'var' },
        ],
      },
    ] as never;
    await act('[data-action="bundle-toggle"][data-bundle="bun"]');
    promptAnswers = ['uno'];
    await act('[data-action="bundle-rename-slot"][data-member="a1"]');
    expect(toasts[toasts.length - 1]).toEqual(['Updated 2 bundle references', 'ok']);
    await act('[data-action="bundle-export"][data-bundle="bun"]');
    [...document.querySelectorAll<HTMLElement>('#dropdown .dropdown-item')][0].click();
    await flush();
    expect(copied[0]).toContain('k-a1-longenough');
    // Adding to members that carry no order starts counting from zero.
    st.vault.api_keys.push(entry('late', 'Late') as never);
    promptAnswers = ['Late'];
    await act('[data-action="bundle-add-member"][data-bundle="bun"]');
    expect(member('late').bundle_order).toBe(10);
  });
});

describe('adding a new key or env to a bundle', { timeout: 30000 }, () => {
  const fillAndSave = async (provider: string, key: string) => {
    const set = (id: string, v: string) => {
      const el = document.getElementById(id) as HTMLInputElement;
      el.value = v;
      el.dispatchEvent(new Event('input', { bubbles: true }));
    };
    set('f-provider', provider);
    set('f-key', key);
    document.getElementById('modal-save')!.click();
    await flush(80);
  };

  it('Add key opens the form on an API key and saves it as a slot of the bundle', async () => {
    await act(bundleBtn('bundle-add-key'));
    expect((document.getElementById('f-secret-type') as HTMLSelectElement).value).toBe('api_key');
    await fillAndSave('Fresh', 'fresh-key-value-1234');
    const added = st.vault.api_keys.find((e) => e.provider === 'Fresh')!;
    expect(added).toMatchObject({ bundle_id: 'bun', bundle_slot: 'Fresh', bundle_order: 30 });
  });

  it('Add env opens the form on an environment variable', async () => {
    await act(bundleBtn('bundle-add-env'));
    expect((document.getElementById('f-secret-type') as HTMLSelectElement).value).toBe('env_var');
    document.getElementById('modal-cancel')!.click();
  });

  it('does nothing for a bundle that no longer exists', async () => {
    const btn = document.querySelector<HTMLElement>(bundleBtn('bundle-add-key'))!;
    btn.dataset.bundle = 'gone';
    btn.click();
    await flush();
    expect(document.getElementById('modal-overlay')!.classList.contains('open')).toBe(false);
  });
});

describe('copying a bundle', { timeout: 30000 }, () => {
  it('the header button copies the whole combined environment, not the primary value', async () => {
    // Bug: it copied the primary member's value alone.
    await act(bundleBtn('bundle-copy-env'));
    expect(copied).toHaveLength(1);
    const text = copied[0];
    expect(text).toContain('=');
    expect(text.split('\n').filter((l) => /^[A-Z_0-9]+=/.test(l)).length).toBeGreaterThan(1);
    expect(toasts[toasts.length - 1][0]).toBe('Bundle environment copied');
  });
});

describe('copying a bundle environment, failures', { timeout: 30000 }, () => {
  it('does nothing for a bundle that is gone', async () => {
    const btn = document.querySelector<HTMLElement>(bundleBtn('bundle-copy-env'))!;
    btn.dataset.bundle = 'gone';
    copied.length = 0;
    btn.click();
    await flush();
    expect(copied).toHaveLength(0);
  });

  it('says so when a template variable cannot be resolved', async () => {
    bundle().extra_vars = [{ key: 'broken', kind: 'template', value: 'x-{nope}' }] as never;
    await act(bundleBtn('bundle-copy-env'));
    expect(toasts[toasts.length - 1][0]).toMatch(/^Export failed/);
    expect(toasts[toasts.length - 1][1]).toBe('err');
  });
});
