/**
 * Settings are plain JSON in browser storage, so anything may be in there: an
 * older build's blob, a hand edit, a key set to null. The app must still start,
 * show its screens and fall back to the documented default for each setting.
 */
import { describe, it, expect, afterAll } from 'vitest';
import { bootApp, entry, teardownApp } from './app-boot';

afterAll(() => teardownApp());

describe('a settings blob full of nulls', { timeout: 60000 }, () => {
  it('starts, draws the grid and the settings panel, and keeps the defaults', async () => {
    const { DEFAULT_SETTINGS } = await import('../src/ts/state');
    const nulls = Object.fromEntries(Object.keys(DEFAULT_SETTINGS).map((k) => [k, null]));
    const { st } = await bootApp(
      [entry('a', 'Alpha'), entry('b', 'Beta', { pool: 'p' }), entry('c', 'Gamma', { pool: 'p' })],
      { settings: nulls },
    );
    expect(st.vaultOpen).toBe(true);
    expect(
      document.querySelectorAll('#card-grid .card, #card-grid .pool-card-wrap').length,
    ).toBeGreaterThan(0);
    document.getElementById('settings-btn')!.click();
    expect(document.getElementById('settings-overlay')!.classList.contains('open')).toBe(true);
    for (const id of ['s-sidebar-sections', 's-panel-order']) {
      expect(document.getElementById(id)!.children.length).toBeGreaterThan(0);
    }
  });

  it('values of the wrong kind are dropped, the rest kept', async () => {
    const { st, Settings } = await bootApp([entry('a', 'Alpha')], {
      settings: {
        sidebarSections: 'all',
        cardSize: 7,
        accentColor: '#112233',
        collapsedSections: ['tags'],
      },
    });
    expect(st.vaultOpen).toBe(true);
    expect(Array.isArray(Settings.get('sidebarSections'))).toBe(true);
    expect(Settings.get('cardSize')).toBe('medium');
    expect(Settings.get('accentColor')).toBe('#112233');
    expect(Settings.get('collapsedSections')).toEqual(['tags']);
  });
});

describe('entries whose optional fields are null', { timeout: 60000 }, () => {
  it('draws every view without throwing', async () => {
    const nulls = {
      account_name: null,
      username: null,
      email: null,
      api_secret: null,
      key_id: null,
      label: null,
      api_url: null,
      callback_url: null,
      version: null,
      purpose: null,
      pool: null,
      expires_at: null,
      rotation_days: null,
      last_rotated_at: null,
      description: null,
      api_description: null,
      details: null,
      extra_vars: null,
      scopes: null,
      tags: null,
      categories: null,
      env_prefixes: null,
      environment: null,
      custom_icon: null,
      totp_secret: null,
      version_history: null,
      bundle_id: null,
    };
    const { st } = await bootApp([
      entry('a', 'Alpha', nulls),
      entry('b', 'Beta', { ...nulls, secretType: 'composite' }),
      entry('c', 'Gamma', { ...nulls, secretType: 'bundle' }),
    ]);
    expect(st.vaultOpen).toBe(true);
    expect(
      document.querySelectorAll('#card-grid .card, #card-grid .bundle-card-wrap').length,
    ).toBeGreaterThan(0);
    for (const id of ['health-scan-btn']) document.getElementById(id)?.click();
  });
});
