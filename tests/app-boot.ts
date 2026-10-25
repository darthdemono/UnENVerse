// SPDX-License-Identifier: PolyForm-Strict-1.0.0
// Copyright Jubair Hasan.
/**
 * Boots the real application against a small vault, the way the shell tests
 * need it: `vault.ts` wires itself on import, so every boot gets a fresh module
 * graph and a fresh document.
 */
import { expect, vi } from 'vitest';
import { loadRealIndexHtml } from './helpers';

export const entry = (id: string, provider: string, o: object = {}) => ({
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

type Registered = [
  EventTarget,
  string,
  EventListenerOrEventListenerObject,
  boolean | AddEventListenerOptions | undefined,
];
let registered: Registered[] = [];
let restore: (() => void) | null = null;

/**
 * The app binds document- and window-level listeners when it boots. The document
 * outlives a test, so without this every boot would stack another copy of each
 * (a keyboard shortcut would fire once per earlier test).
 */
export function teardownApp(): void {
  for (const [target, type, fn, opts] of registered) target.removeEventListener(type, fn, opts);
  registered = [];
  restore?.();
  restore = null;
}

function trackListeners(): void {
  teardownApp();
  const targets: EventTarget[] = [document, window];
  const undo: (() => void)[] = [];
  targets.forEach((t) => {
    const own = Object.getOwnPropertyDescriptor(t, 'addEventListener');
    const original = t.addEventListener.bind(t);
    Object.defineProperty(t, 'addEventListener', {
      configurable: true,
      writable: true,
      value: (
        type: string,
        fn: EventListenerOrEventListenerObject,
        opts?: boolean | AddEventListenerOptions,
      ) => {
        registered.push([t, type, fn, opts]);
        original(type, fn, opts);
      },
    });
    // Put back exactly what was there: an own property if there was one (jsdom's window), else fall through to the prototype.
    undo.push(() => {
      if (own) Object.defineProperty(t, 'addEventListener', own);
      else delete (t as { addEventListener?: unknown }).addEventListener;
    });
  });
  restore = () => undo.forEach((fn) => fn());
}

export async function bootApp(
  entries: object[],
  opts: { settings?: object; projects?: object[]; categories?: string[] } = {},
) {
  trackListeners();
  vi.resetModules();
  loadRealIndexHtml();
  localStorage.clear();
  sessionStorage.clear();
  localStorage.setItem(
    'unenverse-settings',
    JSON.stringify({
      onboardingCompleted: true,
      groupPools: false,
      groupBundles: false,
      ...opts.settings,
    }),
  );
  delete (window as any).__TAURI__;
  sessionStorage.setItem(
    'unenverse',
    JSON.stringify({
      api_keys: entries,
      user_categories: opts.categories ?? ['infra'],
      projects: opts.projects ?? [{ id: 'Universal', name: 'Universal' }],
    }),
  );
  const state = await import('../src/ts/state');
  await import('../src/ts/vault');
  await vi.waitFor(() => expect(state.st.vaultOpen).toBe(true), { timeout: 15000 });
  await new Promise((r) => setTimeout(r, 100));
  return state;
}
