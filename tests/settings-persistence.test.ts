// SPDX-License-Identifier: PolyForm-Strict-1.0.0
// Copyright Jubair Hasan.
/**
 * Settings: what is remembered between launches, what is migrated for older
 * installs, and how a stored setting becomes what is on screen.
 *
 * Settings live in a plain JSON blob the person (or an older build) may have
 * edited, so a stored value is never trusted to be sensible: a sidebar width of
 * 3px must not strand the sidebar, and a section key from a removed feature must
 * not come back.
 */
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { loadRealIndexHtml } from './helpers';

let state: typeof import('../src/ts/state');

async function fresh(stored?: object, flags: string[] = []) {
  vi.resetModules();
  loadRealIndexHtml();
  localStorage.clear();
  if (stored) localStorage.setItem('unenverse-settings', JSON.stringify(stored));
  for (const f of flags) localStorage.setItem(f, '1');
  state = await import('../src/ts/state');
  return state;
}

beforeEach(() => {
  vi.stubGlobal('fetch', vi.fn().mockRejectedValue(new Error('no settings.json')));
});
afterEach(() => {
  vi.unstubAllGlobals();
});

describe('loading', () => {
  it('starts from the defaults and overlays what was saved', async () => {
    const { Settings } = await fresh({ theme: 'light', autoLockMinutes: 5 });
    await Settings.init();
    expect(Settings.get('theme')).toBe('light');
    expect(Settings.get('autoLockMinutes')).toBe(5);
    expect(Settings.get('maskKeysByDefault')).toBe(true);
    expect(Settings.getAll().expiryWarningDays).toBe(30);
  });

  it('a settings file shipped with the app is overridden by what the person saved', async () => {
    vi.stubGlobal(
      'fetch',
      vi
        .fn()
        .mockResolvedValue({ ok: true, json: async () => ({ theme: 'light', cardSize: 'large' }) }),
    );
    const { Settings } = await fresh({ theme: 'dark' });
    await Settings.init();
    expect(Settings.get('theme')).toBe('dark');
    expect(Settings.get('cardSize')).toBe('large');
  });

  it('an unreadable saved blob leaves the defaults standing', async () => {
    const { Settings } = await fresh();
    localStorage.setItem('unenverse-settings', '{ not json');
    await Settings.init();
    expect(Settings.get('theme')).toBe('dark');
  });

  it('saving writes the whole blob back', async () => {
    const { Settings } = await fresh();
    Settings.set('accentColor', '#112233');
    Settings.setAll({ cardSize: 'compact', theme: 'light' });
    expect(JSON.parse(localStorage.getItem('unenverse-settings')!)).toMatchObject({
      accentColor: '#112233',
      cardSize: 'compact',
      theme: 'light',
    });
  });
});

describe('migrating an older install', () => {
  it('adds the sections introduced since (env after price, then tags, pools, prefixes) once, keeping later removals', async () => {
    const { Settings } = await fresh({ sidebarSections: ['all', 'price', 'category'] });
    await Settings.init();
    expect(Settings.get('sidebarSections')).toEqual([
      'all',
      'price',
      'env',
      'category',
      'tags',
      'pools',
      'prefixes',
    ]);
    Settings.set('sidebarSections', ['all', 'price', 'category']);
    await Settings.init();
    expect(Settings.get('sidebarSections')).toEqual(['all', 'price', 'category']);
  });

  it('puts the new section last when its anchor is missing, and never duplicates', async () => {
    const { Settings } = await fresh({ sidebarSections: ['all', 'env', 'tags'] });
    await Settings.init();
    expect(Settings.get('sidebarSections')).toEqual(['all', 'env', 'tags', 'pools', 'prefixes']);
  });

  it('adds the Authenticator panel to a stored panel order exactly once', async () => {
    const { Settings } = await fresh({ panelOrder: ['secrets', 'tools'] });
    await Settings.init();
    expect(Settings.get('panelOrder')).toEqual(['secrets', 'tools', 'auth']);
    Settings.set('panelOrder', ['secrets']);
    await Settings.init();
    expect(Settings.get('panelOrder')).toEqual(['secrets']);
  });

  it('a section from a removed feature is dropped on every load, even when saved again', async () => {
    const { Settings } = await fresh({ sidebarSections: ['all', 'authenticator', 'price'] }, [
      'unenverse-sb-migrated',
      'unenverse-panel-migrated-auth',
    ]);
    await Settings.init();
    expect(Settings.get('sidebarSections')).toEqual(['all', 'price']);
    Settings.set('sidebarSections', ['authenticator', 'all'] as never);
    await Settings.init();
    expect(Settings.get('sidebarSections')).toEqual(['all']);
  });
});

describe('applying what is stored', () => {
  it('sets the theme, the accent and its tints, and the custom style', async () => {
    const { Settings } = await fresh(
      { theme: 'light', accentColor: '#ff0000', customCss: 'body{color:red}' },
      ['unenverse-sb-migrated', 'unenverse-panel-migrated-auth'],
    );
    // The style element lives in <head>, which the shared HTML fixture does not carry.
    document.head.insertAdjacentHTML('beforeend', '<style id="custom-style"></style>');
    await Settings.init();
    const root = document.documentElement;
    expect(root.getAttribute('data-theme')).toBe('light');
    expect(root.style.getPropertyValue('--accent')).toBe('#ff0000');
    expect(root.style.getPropertyValue('--accent-dim')).toContain('rgba(255,0,0,0.14)');
    expect(root.style.getPropertyValue('--accent-mid')).toContain('rgba(255,0,0,0.3)');
    expect(document.getElementById('custom-style')!.textContent).toBe('body{color:red}');
  });

  it('"system" follows the operating system, and follows it again when it changes', async () => {
    let dark = false;
    const listeners: (() => void)[] = [];
    window.matchMedia = ((q: string) => ({
      matches: dark,
      media: q,
      addEventListener: (_: string, fn: () => void) => listeners.push(fn),
      removeEventListener() {},
      addListener() {},
      removeListener() {},
      dispatchEvent: () => false,
      onchange: null,
    })) as never;
    const { Settings } = await fresh({ theme: 'system' });
    await Settings.init();
    expect(document.documentElement.getAttribute('data-theme')).toBe('light');
    dark = true;
    listeners.forEach((fn) => fn());
    expect(document.documentElement.getAttribute('data-theme')).toBe('dark');
    Settings.set('theme', 'light');
    dark = false;
    listeners.forEach((fn) => fn());
    expect(document.documentElement.getAttribute('data-theme')).toBe('dark');
  });

  it('card size and column count shape the grid; a fixed count never forces horizontal scrolling', async () => {
    const { Settings, applyGridSettings } = await fresh();
    const grid = document.getElementById('card-grid')!;
    Settings.setAll({ cardSize: 'compact', gridColumns: 'auto' });
    applyGridSettings();
    expect(grid.dataset.cardSize).toBe('compact');
    expect(grid.style.gridTemplateColumns).toContain('minmax(min(100%, 280px), 1fr)');
    Settings.setAll({ cardSize: 'large', gridColumns: 4 as never });
    applyGridSettings();
    expect(grid.style.gridTemplateColumns).toBe('repeat(4, minmax(0, 1fr))');
    document.getElementById('card-grid')!.remove();
    expect(() => applyGridSettings()).not.toThrow();
  });

  it('the sidebar order and visibility follow the saved sections; collapsed ones are marked', async () => {
    const { Settings, applySidebarOrder, isSidebarSectionEnabled } = await fresh();
    Settings.setAll({ sidebarSections: ['price', 'all'], collapsedSections: ['all'] });
    applySidebarOrder();
    const el = (k: string) => document.getElementById(`sidebar-section-${k}`)!;
    expect(el('price').style.order).toBe('0');
    expect(el('all').style.order).toBe('1');
    expect(el('all').style.borderTop).toContain('solid');
    expect(el('price').style.borderTop).toBe('');
    expect(el('env').style.display).toBe('none');
    expect(el('all').classList.contains('collapsed')).toBe(true);
    expect(el('price').classList.contains('collapsed')).toBe(false);
    expect(isSidebarSectionEnabled('price')).toBe(true);
    expect(isSidebarSectionEnabled('env')).toBe(false);
  });

  it('a stored sidebar width is kept within sensible bounds, and the collapse state reaches the toggle', async () => {
    const { Settings, applySidebarLayout, SIDEBAR_MIN_W, SIDEBAR_MAX_W } = await fresh();
    const sidebar = document.getElementById('sidebar')!;
    for (const [stored, shown] of [
      [3, `${SIDEBAR_MIN_W}px`],
      [9999, `${SIDEBAR_MAX_W}px`],
      [250, '250px'],
      [0, ''],
      ['junk', ''],
    ] as const) {
      Settings.set('sidebarWidth', stored as never);
      applySidebarLayout();
      expect(sidebar.style.width, String(stored)).toBe(shown);
    }
    Settings.set('sidebarCollapsed', true);
    applySidebarLayout();
    expect(sidebar.classList.contains('collapsed')).toBe(true);
    expect(document.getElementById('sidebar-toggle')!.getAttribute('aria-expanded')).toBe('false');
    expect(document.getElementById('sidebar-resizer')!.getAttribute('aria-valuenow')).toBe(
      String(SIDEBAR_MIN_W),
    );
    document.getElementById('sidebar')!.remove();
    expect(() => applySidebarLayout()).not.toThrow();
  });
});

describe('recent searches', () => {
  it('keeps the newest first, drops case-insensitive duplicates (newest casing wins), ignores blanks and caps the list', async () => {
    const { Settings, pushRecentSearch, RECENT_SEARCH_MAX } = await fresh();
    pushRecentSearch('github');
    pushRecentSearch('  ');
    pushRecentSearch('aws');
    pushRecentSearch('GitHub');
    expect(Settings.get('recentSearches')).toEqual(['GitHub', 'aws']);
    for (let i = 0; i < RECENT_SEARCH_MAX + 3; i++) pushRecentSearch(`q${i}`);
    expect(Settings.get('recentSearches')).toHaveLength(RECENT_SEARCH_MAX);
    expect(Settings.get('recentSearches')![0]).toBe(`q${RECENT_SEARCH_MAX + 2}`);
    Settings.set('recentSearches', undefined as never);
    pushRecentSearch('first');
    expect(Settings.get('recentSearches')).toEqual(['first']);
  });
});

describe('locking when the window is hidden', () => {
  const hide = () => {
    Object.defineProperty(document, 'visibilityState', { value: 'hidden', configurable: true });
    document.dispatchEvent(new Event('visibilitychange'));
  };
  afterEach(() => {
    Object.defineProperty(document, 'visibilityState', { value: 'visible', configurable: true });
  });

  it('only when the person asked for it, the vault is open and local, and no one is being served', async () => {
    const { Settings, st } = await fresh();
    const lock = await import('../src/ts/lock');
    const spy = vi.spyOn(lock, 'lockVault').mockResolvedValue(undefined);
    st.vaultOpen = true;
    Settings.set('lockOnHide', false);
    hide();
    await Promise.resolve();
    await new Promise((r) => setTimeout(r, 20));
    expect(spy).not.toHaveBeenCalled();
    Settings.set('lockOnHide', true);
    st.lanServerRunning = true;
    hide();
    await new Promise((r) => setTimeout(r, 20));
    expect(spy).not.toHaveBeenCalled();
    st.lanServerRunning = false;
    st.vaultOpen = false;
    hide();
    await new Promise((r) => setTimeout(r, 20));
    expect(spy).not.toHaveBeenCalled();
    st.vaultOpen = true;
    (st.store as { isRemote: boolean }) = { isRemote: true } as never;
    hide();
    await new Promise((r) => setTimeout(r, 20));
    expect(spy).not.toHaveBeenCalled();
    st.store = new state.LocalVaultStore();
    hide();
    await vi.waitFor(() => expect(spy).toHaveBeenCalledWith('visibility'));
    spy.mockClear();
    Object.defineProperty(document, 'visibilityState', { value: 'visible', configurable: true });
    document.dispatchEvent(new Event('visibilitychange'));
    await new Promise((r) => setTimeout(r, 20));
    expect(spy).not.toHaveBeenCalled();
  });
});
