// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
// Tools, Site families (R40). Normalising hosts happens in Rust; these tests stand in for it with a
// recording stub and check what the pane stores, shows and sends.
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { initFamiliesPane, paintFamilies } from '../src/ts/families-pane';
import { st, switchTool } from '../src/ts/state';
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
const tick = () => new Promise((r) => setTimeout(r, 0));

let calls: { cmd: string; args: Record<string, unknown> }[] = [];
let answer: (cmd: string, args: Record<string, unknown>) => unknown;
let saves = 0;
const stubTauri = () => {
  (window as unknown as { __TAURI__: unknown }).__TAURI__ = {
    core: {
      invoke: async (cmd: string, args: Record<string, unknown>) => {
        calls.push({ cmd, args });
        return answer(cmd, args);
      },
    },
  };
};
const families = () =>
  (st.vault as unknown as { site_families?: { name: string; hosts: string[] }[] }).site_families;

beforeEach(async () => {
  resetState(st);
  loadRealIndexHtml();
  mountToolsPanes();
  toasts.length = 0;
  calls = [];
  saves = 0;
  confirmAnswer = true;
  answer = (_cmd, a) => ({
    name: String(a.name).trim(),
    hosts: (a.hosts as string[]).map((h) => h.toLowerCase()),
  });
  st.vault = makeVault({
    api_keys: [],
    site_families: [{ name: 'mine', hosts: ['a.test', 'b.test'] }],
  } as never);
  st.vaultOpen = true;
  st.store = {
    save: async () => {
      saves += 1;
    },
    isRemote: false,
  } as never;
  stubTauri();
  initFamiliesPane();
  await tick();
});

afterEach(() => {
  delete (window as unknown as { __TAURI__?: unknown }).__TAURI__;
});

describe('the pane', () => {
  it('has a nav button and labelled controls, and opens through the tool switcher', async () => {
    expect(document.querySelector('.tool-nav-btn[data-tool="families"]')).not.toBeNull();
    const pane = $('tool-families');
    for (const c of pane.querySelectorAll<HTMLElement>('input, select, textarea')) {
      const labelled =
        c.getAttribute('aria-label') ||
        (c.id && pane.querySelector(`label[for="${c.id}"]`)) ||
        c.closest('label');
      expect(labelled, c.id).toBeTruthy();
    }
    switchTool('families');
    await tick();
    expect(pane.style.display).toBe('');
  });

  it('lists yours and the shipped ones, marking a shipped one that yours replaces', () => {
    expect($('fam-list').textContent).toContain('a.test');
    expect($('fam-list').textContent).toContain('metabrainz');
    expect($('fam-status').textContent).toMatch(/^1 of your own, \d+ shipped$/);
    (st.vault as never as { site_families: unknown }).site_families = [
      { name: 'google', hosts: ['x.test'] },
    ];
    paintFamilies();
    expect($('fam-list').textContent).toContain('shipped, replaced by yours');
  });

  it('paints nothing when the pane is not mounted', () => {
    $('tool-families').remove();
    expect(() => paintFamilies()).not.toThrow();
    expect(() => initFamiliesPane()).not.toThrow();
  });
});

describe('adding', () => {
  it('asks Rust to normalise, stores the family and clears the form', async () => {
    $<HTMLInputElement>('fam-name').value = ' Brainz ';
    $<HTMLTextAreaElement>('fam-hosts').value = 'A.Example, b.example\nc.example';
    $('fam-add').click();
    await tick();
    expect(calls[0]).toMatchObject({
      cmd: 'site_family_normalise',
      args: { name: 'Brainz', hosts: ['A.Example', 'b.example', 'c.example'] },
    });
    expect(families()!.find((f) => f.name === 'Brainz')!.hosts).toEqual([
      'a.example',
      'b.example',
      'c.example',
    ]);
    expect($<HTMLInputElement>('fam-name').value).toBe('');
    expect(saves).toBe(1);
    expect(toasts[toasts.length - 1]).toBe('Family saved');
  });

  it('adds new hosts to an existing family without repeating one', async () => {
    $<HTMLInputElement>('fam-name').value = 'mine';
    $<HTMLTextAreaElement>('fam-hosts').value = 'b.test c.test';
    $('fam-add').click();
    await tick();
    expect(families()!.find((f) => f.name === 'mine')!.hosts).toEqual([
      'a.test',
      'b.test',
      'c.test',
    ]);
    expect(toasts[toasts.length - 1]).toBe('Hosts added to the family');
  });

  it('shows the reason when Rust refuses and stores nothing', async () => {
    answer = () => {
      throw new Error('the address has no host');
    };
    $<HTMLInputElement>('fam-name').value = 'x';
    $('fam-add').click();
    await tick();
    expect(toasts[toasts.length - 1]).toContain('no host');
    expect(saves).toBe(0);
  });

  it('creates the list when the vault has none', async () => {
    delete (st.vault as never as { site_families?: unknown }).site_families;
    $<HTMLInputElement>('fam-name').value = 'first';
    $<HTMLTextAreaElement>('fam-hosts').value = 'one.test';
    $('fam-add').click();
    await tick();
    expect(families()).toEqual([{ name: 'first', hosts: ['one.test'] }]);
  });

  it('needs the desktop app', async () => {
    delete (window as unknown as { __TAURI__?: unknown }).__TAURI__;
    $('fam-add').click();
    await tick();
    expect(toasts[toasts.length - 1]).toBe('Needs the desktop app.');
  });
});

describe('removing', () => {
  const press = async (action: string, name: string, host = '') => {
    const b = document.createElement('button');
    b.dataset.action = action;
    b.dataset.name = name;
    b.dataset.host = host;
    $('fam-list').append(b);
    b.click();
    await tick();
  };

  it('removes one host, and the family when its last host goes', async () => {
    await press('fam-host-rm', 'mine', 'a.test');
    expect(families()![0].hosts).toEqual(['b.test']);
    await press('fam-host-rm', 'mine', 'b.test');
    expect(families()).toEqual([]);
  });

  it('removes a whole family after a question, and not when declined', async () => {
    confirmAnswer = false;
    await press('fam-rm', 'mine');
    expect(families()).toHaveLength(1);
    confirmAnswer = true;
    await press('fam-rm', 'mine');
    expect(families()).toEqual([]);
    expect(saves).toBe(1);
  });

  it('ignores a family that is not there, and a click that is not on a button', async () => {
    await press('fam-rm', 'ghost');
    expect(saves).toBe(0);
    $('fam-list').click();
    await tick();
    expect(saves).toBe(0);
  });
});
