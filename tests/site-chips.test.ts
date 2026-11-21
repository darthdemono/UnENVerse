// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
/**
 * R40 slice 3: under the Addresses box each address has a row where its kind and its matching rule
 * are chosen. The box stays the one place addresses are typed; the rows only add what a line of
 * text cannot say. What the user picks is what is stored, and "Auto" and "Exact" mean "not set".
 */
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { st } from '../src/ts/state';
import {
  fillForm,
  formToEntry,
  openModal,
  populateProjectSelect,
  buildCatChips,
  renderSiteChips,
  seedSiteEdits,
} from '../src/ts/modals';
import { loadRealIndexHtml, makeVault, resetState } from './helpers';

vi.mock('../src/ts/utils', async (importOriginal) => {
  const real = await importOriginal<typeof import('../src/ts/utils')>();
  return { ...real, showToast: () => {}, showConfirm: async () => true };
});

const $ = <T extends HTMLElement = HTMLElement>(id: string) => document.getElementById(id) as T;
const rows = () => [...document.querySelectorAll<HTMLElement>('#f-sites-chips .site-chip')];
const pick = (url: string, field: 'kind' | 'mode', value: string) => {
  const sel = document.querySelector<HTMLSelectElement>(
    `#f-sites-chips select[data-site-field="${field}"][data-url="${CSS.escape(url)}"]`,
  )!;
  sel.value = value;
  sel.dispatchEvent(new Event('change'));
};
const type = (text: string) => {
  $<HTMLTextAreaElement>('f-sites').value = text;
  $('f-sites').dispatchEvent(new Event('input'));
};

beforeEach(() => {
  loadRealIndexHtml();
  resetState(st);
  st.vault = makeVault({ api_keys: [] });
  populateProjectSelect();
  buildCatChips([]);
  fillForm({} as never);
  openModal('Add', -1);
});

describe('the address rows', () => {
  it('shows one row per distinct address, each starting on Auto kind and the Exact rule', () => {
    type('localhost:8989\n\n localhost:8989 \nhttps://tv.home.lan');
    expect(rows().map((r) => r.querySelector('.site-chip-url')!.textContent)).toEqual([
      'localhost:8989',
      'https://tv.home.lan',
    ]);
    for (const row of rows()) {
      const [kind, mode] = [...row.querySelectorAll<HTMLSelectElement>('select')];
      expect([kind.value, mode.value]).toEqual(['', 'exact']);
    }
  });

  it('keeps the rows in step with the box as lines are added and removed', () => {
    type('a.test');
    expect(rows()).toHaveLength(1);
    type('a.test\nb.test:81');
    expect(rows()).toHaveLength(2);
    type('');
    expect(rows()).toHaveLength(0);
  });

  it('stores the kind and the matching rule that were picked', () => {
    type('localhost:8989\nhttps://api.example.com');
    pick('localhost:8989', 'kind', 'local');
    pick('localhost:8989', 'mode', 'any_port');
    pick('https://api.example.com', 'kind', 'api');
    const sites = formToEntry().sites!;
    expect(sites).toEqual([
      { url: 'localhost:8989', source: 'manual', kind: 'local', mode: 'any_port' },
      { url: 'https://api.example.com', source: 'manual', kind: 'api' },
    ]);
  });

  it('shows what is stored, and choosing Auto clears the kind again', () => {
    const stored = [
      {
        url: 'sonarr.lan:8989',
        kind: 'web',
        mode: 'host',
        source: 'manual',
        added_at: '2026-11-01',
      },
    ];
    seedSiteEdits({ sites: stored } as never);
    $<HTMLTextAreaElement>('f-sites').value = 'sonarr.lan:8989';
    renderSiteChips();
    const [kind, mode] = [...document.querySelectorAll<HTMLSelectElement>('#f-sites-chips select')];
    expect([kind.value, mode.value]).toEqual(['web', 'host']);
    // Untouched, the entry round trips with its date.
    expect(formToEntry({ sites: stored } as never).sites).toEqual(stored);
    pick('sonarr.lan:8989', 'kind', '');
    pick('sonarr.lan:8989', 'mode', 'exact');
    expect(formToEntry({ sites: stored } as never).sites).toEqual([
      { url: 'sonarr.lan:8989', source: 'manual', added_at: '2026-11-01', mode: 'exact' },
    ]);
  });

  it('forgets one entry choices when the next entry is opened', () => {
    type('x.test');
    pick('x.test', 'kind', 'api');
    fillForm({ sites: [{ url: 'x.test', source: 'manual' }] } as never);
    expect(formToEntry().sites).toEqual([{ url: 'x.test', source: 'manual' }]);
  });

  it('writes a hostile address as text, never as markup', () => {
    type('"><img src=x onerror=alert(1)>');
    expect(document.querySelector('#f-sites-chips img')).toBeNull();
    expect(rows()[0].querySelector('.site-chip-url')!.textContent).toBe(
      '"><img src=x onerror=alert(1)>',
    );
  });
});
