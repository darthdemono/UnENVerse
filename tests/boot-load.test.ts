// SPDX-License-Identifier: PolyForm-Strict-1.0.0
// Copyright Jubair Hasan.
/**
 * Opening a vault: what the app does with what it reads.
 *
 * A vault can be older than the code reading it (no ids, no Universal project,
 * a secret type that has since been removed) or damaged. The person must end up
 * with a usable vault and a truthful banner, and anything repaired must be
 * written back so the repair happens once.
 */
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { loadRealIndexHtml } from './helpers';

const toasts: [string, string?][] = [];
vi.mock('../src/ts/utils', async (importOriginal) => {
  const real = await importOriginal<typeof import('../src/ts/utils')>();
  return {
    ...real,
    showToast: (m: string, k?: string) => toasts.push([m, k]),
    showConfirm: async () => true,
  };
});

const flush = (ms = 100) => new Promise((r) => setTimeout(r, ms));

async function boot(stored: unknown) {
  vi.resetModules();
  loadRealIndexHtml();
  delete (window as any).__TAURI__;
  if (stored !== undefined)
    sessionStorage.setItem(
      'unenverse',
      typeof stored === 'string' ? stored : JSON.stringify(stored),
    );
  const { st } = await import('../src/ts/state');
  await import('../src/ts/vault');
  await vi.waitFor(() =>
    expect(document.documentElement.classList.contains('booting')).toBe(false),
  );
  await flush();
  return st;
}

beforeEach(() => {
  toasts.length = 0;
  document.documentElement.classList.add('booting');
});

describe('opening an older vault', { timeout: 30000 }, () => {
  it('gives every entry an id, the Universal project and a type, and writes the repair back once', async () => {
    const st = await boot({
      api_keys: [
        { provider: 'NoIds', api_key: 'k1' },
        { provider: 'NoUniversal', api_key: 'k2', projectIds: ['web'], secretType: 'password' },
        { provider: 'OldChunk', api_key: 'k3', projectIds: ['Universal'], secretType: 'chunk' },
      ],
      user_categories: ['infra'],
      projects: [{ id: 'web', name: 'Web' }],
    });
    expect(st.vault.api_keys).toHaveLength(3);
    const [a, b, c] = st.vault.api_keys;
    expect(a.projectIds).toEqual(['Universal']);
    expect(a.secretType).toBe('api_key');
    expect(b.projectIds).toEqual(['web', 'Universal']);
    expect(c.secretType).toBe('env_var');
    expect(new Set(st.vault.api_keys.map((e) => e.id)).size).toBe(3);
    expect(st.vault.api_keys.every((e) => e.id)).toBe(true);
    expect(st.vault.projects[0]).toMatchObject({ id: 'Universal', name: 'Universal' });
    expect(st.vault.projects.map((p) => p.id)).toContain('web');
    expect(st.vault.user_categories).toEqual(['infra']);
    expect(st.vaultOpen).toBe(true);
    expect(toasts).toContainEqual(['Loaded 3 keys', 'ok']);
    expect(document.getElementById('load-banner')!.style.display).toBe('none');
    const saved = JSON.parse(sessionStorage.getItem('unenverse')!);
    expect(saved.api_keys.every((e: any) => e.id && e.projectIds.includes('Universal'))).toBe(true);
    expect(saved.api_keys[2].secretType).toBe('env_var');
  });

  it('opens a vault blob that is missing whole sections', async () => {
    const st = await boot({
      api_keys: [
        {
          id: 'x',
          provider: 'Only',
          api_key: 'k',
          projectIds: ['Universal'],
          secretType: 'api_key',
        },
      ],
    });
    expect(st.vault.user_categories).toEqual([]);
    expect(st.vault.projects.map((p) => p.id)).toEqual(['Universal']);
    expect(st.vault.api_keys.map((e) => e.provider)).toEqual(['Only']);
    const empty = await boot({});
    expect(empty.vault.api_keys).toEqual([]);
    expect(empty.vaultOpen).toBe(true);
  });

  it('renders the vault it opened', async () => {
    await boot({
      api_keys: [
        {
          id: 'x',
          provider: 'ShownOnScreen',
          api_key: 'k',
          projectIds: ['Universal'],
          secretType: 'api_key',
        },
      ],
    });
    expect(document.body.textContent).toContain('ShownOnScreen');
  });
});

describe('opening nothing or something broken', () => {
  it('with no saved vault it shows the "load a vault" banner and an empty vault', async () => {
    const st = await boot(undefined);
    expect(document.getElementById('load-banner')!.style.display).toBe('flex');
    expect(st.vault.api_keys).toEqual([]);
    expect(st.vaultOpen).toBe(false);
  });

  it('unreadable saved data shows the banner instead of half a vault', async () => {
    const st = await boot('{ this is not json');
    expect(document.getElementById('load-banner')!.style.display).toBe('flex');
    expect(st.vault.api_keys).toEqual([]);
    expect(st.vaultOpen).toBe(false);
  });
});

describe('the first run', () => {
  const vault = {
    api_keys: [
      { id: 'x', provider: 'P', api_key: 'k', projectIds: ['Universal'], secretType: 'api_key' },
    ],
  };

  it('shows the welcome wizard until it has been completed, whatever the vault holds', async () => {
    await boot(vault);
    expect(document.querySelector('.onboard-backdrop')).not.toBeNull();
  });

  it('does not show it again once completed', async () => {
    localStorage.setItem('unenverse-settings', JSON.stringify({ onboardingCompleted: true }));
    await boot(vault);
    expect(document.querySelector('.onboard-backdrop')).toBeNull();
  });
});
