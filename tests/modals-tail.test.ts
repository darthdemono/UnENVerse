// SPDX-License-Identifier: PolyForm-Strict-1.0.0
// Copyright Jubair Hasan.
/**
 * Corners of the entry form and card menus: tooltips from the schema, pasting a
 * seed, editing blobs and cookie jars, refusing an entry that would hold
 * nothing, the generator popover when its entropy source is empty, and choosing
 * or clearing a card's icon.
 */
import { describe, it, expect, beforeEach, afterAll, vi } from 'vitest';
import { bootApp, entry, teardownApp } from './app-boot';

const toasts: [string, string?][] = [];
vi.mock('../src/ts/utils', async (importOriginal) => {
  const real = await importOriginal<typeof import('../src/ts/utils')>();
  return {
    ...real,
    showToast: (m: string, k?: string) => toasts.push([m, k]),
    showConfirm: async () => true,
    clipboardWrite: async () => {},
  };
});

afterAll(() => teardownApp());

const $ = <T extends HTMLElement = HTMLElement>(id: string) => document.getElementById(id) as T;
const flush = (ms = 40) => new Promise((r) => setTimeout(r, ms));
type St = Awaited<ReturnType<typeof bootApp>>['st'];
let st: St;
const set = (id: string, value: string) => {
  const el = $<HTMLInputElement | HTMLSelectElement>(id);
  el.value = value;
  el.dispatchEvent(new Event('input', { bubbles: true }));
  el.dispatchEvent(new Event('change', { bubbles: true }));
};

beforeEach(async () => {
  toasts.length = 0;
  ({ st } = await bootApp([
    entry('a', 'Alpha'),
    entry('blob', 'Blob', { secretType: 'file_blob', api_key: '', blob_ref: '/etc/x.bin' }),
    entry('jar', 'Jar', {
      secretType: 'cookie',
      api_key: 'sid=1',
      extra_vars: [
        { key: 'sid', value: '1', secret: true, attrs: { domain: 'site.test', path: '/' } },
      ],
    }),
    entry('mount', 'Mounted', { mount_path: '/run/secret', api_key: '' }),
  ]));
  toasts.length = 0;
}, 30000);

const edit = async (idx: number) => {
  document
    .querySelector<HTMLElement>(`#card-grid [data-action="edit"][data-idx="${idx}"]`)!
    .click();
  await flush();
};

describe('editing different kinds of entry', { timeout: 30000 }, () => {
  it('a file blob shows its reference, and a cookie jar keeps its attributes on the variable rows', async () => {
    const blobIdx = st.vault.api_keys.findIndex((e) => e.id === 'blob');
    await edit(blobIdx);
    expect($<HTMLInputElement>('f-blob').value).toBe('/etc/x.bin');
    $('modal-cancel').click();
    const jarIdx = st.vault.api_keys.findIndex((e) => e.id === 'jar');
    await edit(jarIdx);
    const row = document.querySelector<HTMLElement>('#f-extra-vars-list .extra-var-row')!;
    expect(JSON.parse(row.dataset.cookieAttrs!)).toEqual({ domain: 'site.test', path: '/' });
  });

  it('a file blob with no reference stored shows an empty box', async () => {
    const idx = st.vault.api_keys.findIndex((e) => e.id === 'blob');
    delete st.vault.api_keys[idx].blob_ref;
    await edit(idx);
    expect($<HTMLInputElement>('f-blob').value).toBe('');
  });
});

describe('the form’s helpers', { timeout: 30000 }, () => {
  it('pasting into the seed box refreshes its status after the paste lands', async () => {
    $('add-btn').click();
    await flush();
    const input = $<HTMLInputElement>('f-totp');
    input.value = 'JBSWY3DPEHPK3PXP';
    input.dispatchEvent(new Event('paste'));
    await flush(30);
    expect($('f-totp-status').textContent).toMatch(/\S/);
  });

  it('a type that holds nothing without a variable is refused with an explanation', async () => {
    $('add-btn').click();
    await flush();
    set('f-secret-type', 'database');
    set('f-provider', 'EmptyDb');
    // The form suggests variable names for the type; with all of them removed nothing is left.
    document
      .querySelectorAll<HTMLElement>('#f-extra-vars-list .extra-var-remove')
      .forEach((b) => b.click());
    $('modal-save').click();
    await flush();
    expect(toasts).toContainEqual([
      'This entry would hold nothing — add a value or a variable',
      'err',
    ]);
    expect(st.vault.api_keys.some((e) => e.provider === 'EmptyDb')).toBe(false);
  });

  it('the popover’s Use button does nothing when there is nothing generated', async () => {
    $('add-btn').click();
    await flush();
    $('f-key-generate-caret').click();
    const use = $<HTMLButtonElement>('gp-use-btn');
    use.dispatchEvent(new MouseEvent('click'));
    expect($<HTMLInputElement>('f-key').value).toBe('');
    expect(toasts).toEqual([]);
  });

  it('each popover generator says so when its entropy source is still filling', async () => {
    const { setEntropySource } = await import('../src/ts/generators');
    await setEntropySource('hardware').catch(() => {});
    $('add-btn').click();
    await flush();
    $('f-key-generate-caret').click();
    $('gp-generate-btn').click();
    await flush();
    for (const tab of ['password', 'apikey']) {
      document.querySelector<HTMLElement>(`.gen-tab-btn[data-gen-tab="${tab}"]`)!.click();
      $('gp-generate-btn').click();
      await flush();
    }
    expect(toasts.filter(([m]) => m.includes('still filling'))).toHaveLength(3);
    await setEntropySource('os');
  });

  it('field tooltips come from the schema the page ships with, for the fields it describes', async () => {
    const { applySchemaTooltips } = await import('../src/ts/modals');
    document.body.insertAdjacentHTML(
      'beforeend',
      '<label id="t1" class="form-label" data-field="provider"></label><label id="t2" class="form-label" data-field="nothing"></label>',
    );
    st.schema = undefined as never;
    applySchemaTooltips();
    expect($('t1').title).toBe('');
    st.schema = {
      properties: {
        api_keys: { items: { properties: { provider: { description: 'Who issued it' } } } },
      },
    } as never;
    applySchemaTooltips();
    expect($('t1').title).toBe('Who issued it');
    expect($('t2').title).toBe('');
    st.schema = {} as never;
    expect(() => applySchemaTooltips()).not.toThrow();
  });
});

describe('the copy menu for a file-shaped entry with nothing to write', () => {
  it('Write to file does nothing when the entry holds a path but no contents', async () => {
    const idx = st.vault.api_keys.findIndex((e) => e.id === 'mount');
    document
      .querySelector<HTMLElement>(`#card-grid [data-action="copy-env-menu"][data-idx="${idx}"]`)!
      .click();
    const item = [...document.querySelectorAll<HTMLElement>('#dropdown .dropdown-item')].find((i) =>
      i.textContent!.startsWith('Write to file'),
    )!;
    item.click();
    await flush();
    expect(toasts).toEqual([]);
  });
});

describe('choosing a card’s icon', () => {
  it('sets the chosen icon on that card, and clears it with No Icon', async () => {
    const idx = st.vault.api_keys.findIndex((e) => e.id === 'a');
    document
      .querySelector<HTMLElement>(`#card-grid [data-action="icon"][data-idx="${idx}"]`)!
      .click();
    expect($('icon-picker-overlay').classList.contains('open')).toBe(true);
    $<HTMLInputElement>('icon-manual').value = 'github';
    $('icon-manual-apply').click();
    $('icon-picker-close').click();
    await flush();
    expect(st.vault.api_keys[idx].custom_icon).toBe('github');
    document
      .querySelector<HTMLElement>(`#card-grid [data-action="icon"][data-idx="${idx}"]`)!
      .click();
    $('icon-clear').click();
    $('icon-picker-close').click();
    await flush();
    expect(st.vault.api_keys[idx].custom_icon).toBeUndefined();
  });
});
