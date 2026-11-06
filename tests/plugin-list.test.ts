// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { loadRealIndexHtml } from './helpers';

// Settings, Plugins (R14): a read-only list.
describe('plugin list', () => {
  beforeEach(() => {
    loadRealIndexHtml();
    vi.resetModules();
    delete (window as unknown as { __TAURI__?: unknown }).__TAURI__;
  });
  const tauri = (invoke: (cmd: string, args?: unknown) => Promise<unknown>) => {
    (window as unknown as { __TAURI__: unknown }).__TAURI__ = { core: { invoke } };
  };
  const tick = () => new Promise((r) => setTimeout(r, 0));
  const items = () =>
    Array.from(document.querySelectorAll('#s-plugin-list li')).map((li) => li.textContent);

  it('says it is a desktop feature in a plain browser', async () => {
    const m = await import('../src/ts/settings-panel');
    m.openSettings();
    expect(items()).toEqual(['Shown in the desktop app only.']);
  });

  it('lists installed, available and problem rows as plain text', async () => {
    tauri(
      vi.fn(async () => ({
        installed: [
          {
            name: 'ntfy',
            version: '1.0.0',
            hash: 'abcdef0123456789',
            capabilities: ['notify'],
            enabled: false,
          },
        ],
        available: [{ name: '<img src=x onerror=alert(1)>', version: '0.1.0' }],
        problems: ['plugins/bad: no manifest'],
      })),
    );
    const m = await import('../src/ts/settings-panel');
    m.openSettings();
    await tick();
    expect(items()).toEqual([
      'ntfy 1.0.0 [notify] disabled sha256:abcdef012345',
      '<img src=x onerror=alert(1)> 0.1.0 (not installed)',
      'Problem: plugins/bad: no manifest',
    ]);
    // Vault data is untrusted: the name must have stayed text, not become an element.
    expect(document.querySelector('#s-plugin-list img')).toBeNull();
  });

  it('says so when there are none, and when the vault is locked', async () => {
    tauri(vi.fn(async () => ({ installed: [], available: [], problems: [] })));
    let m = await import('../src/ts/settings-panel');
    m.openSettings();
    await tick();
    expect(items()).toEqual(['No plugins found.']);
    vi.resetModules();
    tauri(
      vi.fn(async () => {
        throw new Error('Vault is locked');
      }),
    );
    m = await import('../src/ts/settings-panel');
    m.openSettings();
    await tick();
    expect(items()).toEqual(['Unlock the vault to see the plugins.']);
  });
});
