// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
/**
 * The Add form opens on the fields a new secret needs and folds the optional sections (R15);
 * editing opens them, because what is already filled in must be seen. Folding never removes a
 * field: every input is still in the form and is still read when the entry is saved.
 */
import { describe, it, expect, beforeEach, afterAll } from 'vitest';
import { bootApp, entry, teardownApp } from './app-boot';

afterAll(() => teardownApp());

const $ = <T extends HTMLElement = HTMLElement>(id: string) => document.getElementById(id) as T;
const flush = (ms = 30) => new Promise((r) => setTimeout(r, ms));
const more = () => $<HTMLDetailsElement>('f-more');

beforeEach(async () => {
  await bootApp([entry('a', 'Existing', { tags: ['x'], description: 'about it' })], {});
}, 30000);

describe('the progressive entry form', { timeout: 30000 }, () => {
  it('folds the optional sections when adding and shows the secret fields', async () => {
    $('add-btn').click();
    await flush();
    expect(more().open).toBe(false);
    expect($('f-provider')).not.toBeNull();
    expect($('f-key')).not.toBeNull();
    // The sections that stay visible hold the fields a new secret needs.
    expect(more().contains($('f-key'))).toBe(false);
    expect(more().contains($('f-provider'))).toBe(false);
  });

  it('keeps every optional field in the form, inside the fold', async () => {
    $('add-btn').click();
    await flush();
    for (const id of ['f-tags-input', 'f-desc', 'f-expires', 'f-rotation-days', 'f-compromised']) {
      expect(more().contains($(id)), id).toBe(true);
    }
  });

  it('opens the fold when editing an existing entry', async () => {
    const { openModal } = await import('../src/ts/modals');
    openModal('Edit entry', 0);
    await flush();
    expect(more().open).toBe(true);
    // And folds again for the next new entry.
    openModal('Add entry', -1);
    await flush();
    expect(more().open).toBe(false);
  });

  it('names what is inside the fold in its summary', () => {
    const text = $('f-more-summary').textContent ?? '';
    for (const word of ['two-factor', 'notes', 'organise', 'lifecycle']) {
      expect(text.toLowerCase()).toContain(word);
    }
  });
});
