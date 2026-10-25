// SPDX-License-Identifier: PolyForm-Strict-1.0.0
// Copyright Jubair Hasan.
/**
 * Tools -> Import: what a file in each supported format becomes in the vault.
 *
 * Driven through the real buttons. The questions are the ones a person moving
 * in from another manager asks: does my login keep its user name and URL, do
 * comments in a .env stay out, does a nameless row get refused rather than
 * crash the grid, and does the preview tell me what is about to land.
 */
import { describe, it, expect, beforeAll, beforeEach, vi } from 'vitest';
import { st, resetViewState } from '../src/ts/state';
import { loadRealIndexHtml, makeEntry, makeVault, resetState } from './helpers';

const toasts: string[] = [];
vi.mock('../src/ts/utils', async (importOriginal) => {
  const real = await importOriginal<typeof import('../src/ts/utils')>();
  return { ...real, showToast: (m: string) => toasts.push(m), showConfirm: async () => true };
});

const $ = (id: string) => document.getElementById(id)!;
let boundNodes: ChildNode[] = [];

beforeAll(async () => {
  loadRealIndexHtml();
  (await import('../src/ts/tools-markup')).mountToolsPanes();
  (await import('../src/ts/tools')).initTools();
  boundNodes = Array.from(document.body.childNodes);
});

beforeEach(() => {
  document.body.innerHTML = '';
  for (const node of boundNodes) document.body.appendChild(node);
  resetState(st);
  resetViewState();
  st.vault = makeVault({ api_keys: [] });
  toasts.length = 0;
  $('import-file-name').textContent = 'No file chosen';
  $('import-preview-text').textContent = '';
  $('import-status').textContent = '';
});

/** Pick a format, "choose" a file with this text, and wait for the preview. */
async function load(format: string, text: string, name = 'in.txt') {
  $('import-preview-text').textContent = '';
  $('import-status').textContent = '';
  document.querySelectorAll<HTMLElement>('.import-fmt-btn').forEach((b) => {
    if (b.dataset.fmt === format) b.click();
  });
  const spy = vi.spyOn(HTMLInputElement.prototype, 'click').mockImplementation(function (
    this: HTMLInputElement,
  ) {
    Object.defineProperty(this, 'files', { value: [new File([text], name)] });
    this.onchange?.(new Event('change'));
  });
  $('import-file-btn').click();
  spy.mockRestore();
  await vi.waitFor(() => expect($('import-preview-text').textContent).toContain('Found'));
}

const confirm = async () => {
  $('import-status').textContent = '';
  $('import-confirm-btn').click();
  await vi.waitFor(() => expect($('import-status').textContent).toContain('Imported'));
};

describe('.env files', () => {
  it('imports KEY=value lines, unquotes values, and leaves comments, blanks and malformed lines out', async () => {
    await load(
      'env',
      '# a comment\n\nTOKEN="abc123"\nOTHER=\'x y\'\nURL=https://h/?a=b\n=novalue\nnoequals\n',
    );
    expect($('import-preview-text').textContent).toContain('Found 3 entries');
    expect($('import-file-name').textContent).toBe('in.txt');
    await confirm();
    expect(st.vault.api_keys.map((e) => [e.provider, e.api_key])).toEqual([
      ['TOKEN', 'abc123'],
      ['OTHER', 'x y'],
      ['URL', 'https://h/?a=b'],
    ]);
    expect(st.vault.api_keys[0].secretType).toBe('env_var');
    expect(st.vault.api_keys.every((e) => e.id)).toBe(true);
  });

  it('previews at most five entries and says how many more there are', async () => {
    await load('env', Array.from({ length: 8 }, (_, i) => `K${i}=v${i}`).join('\n'));
    const text = $('import-preview-text').textContent!;
    expect(text).toContain('Found 8 entries');
    expect(text).toContain('+3 more');
    expect(text).toContain('K4');
    expect(text).not.toContain('K5');
  });
});

describe('Bitwarden export', () => {
  const login = (o: object) => ({
    name: 'Site',
    login: { username: 'me', password: 'pw-1234567890', uris: [{ uri: 'https://site.test' }] },
    ...o,
  });

  it('keeps name, user name, password and address from a login', async () => {
    await load('bitwarden', JSON.stringify({ items: [login({ notes: 'note' })] }));
    await confirm();
    expect(st.vault.api_keys[0]).toMatchObject({
      provider: 'Site',
      account_name: 'me',
      api_key: 'pw-1234567890',
      api_url: 'https://site.test',
      secretType: 'password',
      description: 'note',
    });
  });

  it('falls back to the notes for an item with no password (a secure note), and accepts a bare list', async () => {
    await load('bitwarden', JSON.stringify([{ name: 'Note', notes: 'the secret text' }, {}]));
    await confirm();
    expect(st.vault.api_keys.map((e) => [e.provider, e.api_key])).toEqual([
      ['Note', 'the secret text'],
      ['Unknown', ''],
    ]);
  });

  it('finds nothing in a file that is not JSON', async () => {
    document.querySelector<HTMLElement>('[data-fmt="bitwarden"]')!.click();
    await load('bitwarden', 'not json');
    expect($('import-preview-text').textContent).toContain('Found 0 entries');
    expect($('import-confirm-btn').style.display).toBe('none');
  });
});

describe('1Password export', () => {
  it('reads the password and user name by designation or id, and keeps tags as categories', async () => {
    await load(
      '1password',
      JSON.stringify([
        {
          title: 'Bank',
          tags: ['finance', 7],
          fields: [
            { designation: 'username', value: 'joy' },
            { id: 'password', value: 'p4ss-word-long' },
          ],
        },
        { title: 'Empty' },
      ]),
    );
    await confirm();
    const [bank, empty] = st.vault.api_keys;
    expect(bank).toMatchObject({
      provider: 'Bank',
      account_name: 'joy',
      api_key: 'p4ss-word-long',
      categories: ['finance'],
    });
    expect(empty).toMatchObject({ provider: 'Empty', api_key: '' });
    expect(st.vault.api_keys).toHaveLength(2);
  });

  it('finds nothing in an object (1Password exports a list) or in broken JSON', async () => {
    await load('1password', '{"a":1}');
    expect($('import-preview-text').textContent).toContain('Found 0 entries');
    await load('1password', '{');
    expect($('import-preview-text').textContent).toContain('Found 0 entries');
  });
});

describe('JSON and YAML', () => {
  it('JSON: a bare list or a vault with api_keys; skips rows with no provider and says how many', async () => {
    await load(
      'json',
      JSON.stringify({
        api_keys: [
          { provider: 'A', api_key: 'x' },
          { api_key: 'orphan' },
          { name: 'B', price_type: 'weird' },
        ],
      }),
    );
    await confirm();
    expect(st.vault.api_keys.map((e) => e.provider)).toEqual(['A', 'B']);
    expect(st.vault.api_keys[1].price_type).toBe('free');
    expect($('import-status').textContent).toContain('skipped 1 with no provider name');
    st.vault.api_keys = [];
    await load('json', '[{"provider":"Solo"}]');
    await confirm();
    expect(st.vault.api_keys).toHaveLength(1);
    await load('json', '{"nothing":true}');
    expect($('import-preview-text').textContent).toContain('Found 0');
    await load('json', 'oops');
    expect($('import-preview-text').textContent).toContain('Found 0');
  });

  it('YAML: a list, a vault, or a flat KEY: value map (non-strings are kept as JSON text)', async () => {
    await load('yaml', '- provider: L\n  api_key: k\n');
    await confirm();
    expect(st.vault.api_keys[0].provider).toBe('L');
    st.vault.api_keys = [];
    await load('yaml', 'api_keys:\n  - provider: V\n');
    await confirm();
    expect(st.vault.api_keys[0].provider).toBe('V');
    st.vault.api_keys = [];
    await load('yaml', 'TOKEN: abc\nPORT: 8080\nLIST: [1, 2]\n');
    await confirm();
    expect(st.vault.api_keys.map((e) => [e.provider, e.api_key])).toEqual([
      ['TOKEN', 'abc'],
      ['PORT', '8080'],
      ['LIST', '[1,2]'],
    ]);
    await load('yaml', '42');
    expect($('import-preview-text').textContent).toContain('Found 0');
    await load('yaml', 'a: [unclosed');
    expect($('import-preview-text').textContent).toContain('Found 0');
  });
});

describe('confirming', () => {
  it('refuses to import when nothing was loaded', () => {
    $('import-confirm-btn').click();
    expect(toasts).toContain('Nothing to import — load a file first');
    expect(st.vault.api_keys).toHaveLength(0);
  });

  it('refuses a file whose every row lacks a provider, and keeps the existing vault untouched', async () => {
    st.vault.api_keys = [makeEntry({ provider: 'Existing' })];
    await load('json', '[{"api_key":"a"},{"api_key":"b"}]');
    $('import-confirm-btn').click();
    await vi.waitFor(() => expect(toasts.some((t) => t.includes('Nothing importable'))).toBe(true));
    expect(st.vault.api_keys.map((e) => e.provider)).toEqual(['Existing']);
  });

  it('appends to what is already there and hides the preview afterwards', async () => {
    st.vault.api_keys = [makeEntry({ provider: 'Existing' })];
    await load('env', 'NEW=1');
    await confirm();
    expect(st.vault.api_keys.map((e) => e.provider)).toEqual(['Existing', 'NEW']);
    expect($('import-preview').style.display).toBe('none');
    expect($('import-confirm-btn').style.display).toBe('none');
    expect(toasts).toContain('Imported 1 entry');
  });

  it('choosing no file changes nothing', () => {
    const spy = vi.spyOn(HTMLInputElement.prototype, 'click').mockImplementation(function (
      this: HTMLInputElement,
    ) {
      Object.defineProperty(this, 'files', { value: [] });
      this.onchange?.(new Event('change'));
    });
    $('import-file-btn').click();
    spy.mockRestore();
    expect($('import-file-name').textContent).toBe('No file chosen');
  });
});
