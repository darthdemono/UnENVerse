// SPDX-License-Identifier: PolyForm-Strict-1.0.0
// Copyright Jubair Hasan.
/**
 * State helpers at their edges: restoring a remembered view against a vault
 * that has changed, spending a recovery code that is gone, the .env / YAML / JSON
 * exports of every kind of value, the panels, and name disambiguation.
 */
import { describe, it, expect, beforeEach } from 'vitest';
import {
  st,
  Settings,
  Exporter,
  restoreViewState,
  spendRecoveryCodeLive,
  disambiguateNames,
  namesGeneratedBy,
  switchPanel,
  applyUsersPanelVisibility,
  applySidebarOrder,
  isSidebarSectionEnabled,
  applyActivityBar,
} from '../src/ts/state';
import { loadRealIndexHtml, makeEntry, makeProject, makeVault, resetState } from './helpers';

beforeEach(() => {
  loadRealIndexHtml();
  resetState(st);
  Settings.set('rememberFilters', true);
  st.activeTypeChips = new Set();
  st.activeTagFilter = null;
  st.activePrefixFilter = null;
  st.activePoolFilter = null;
  st.currentEnvFilter = '';
  st.vault = makeVault({
    api_keys: [
      makeEntry({
        id: 'a',
        provider: 'Alpha',
        price_type: 'paid',
        environment: 'production',
        tags: ['prod'],
        pool: 'ring',
        secretType: 'password',
      } as any),
      makeEntry({ id: 'b', provider: 'Beta', price_type: 'free', tags: undefined } as any),
    ],
    user_categories: ['infra', 'infra/db'],
    projects: [
      makeProject({ id: 'Universal', name: 'Universal' }),
      makeProject({ id: 'web', name: 'Web' }),
    ],
  });
});

const remember = (v: object) =>
  Settings.set('lastView', { filterType: 'all', filterValue: '', ...v } as never);

describe('restoring a remembered view', () => {
  it('does nothing when the setting is off or nothing was remembered', () => {
    Settings.set('rememberFilters', false);
    remember({ envFilter: 'production' });
    expect(restoreViewState()).toBe(false);
    Settings.set('rememberFilters', true);
    Settings.set('lastView', null as never);
    expect(restoreViewState()).toBe(false);
  });

  it('brings back every filter that still matches something in the vault', () => {
    remember({
      filterType: 'price',
      filterValue: 'paid',
      envFilter: 'production',
      tagFilter: 'prod',
      prefixFilter: 'Alp',
      poolFilter: 'ring',
      typeChips: ['password', '__totp'],
      projectIds: ['web'],
    });
    expect(restoreViewState()).toBe(true);
    expect(st.filter).toEqual({ type: 'price', value: 'paid' });
    expect(st.currentEnvFilter).toBe('production');
    expect(st.activeTagFilter).toBe('prod');
    expect(st.activePrefixFilter).toBe('Alp');
    expect(st.activePoolFilter).toBe('ring');
    expect([...st.activeTypeChips].sort()).toEqual(['__totp', 'password']);
    expect(st.currentSelectedProjectIds).toEqual(['web']);
  });

  it('drops every filter that no longer matches, so the app never opens to an empty grid', () => {
    remember({
      filterType: 'price',
      filterValue: 'conditional',
      envFilter: 'staging',
      tagFilter: 'gone',
      prefixFilter: 'Zzz',
      poolFilter: 'gone',
      typeChips: ['__pool', 'cookie'],
      projectIds: ['ghost'],
    });
    expect(restoreViewState()).toBe(false);
    expect(st.activeTypeChips.size).toBe(0);
    expect(st.activeTagFilter).toBeNull();
    expect(st.activePrefixFilter).toBeNull();
    expect(st.currentSelectedProjectIds).toEqual(['Universal']);
  });

  it('a category filter survives when only a child of it exists, and a bundle selection survives only for real bundles', () => {
    remember({ filterType: 'category', filterValue: 'infra' });
    expect(restoreViewState()).toBe(true);
    expect(st.filter).toEqual({ type: 'category', value: 'infra' });
    remember({ filterType: 'category', filterValue: 'nope' });
    restoreViewState();
    st.vault.api_keys.push(makeEntry({ id: 'bun', provider: 'B', secretType: 'bundle' } as any));
    st.vault.api_keys.push(makeEntry({ id: 'm', provider: 'M', bundle_id: 'bun' } as any));
    remember({
      expandedBundleIds: ['bun', 'ghost'],
      bundleSlotTabs: { bun: 'm', ghost: 'm', bun2: 'zzz' },
    });
    expect(restoreViewState()).toBe(true);
    expect([...st.expandedBundles]).toEqual(['bun']);
    expect(st.bundleSlotTabs).toEqual({ bun: 'm' });
    remember({ bundleSlotTabs: [] as never, expandedBundleIds: 'x' as never });
    restoreViewState();
    expect(st.bundleSlotTabs).toEqual({});
  });
});

describe('spending a recovery code', () => {
  it('names the problem when the entry is gone, has no codes, or has none left', () => {
    st.vault.api_keys.push(
      makeEntry({
        id: 'rc',
        provider: 'RC',
        secretType: 'recovery_codes',
        extra_vars: [{ key: 'codes', value: 'one\ttwo' }],
      } as any),
    );
    expect(spendRecoveryCodeLive('ghost', '2026-01-01')).toEqual({
      error: 'That entry changed or was deleted while you were deciding.',
    });
    st.vault.api_keys.push(
      makeEntry({
        id: 'rc2',
        provider: 'RC2',
        secretType: 'recovery_codes',
        extra_vars: [],
      } as any),
    );
    expect(spendRecoveryCodeLive('rc2', '2026-01-01')).toEqual({
      error: 'That entry has no recovery codes.',
    });
    st.vault.api_keys.find((e) => e.id === 'rc')!.extra_vars = [
      { key: 'codes', value: 'a\tUSED 2026-01-01' },
    ];
    expect(spendRecoveryCodeLive('rc', '2026-01-02')).toEqual({ error: 'no unused codes remain' });
  });

  it('marks the next unused code and returns the entry', () => {
    st.vault.api_keys.push(
      makeEntry({
        id: 'rc',
        provider: 'RC',
        secretType: 'recovery_codes',
        extra_vars: [{ key: 'codes', value: 'one\ntwo' }],
      } as any),
    );
    const r = spendRecoveryCodeLive('rc', '2026-01-02') as {
      entry: { extra_vars: { value: string }[] };
    };
    expect(r.entry.extra_vars[0].value).toBe('one\tUSED 2026-01-02\ntwo');
  });
});

describe('the exports', () => {
  const rich = makeEntry({
    id: 'r',
    provider: 'Rich',
    api_key: 'k 1',
    api_secret: 's"2',
    api_url: 'https://a.test',
    account_name: 'ops',
    extra_vars: [
      { key: 'region', value: 'eu' },
      { key: '', value: 'skipped' },
      { key: 'blank', value: undefined as never },
    ],
  } as any);
  const bare = makeEntry({ id: 'n', provider: 'Bare', api_key: '' } as any);

  it('.env: one commented block per entry, with every value quoted and empty ones left out', () => {
    const out = Exporter.dotenv([rich, bare]);
    expect(out).toContain('# Rich — ops');
    expect(out).toContain('RICH="k 1"');
    expect(out).toContain('RICH_SECRET="s\\"2"');
    expect(out).toContain('RICH_URL=https://a.test');
    expect(out).toContain('RICH_REGION=eu');
    expect(out).toContain('RICH_BLANK=');
    expect(out).not.toContain('skipped');
    expect(out.endsWith('# Bare')).toBe(true);
  });

  it('YAML and JSON carry the same values', () => {
    const y = Exporter.yaml([rich, bare]);
    expect(y).toContain('RICH_SECRET:');
    expect(y).toContain('RICH_REGION: eu');
    expect(y).not.toContain('skipped');
    expect(y).not.toContain('BARE');
    expect(JSON.parse(Exporter.json([rich])).api_keys[0].provider).toBe('Rich');
  });
});

describe('names that collide', () => {
  it('the first keeps its name and the rest get free ordinals that skip names already in use', () => {
    const e = (id: string, provider: string) =>
      makeEntry({ id, provider, api_key: 'v-' + id } as any);
    const names = disambiguateNames([
      ...namesGeneratedBy(e('1', 'Same')),
      ...namesGeneratedBy(e('2', 'Same')),
      ...namesGeneratedBy(e('3', 'Same')),
      ...namesGeneratedBy(e('4', 'Same_2')),
    ]);
    expect(names).toEqual(['SAME', 'SAME_3', 'SAME_4', 'SAME_2']);
  });
});

describe('panels and sidebar', () => {
  it('an unknown panel shows the secrets, and the users tab hides when there is nothing to serve', () => {
    switchPanel('nonsense' as never);
    expect(document.getElementById('secrets-panel')!.style.display).toBe('');
    expect(document.getElementById('tools-panel')!.style.display).toBe('none');
    st.lanServerRunning = false;
    st.store = { isRemote: false } as never;
    Settings.set('activePanel', 'users');
    applyUsersPanelVisibility();
    expect(Settings.get('activePanel')).toBe('secrets');
    document.querySelector<HTMLElement>('.activity-btn[data-panel="users"]')!.remove();
    expect(() => applyUsersPanelVisibility()).not.toThrow();
  });

  it('sections that are not in the list are hidden, the rest ordered, and a missing section element is skipped', () => {
    Settings.set('sidebarSections', ['all', 'env'] as never);
    document.getElementById('sidebar-section-tags')!.remove();
    applySidebarOrder();
    expect(document.getElementById('sidebar-section-price')!.style.display).toBe('none');
    expect(document.getElementById('sidebar-section-env')!.style.order).toBe('1');
    expect(isSidebarSectionEnabled('env')).toBe(true);
    expect(isSidebarSectionEnabled('price')).toBe(false);
  });

  it('the activity bar can move right and show labels', () => {
    Settings.set('activityBarPosition', 'right');
    Settings.set('activityBarStyle', 'icon-label');
    applyActivityBar();
    const layout = document.getElementById('layout')!;
    expect(layout.classList.contains('activity-bar-right')).toBe(true);
    expect(layout.classList.contains('activity-bar-icon-label')).toBe(true);
  });
});
