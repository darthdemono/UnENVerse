// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
/**
 * R40: an entry stored with addresses must come out of the entry form as it went in. The fixture is
 * what a 0.81 desktop saved, and `vault-core/src/sites.rs` reads the same bytes. A desktop that has
 * no control for a field must not lose it (the R07 lesson), and one that has the Addresses rows must
 * not change a kind, mode, label, source or date nobody touched.
 */
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { readFileSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { st } from '../src/ts/state';
import {
  fillForm,
  formToEntry,
  openModal,
  populateProjectSelect,
  buildCatChips,
} from '../src/ts/modals';
import { loadRealIndexHtml, makeVault, resetState } from './helpers';

vi.mock('../src/ts/utils', async (importOriginal) => {
  const real = await importOriginal<typeof import('../src/ts/utils')>();
  return { ...real, showToast: () => {}, showConfirm: async () => true };
});

const stored = JSON.parse(
  readFileSync(
    join(dirname(fileURLToPath(import.meta.url)), 'fixtures', 'parity', 'entry-sites.json'),
    'utf8',
  ),
);

beforeEach(() => {
  loadRealIndexHtml();
  resetState(st);
  st.vault = makeVault({ api_keys: [structuredClone(stored)] });
  populateProjectSelect();
  buildCatChips([]);
});

describe('an entry with addresses, through the entry form', () => {
  it('comes back with every address, kind, mode, label, source and date it had', () => {
    fillForm(structuredClone(stored));
    openModal('Edit', 0);
    fillForm(structuredClone(stored));
    const back = formToEntry(structuredClone(stored));
    expect(back.sites).toEqual(stored.sites);
    expect(back.site_family).toBe('servarr');
    expect(back.api_url).toBe(stored.api_url);
  });

  it('keeps a field this build has no control for', () => {
    fillForm(structuredClone(stored));
    const back = formToEntry(structuredClone(stored)) as unknown as Record<string, unknown>;
    expect(back.a_field_from_a_newer_build).toEqual({ kept: true });
  });

  it('shows one row per stored address with its stored kind and rule selected', () => {
    openModal('Edit', 0);
    fillForm(structuredClone(stored));
    const rows = [...document.querySelectorAll('#f-sites-chips .site-chip')];
    expect(rows).toHaveLength(3);
    const values = rows.map((r) =>
      [...r.querySelectorAll<HTMLSelectElement>('select')].map((s) => s.value),
    );
    expect(values).toEqual([
      ['local', 'any_port'],
      ['web', 'exact'],
      ['api', 'host'],
    ]);
  });
});
