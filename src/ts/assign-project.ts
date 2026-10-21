// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
/**
 * @file
 * Put entries on a project (and take them off) from a card or the bulk bar.
 *
 * Projects are the sidebar's "Projects" section and `entry.projectIds`; categories are
 * a separate flat list. The edit form could already pick projects, but nothing outside
 * it could, so putting ten secrets on one project meant ten edits.
 */
import type { VaultEntry } from './types';
import { st, persist, triggerRender } from './state';
import { showToast } from './utils';
import { showDropdown } from './modals';
import { openProjectCreateModal } from './projects';

/**
 * Toggle `projectId` on every entry: if all of them already carry it, remove it from all;
 * otherwise add it to the ones that lack it. `Universal` is never touched.
 * @returns how many entries changed.
 */
export function toggleProjectOn(entries: VaultEntry[], projectId: string): number {
  if (!projectId || projectId === 'Universal') return 0;
  const all = entries.every((e) => (e.projectIds ?? []).includes(projectId));
  let changed = 0;
  for (const e of entries) {
    const has = (e.projectIds ?? []).includes(projectId);
    if (all && has) {
      e.projectIds = e.projectIds.filter((p) => p !== projectId);
      changed++;
    } else if (!all && !has) {
      e.projectIds = [...(e.projectIds ?? ['Universal']), projectId];
      changed++;
    }
  }
  return changed;
}

export function openProjectMenu(anchor: HTMLElement, entries: VaultEntry[]): void {
  if (!entries.length) return;
  const projects = st.vault.projects.filter((p) => p.id !== 'Universal');
  if (!projects.length) {
    showDropdown(anchor, [{ label: 'New project…', fn: openProjectCreateModal }]);
    return;
  }
  showDropdown(anchor, [
    ...projects.map((p) => ({
      label: `${entries.every((e) => (e.projectIds ?? []).includes(p.id)) ? '✓ ' : ''}${p.name}`,
      fn: () => {
        const on = !entries.every((e) => (e.projectIds ?? []).includes(p.id));
        toggleProjectOn(entries, p.id);
        void persist();
        triggerRender();
        showToast(
          `${on ? 'Added to' : 'Removed from'} ${p.name}${entries.length > 1 ? ` (${entries.length})` : ''}`,
          'ok',
          1800,
        );
      },
    })),
    '---' as const,
    { label: 'New project…', fn: openProjectCreateModal },
  ]);
}
