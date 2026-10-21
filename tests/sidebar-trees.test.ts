// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
/**
 * The category and project trees in the sidebar: nesting by "/", the grey
 * virtual parent for a path whose parent was never created, per-branch counts
 * that include everything beneath, the active row, and the badge on a
 * structured project.
 */
import { describe, it, expect, beforeEach } from 'vitest';
import { bootApp, entry, teardownApp } from './app-boot';

type St = Awaited<ReturnType<typeof bootApp>>['st'];
let st: St;

const rows = (id: string) =>
  [...document.querySelectorAll<HTMLElement>(`#${id} .sidebar-cat-row`)].map((r) => ({
    label: r.querySelector('.sidebar-label')!.textContent!.replace(/\s+/g, ' ').trim(),
    count: r.querySelector('.sidebar-count')!.textContent,
    indent: r.style.paddingLeft,
    active: r.querySelector('.sidebar-item')!.classList.contains('active'),
    current: r.querySelector('.sidebar-item')!.getAttribute('aria-current'),
    editable: !!r.querySelector('.rename-cat, .rename-proj'),
  }));

async function boot() {
  ({ st } = await bootApp(
    [
      entry('a', 'Alpha', { categories: ['infra/db'], projectIds: ['Universal', 'team/app'] }),
      entry('b', 'Beta', {
        categories: ['infra/web', 'solo'],
        projectIds: ['Universal', 'team/app'],
      }),
      entry('c', 'Gamma', { categories: ['a/b/c'], projectIds: ['Universal', 'lab'] }),
    ],
    {
      categories: ['infra/db', 'infra/web', 'solo', 'a/b/c'],
      projects: [
        { id: 'Universal', name: 'Universal' },
        { id: 'team/app', name: 'team/app' },
        { id: 'lab', name: 'lab', project_type: 'wireguard' },
        { id: 'k', name: 'k', project_type: 'kubernetes' },
        { id: 'weird', name: 'weird', project_type: '<i>odd</i>' },
        { id: 'plain', name: 'plain', project_type: 'generic' },
      ],
    },
  ));
}

beforeEach(async () => {
  await boot();
}, 30000);

describe('the category tree', { timeout: 30000 }, () => {
  it('nests by "/", sorts siblings, and greys out a parent nobody created', () => {
    expect(rows('category-tree').map((r) => [r.label, r.indent, r.editable])).toEqual([
      ['a', '0px', false],
      ['b', '14px', false],
      ['c', '28px', true],
      ['infra', '0px', false],
      ['db', '14px', true],
      ['web', '14px', true],
      ['solo', '0px', true],
    ]);
  });

  it('counts everything beneath a branch, once per entry', () => {
    expect(rows('category-tree').map((r) => r.count)).toEqual(['1', '1', '1', '2', '1', '1', '1']);
  });

  it('marks only the chosen category as active, for assistive tech as well as by colour', () => {
    document.querySelector<HTMLElement>('#category-tree [data-filter-value="infra/db"]')!.click();
    const active = rows('category-tree').filter((r) => r.active);
    expect(active.map((r) => r.label)).toEqual(['db']);
    expect(active[0].current).toBe('true');
    document.querySelector<HTMLElement>('#category-tree [data-filter-value="infra"]')!.click();
    expect(
      rows('category-tree')
        .filter((r) => r.active)
        .map((r) => r.label),
    ).toEqual(['infra']);
  });

  it('a grey parent can be chosen as a filter and narrows to its branch', () => {
    document.querySelector<HTMLElement>('#category-tree [data-filter-value="a"]')!.click();
    expect(document.querySelectorAll('#card-grid .card')).toHaveLength(1);
  });
});

describe('the project tree', { timeout: 30000 }, () => {
  it('nests by "/", greys out a parent that has no project of its own, and badges structured types', () => {
    const r = rows('project-list');
    expect(r.map((x) => [x.label, x.indent, x.editable])).toEqual([
      ['k K8s', '0px', true],
      ['lab WG', '0px', true],
      ['plain', '0px', true],
      ['team', '0px', false],
      ['app', '14px', true],
      ['weird <i>odd</i>', '0px', true],
    ]);
  });

  it('shows an unknown project type as text, never as markup', () => {
    expect(document.querySelector('#project-list i')).toBeNull();
  });

  it('counts the entries of a project and everything nested under it', () => {
    const byLabel = Object.fromEntries(rows('project-list').map((r) => [r.label, r.count]));
    expect(byLabel['team']).toBe('2');
    expect(byLabel['app']).toBe('2');
    expect(byLabel['lab WG']).toBe('1');
  });

  it('marks the selected project active, including a grey parent', () => {
    document.querySelector<HTMLElement>('#project-list [data-project-id="virtual:team"]')!.click();
    expect(st.currentSelectedProjectIds).toEqual(['virtual:team']);
    const active = rows('project-list').filter((r) => r.active);
    expect(active.map((r) => r.label)).toEqual(['team']);
    expect(active[0].current).toBe('true');
  });
});

describe('the price and environment rows', { timeout: 30000 }, () => {
  it('count the entries of each kind and mark only the chosen row', async () => {
    expect(document.getElementById('count-all')!.textContent).toBe('3');
    expect(document.getElementById('count-paid')!.textContent).toBe('3');
    expect(document.getElementById('count-free')!.textContent).toBe('0');
    expect(document.getElementById('count-env-production')!.textContent).toBe('3');
    document
      .querySelector<HTMLElement>('[data-filter-type="env"][data-filter-value="production"]')!
      .click();
    await new Promise((r) => setTimeout(r, 50));
    const on = [...document.querySelectorAll('.sidebar-item[data-filter-type].active')].map(
      (b) => (b as HTMLElement).dataset.filterType + ':' + (b as HTMLElement).dataset.filterValue,
    );
    // Regression: "All" used to stay lit beside an environment filter, because it also
    // matched on type and value and the empty value always matched.
    expect(on).toEqual(['env:production']);
  });
});

afterAllTeardown();
function afterAllTeardown() {
  // Listeners are bound to the shared document; leave it clean for the next file.
  void import('vitest').then(({ afterAll }) => afterAll(() => teardownApp()));
}

describe('folding a nested tree shut', { timeout: 30000 }, () => {
  const caret = (tree: string, key: string) =>
    document.querySelector<HTMLElement>(`#${tree} [data-tree-toggle="${key}"]`);
  const labels = (id: string) => rows(id).map((r) => r.label);

  it('hides the children of a folded category and a folded project, and shows them again', () => {
    expect(labels('category-tree')).toContain('db');
    caret('category-tree', 'c:infra')!.click();
    expect(labels('category-tree')).not.toContain('db');
    expect(labels('category-tree')).toContain('infra');
    expect(caret('category-tree', 'c:infra')!.getAttribute('aria-expanded')).toBe('false');
    caret('category-tree', 'c:infra')!.click();
    expect(labels('category-tree')).toContain('db');

    expect(labels('project-list')).toContain('app');
    caret('project-list', 'p:virtual:team')!.click();
    expect(labels('project-list')).not.toContain('app');
  });

  it('keeps the fold across a render, gives a leaf a spacer, and does not fold the active filter away', () => {
    caret('category-tree', 'c:infra')!.click();
    expect(document.querySelectorAll('#category-tree .tree-caret-spacer').length).toBeGreaterThan(
      0,
    );
    expect(st.vault.api_keys).toHaveLength(3);
    expect(labels('category-tree')).not.toContain('web');
  });
});
