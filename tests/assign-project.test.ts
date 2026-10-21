// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
/**
 * Putting entries on a real project from the card menu and the bulk bar. The edit form was
 * the only place that could, so assigning ten secrets meant ten edits.
 */
import { describe, it, expect, beforeEach } from 'vitest';
import { st } from '../src/ts/state';
import { toggleProjectOn, openProjectMenu } from '../src/ts/assign-project';
import { loadRealIndexHtml, makeEntry, makeProject, makeVault } from './helpers';

beforeEach(() => {
  loadRealIndexHtml();
  st.vault = makeVault({
    projects: [
      { id: 'Universal', name: 'Universal', description: '' },
      makeProject({ id: 'web', name: 'Web' }),
    ],
  });
});

describe('toggleProjectOn', () => {
  it('adds the project to entries that lack it and never touches Universal', () => {
    const a = makeEntry({ id: 'a' });
    const b = makeEntry({ id: 'b', projectIds: ['Universal', 'web'] });
    expect(toggleProjectOn([a, b], 'web')).toBe(1);
    expect(a.projectIds).toEqual(['Universal', 'web']);
    expect(b.projectIds).toEqual(['Universal', 'web']);
  });

  it('removes it from all only when every entry already has it', () => {
    const a = makeEntry({ id: 'a', projectIds: ['Universal', 'web'] });
    const b = makeEntry({ id: 'b', projectIds: ['Universal', 'web'] });
    expect(toggleProjectOn([a, b], 'web')).toBe(2);
    expect(a.projectIds).toEqual(['Universal']);
  });

  it('ignores Universal and an empty id', () => {
    const a = makeEntry({ id: 'a' });
    expect(toggleProjectOn([a], 'Universal')).toBe(0);
    expect(toggleProjectOn([a], '')).toBe(0);
    expect(a.projectIds).toEqual(['Universal']);
  });
});

describe('the project menu', () => {
  const items = () =>
    [...document.querySelectorAll('#dropdown .dropdown-item')].map((i) => i.textContent);

  it('lists the real projects and offers a new one', () => {
    openProjectMenu(document.body, [makeEntry({ id: 'a' })]);
    expect(items()).toEqual(['Web', 'New project…']);
  });

  it('offers only a new project when none exist, instead of an empty list', () => {
    st.vault.projects = [{ id: 'Universal', name: 'Universal', description: '' } as never];
    openProjectMenu(document.body, [makeEntry({ id: 'a' })]);
    expect(items()).toEqual(['New project…']);
  });
});
