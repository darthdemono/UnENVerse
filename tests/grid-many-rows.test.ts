// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
/**
 * R15: past MANY_ROWS visible rows the grid is marked so the stylesheet can let the browser skip
 * off-screen cards. Nothing is removed from the page: every card is still there to search, focus
 * and tab through.
 */
import { describe, it, expect, afterAll } from 'vitest';
import { bootApp, entry, teardownApp } from './app-boot';
import type { VaultEntry } from '../src/ts/types';

afterAll(() => teardownApp());

const many = (n: number) =>
  Array.from({ length: n }, (_, i) => entry(`e${i}`, `Svc ${i}`)) as unknown as VaultEntry[];

describe('the many-rows grid', { timeout: 60000 }, () => {
  it('is marked only above the threshold, and keeps every card in the page', async () => {
    const { st } = await bootApp(many(3) as never, {});
    const { renderGrid, MANY_ROWS } = await import('../src/ts/render');
    const grid = document.getElementById('card-grid')!;
    renderGrid();
    expect(grid.classList.contains('many-rows')).toBe(false);

    st.vault.api_keys = many(MANY_ROWS);
    renderGrid();
    expect(grid.classList.contains('many-rows'), 'exactly at the limit is not many').toBe(false);

    st.vault.api_keys = many(MANY_ROWS + 1);
    renderGrid();
    expect(grid.classList.contains('many-rows')).toBe(true);
    expect(grid.querySelectorAll('.card').length).toBeGreaterThanOrEqual(MANY_ROWS + 1);

    st.vault.api_keys = many(3);
    renderGrid();
    expect(grid.classList.contains('many-rows')).toBe(false);
  });

  it('has a stylesheet rule that skips off-screen cards only under the mark', async () => {
    const { readFileSync } = await import('node:fs');
    const css = readFileSync('src/css/cards.css', 'utf8');
    expect(css).toMatch(/#card-grid\.many-rows \.card\s*\{[^}]*content-visibility:\s*auto/);
    // The plain card rule must not carry it, or small vaults would pay for it too.
    const plain = css.slice(css.indexOf('.card {'), css.indexOf('}', css.indexOf('.card {')));
    expect(plain).not.toContain('content-visibility');
  });
});
