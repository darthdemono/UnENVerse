// SPDX-License-Identifier: PolyForm-Strict-1.0.0
// Copyright Jubair Hasan.
/**
 * The app shell once a vault is open: the sidebar filters, the type chips, the
 * search box, the sort menu and the layout controls.
 *
 * Booted for real (the module wires itself on import), against a small vault,
 * and driven through the same buttons a person presses. Every control here
 * narrows or reorders what is on screen, so the checks are about what the grid
 * shows afterwards and whether pressing the same thing twice puts it back.
 */
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { loadRealIndexHtml } from './helpers';

vi.mock('../src/ts/utils', async (importOriginal) => {
  const real = await importOriginal<typeof import('../src/ts/utils')>();
  return {
    ...real,
    showToast: () => {},
    showConfirm: async () => true,
    showPrompt: async () => null,
  };
});

const $ = <T extends HTMLElement = HTMLElement>(id: string) => document.getElementById(id) as T;
const shown = () =>
  [...document.querySelectorAll<HTMLElement>('#card-grid .card')].map(
    (c) => /^[A-Z] (\w+)/.exec(c.textContent!.replace(/\s+/g, ' ').trim())![1],
  );
const flush = (ms = 30) => new Promise((r) => setTimeout(r, ms));

const e = (id: string, provider: string, o: object = {}) => ({
  id,
  provider,
  api_key: `k-${id}-longenough`,
  price_type: 'paid',
  secretType: 'api_key',
  categories: ['infra'],
  projectIds: ['Universal'],
  tags: ['prod'],
  env_prefixes: ['VITE'],
  environment: 'production',
  ...o,
});

let st: (typeof import('../src/ts/state'))['st'];
let Settings: (typeof import('../src/ts/state'))['Settings'];

async function boot(extra: object = {}) {
  vi.resetModules();
  loadRealIndexHtml();
  localStorage.clear();
  localStorage.setItem(
    'unenverse-settings',
    JSON.stringify({ onboardingCompleted: true, groupPools: false, ...extra }),
  );
  delete (window as any).__TAURI__;
  sessionStorage.setItem(
    'unenverse',
    JSON.stringify({
      api_keys: [
        e('a', 'Alpha', { price_type: 'paid', projectIds: ['Universal', 'web'] }),
        e('b', 'Beta', {
          pool: 'main',
          tags: [],
          price_type: 'free',
          categories: [],
          environment: 'staging',
          env_prefixes: [],
          expires_at: '2030-01-01',
        }),
        e('c', 'Gamma', {
          pool: 'main',
          secretType: 'password',
          tags: ['prod', 'eu'],
          expires_at: '2029-01-01',
        }),
      ],
      user_categories: ['infra'],
      projects: [
        { id: 'Universal', name: 'Universal' },
        { id: 'web', name: 'Web' },
      ],
    }),
  );
  ({ st, Settings } = await import('../src/ts/state'));
  await import('../src/ts/vault');
  await vi.waitFor(() => expect(st.vaultOpen).toBe(true), { timeout: 15000 });
  await flush(100);
}

const side = (sel: string) => document.querySelector<HTMLElement>(`#sidebar ${sel}`)!;

beforeEach(() => boot(), 30000);

describe('sidebar filters', { timeout: 30000 }, () => {
  it('start showing everything', () => {
    expect(shown().sort()).toEqual(['Alpha', 'Beta', 'Gamma']);
  });

  it('price: choosing a price narrows the grid, choosing it again puts everything back', () => {
    side('[data-filter-type="price"][data-filter-value="free"]').click();
    expect(shown()).toEqual(['Beta']);
    side('[data-filter-type="price"][data-filter-value="free"]').click();
    expect(shown().sort()).toEqual(['Alpha', 'Beta', 'Gamma']);
  });

  it('environment, category and tag filters each narrow, and toggle off', () => {
    side('[data-filter-type="env"][data-filter-value="staging"]').click();
    expect(shown()).toEqual(['Beta']);
    side('[data-filter-type="env"][data-filter-value="staging"]').click();
    expect(shown()).toHaveLength(3);
    side('[data-filter-type="category"][data-filter-value="infra"]').click();
    expect(shown().sort()).toEqual(['Alpha', 'Gamma']);
    side('[data-filter-type="category"][data-filter-value="infra"]').click();
    side('.tag-filter-btn[data-tag="eu"]').click();
    expect(shown()).toEqual(['Gamma']);
    expect(st.activeTagFilter).toBe('eu');
    side('.tag-filter-btn[data-tag="eu"]').click();
    expect(st.activeTagFilter).toBeNull();
  });

  it('pool and env-prefix filters narrow and toggle off', () => {
    side('.pool-filter-btn[data-pool="main"]').click();
    expect(shown().sort()).toEqual(['Beta', 'Gamma']);
    expect(st.activePoolFilter).toBe('main');
    side('.pool-filter-btn[data-pool="main"]').click();
    expect(st.activePoolFilter).toBeNull();
    side('.prefix-filter-btn[data-prefix="VITE"]').click();
    expect(shown().sort()).toEqual(['Alpha', 'Gamma']);
    side('.prefix-filter-btn[data-prefix="VITE"]').click();
    expect(st.activePrefixFilter).toBeNull();
  });

  it('"All Secrets" clears every narrowing at once, including project, tag, prefix, pool and environment', () => {
    st.currentSelectedProjectIds = ['web'];
    st.currentEnvFilter = 'staging';
    st.activeTagFilter = 'prod';
    st.activePrefixFilter = 'VITE';
    st.activePoolFilter = 'main';
    side('[data-filter-type="all"]').click();
    expect(st.currentSelectedProjectIds).toEqual(['Universal']);
    expect(st.currentEnvFilter).toBe('');
    expect(st.activeTagFilter).toBeNull();
    expect(st.activePrefixFilter).toBeNull();
    expect(st.activePoolFilter).toBeNull();
    expect(shown()).toHaveLength(3);
  });

  it('choosing a project shows only its entries; choosing it again returns to everything', () => {
    document.querySelector<HTMLElement>('#project-list [data-project-id="web"]')!.click();
    expect(shown()).toEqual(['Alpha']);
    document.querySelector<HTMLElement>('#project-list [data-project-id="web"]')!.click();
    expect(st.currentSelectedProjectIds).toEqual(['Universal']);
    expect(shown()).toHaveLength(3);
  });

  it('a click that is not on a filter does nothing', () => {
    $('sidebar').click();
    expect(shown()).toHaveLength(3);
  });
});

describe('type chips', { timeout: 30000 }, () => {
  it('add and remove one type at a time, combining with each other', () => {
    const chip = (c: string) =>
      document.querySelector<HTMLElement>(`.type-chip[data-chip="${c}"]`)!;
    chip('password').click();
    expect(shown()).toEqual(['Gamma']);
    chip('api_key').click();
    expect(shown().sort()).toEqual(['Alpha', 'Beta', 'Gamma']);
    chip('password').click();
    expect(shown().sort()).toEqual(['Alpha', 'Beta']);
    chip('api_key').click();
    expect(shown()).toHaveLength(3);
    document.getElementById('type-chip-bar')!.click();
    expect(shown()).toHaveLength(3);
  });
});

describe('search', { timeout: 30000 }, () => {
  it('narrows after a short pause, shows the clear button, and clearing brings everything back', async () => {
    const box = $<HTMLInputElement>('search');
    box.value = 'gam';
    box.dispatchEvent(new Event('input', { bubbles: true }));
    expect($('search-clear').classList.contains('visible')).toBe(true);
    expect(shown()).toHaveLength(3);
    await flush(250);
    expect(shown()).toEqual(['Gamma']);
    $('search-clear').click();
    expect(box.value).toBe('');
    expect(st.searchQ).toBe('');
    expect($('search-clear').classList.contains('visible')).toBe(false);
    expect(shown()).toHaveLength(3);
  });

  it('emptying the box hides the clear button', async () => {
    const box = $<HTMLInputElement>('search');
    box.value = 'x';
    box.dispatchEvent(new Event('input', { bubbles: true }));
    box.value = '';
    box.dispatchEvent(new Event('input', { bubbles: true }));
    expect($('search-clear').classList.contains('visible')).toBe(false);
    await flush(250);
    expect(shown()).toHaveLength(3);
  });
});

describe('sorting and expanding', { timeout: 30000 }, () => {
  const choose = async (label: string) => {
    $('sort-btn').click();
    [...document.querySelectorAll<HTMLElement>('#dropdown .dropdown-item')]
      .find((i) => i.textContent!.includes(label))!
      .click();
    await flush();
  };

  it('offers the four orders, marks the current one, and applies and remembers the choice', async () => {
    $('sort-btn').click();
    const items = [...document.querySelectorAll<HTMLElement>('#dropdown .dropdown-item')];
    expect(items.map((i) => i.textContent!.trim().replace(/^✓\s*/, ''))).toEqual([
      'A – Z',
      'Price Type',
      'Category',
      'Expiry',
    ]);
    expect(items[0].classList.contains('active')).toBe(true);
    items[1].click();
    await flush();
    expect(st.currentSortBy).toBe('price');
    expect(Settings.get('lastSortBy')).toBe('price');
    expect($('sort-label-text').textContent).toBe('Price Type');
    expect(shown()).toEqual(['Beta', 'Alpha', 'Gamma']);
  });

  it('expiry puts the soonest first and entries without one last', async () => {
    await choose('Expiry');
    expect(shown()).toEqual(['Gamma', 'Beta', 'Alpha']);
  });

  it('the saved order is restored on the next start', async () => {
    await boot({ lastSortBy: 'expiry' });
    expect(st.currentSortBy).toBe('expiry');
    expect($('sort-label-text').textContent).toBe('Expiry');
  });

  it('a saved order that no longer exists is ignored', async () => {
    localStorage.setItem(
      'unenverse-settings',
      JSON.stringify({ onboardingCompleted: true, lastSortBy: 'bogus' }),
    );
    await boot();
    expect(st.currentSortBy).toBe('provider');
  });

  it('"Expand all" opens every card and a second press closes them', () => {
    $('expand-all-btn').click();
    expect(st.expanded.size).toBe(3);
    expect(document.querySelectorAll('#card-grid .card.expanded')).toHaveLength(3);
    $('expand-all-btn').click();
    expect(st.expanded.size).toBe(0);
    expect(document.querySelectorAll('#card-grid .card.expanded')).toHaveLength(0);
  });
});

describe('layout', { timeout: 30000 }, () => {
  it('the sidebar toggle collapses it, tells assistive tech, and is remembered', () => {
    $('sidebar-toggle').click();
    expect($('sidebar').classList.contains('collapsed')).toBe(true);
    expect($('sidebar-toggle').getAttribute('aria-expanded')).toBe('false');
    expect(Settings.get('sidebarCollapsed')).toBe(true);
    $('sidebar-toggle').click();
    expect($('sidebar').classList.contains('collapsed')).toBe(false);
    expect(Settings.get('sidebarCollapsed')).toBe(false);
  });

  it('dragging the resize handle changes the width within bounds and saves it once, on release', () => {
    const resizer = $('sidebar-resizer');
    vi.spyOn($('sidebar'), 'getBoundingClientRect').mockReturnValue({ width: 260 } as DOMRect);
    resizer.dispatchEvent(new MouseEvent('mousedown', { clientX: 100, bubbles: true }));
    expect(resizer.classList.contains('dragging')).toBe(true);
    document.dispatchEvent(new MouseEvent('mousemove', { clientX: 140 }));
    expect($('sidebar').style.width).toBe('300px');
    expect(resizer.getAttribute('aria-valuenow')).toBe('300');
    document.dispatchEvent(new MouseEvent('mousemove', { clientX: 900 }));
    expect($('sidebar').style.width).toBe('420px');
    document.dispatchEvent(new MouseEvent('mousemove', { clientX: -900 }));
    expect($('sidebar').style.width).toBe('140px');
    expect(Settings.get('sidebarWidth')).not.toBe(140);
    document.dispatchEvent(new MouseEvent('mouseup'));
    expect(resizer.classList.contains('dragging')).toBe(false);
    expect(Settings.get('sidebarWidth')).toBe(140);
    document.dispatchEvent(new MouseEvent('mousemove', { clientX: 200 }));
    expect($('sidebar').style.width).toBe('140px');
  });
});
