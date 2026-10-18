// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
/**
 * Boots the real application inside a faked desktop shell: a Tauri bridge that
 * answers named commands from `backend`, a lock screen that accepts any password,
 * and a vault served by `load_vault`. Each boot gets a fresh module graph.
 */
import { expect, vi } from 'vitest';
import { loadRealIndexHtml } from './helpers';

export const calls: [string, any][] = [];
export const backend: Record<string, (args: any) => unknown> = {};

export async function bootDesktop(
  vault: object,
  opts: { settings?: object; handlers?: Record<string, (a: any) => unknown> } = {},
) {
  vi.resetModules();
  loadRealIndexHtml();
  localStorage.clear();
  sessionStorage.clear();
  calls.length = 0;
  for (const k of Object.keys(backend)) delete backend[k];
  localStorage.setItem(
    'unenverse-settings',
    JSON.stringify({ onboardingCompleted: true, groupPools: false, ...opts.settings }),
  );
  Object.assign(backend, {
    vault_exists: () => true,
    get_vault_path: () => '/v/vault.db',
    unlock_vault: () => true,
    load_vault: () => ({ data: vault, version: 'v1' }),
    save_vault: () => 'v2',
    get_audit_log: () => [],
    ...opts.handlers,
  });
  (window as any).__TAURI__ = {
    core: {
      invoke: async (cmd: string, args: any) => {
        calls.push([cmd, args]);
        const fn = backend[cmd];
        if (!fn) throw new Error(`no ${cmd}`);
        return fn(args);
      },
    },
  };
  const state = await import('../src/ts/state');
  await import('../src/ts/vault');
  await vi.waitFor(
    () => expect(document.getElementById('unlock-overlay')!.classList.contains('open')).toBe(true),
    {
      timeout: 15000,
    },
  );
  (document.getElementById('unlock-password') as HTMLInputElement).value =
    'a long enough master password';
  document.getElementById('unlock-submit-btn')!.click();
  await vi.waitFor(() => expect(state.st.vaultOpen).toBe(true), { timeout: 15000 });
  await new Promise((r) => setTimeout(r, 80));
  return state;
}

export const closeDesktop = async () => {
  // The unlock starts the vault poll; left running it fires after jsdom is torn down ("document is not defined").
  (await import('../src/ts/vault-watch')).stopVaultWatch();
  delete (window as any).__TAURI__;
};
