// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { loadRealIndexHtml } from './helpers';

// Settings, Nodes, Automation key (R12).
describe('automation key settings row', () => {
  beforeEach(() => {
    loadRealIndexHtml();
    vi.resetModules();
    delete (window as unknown as { __TAURI__?: unknown }).__TAURI__;
  });
  const tauri = (invoke: (cmd: string) => Promise<unknown>) => {
    (window as unknown as { __TAURI__: unknown }).__TAURI__ = { core: { invoke } };
  };
  const btn = () => document.getElementById('s-autokey-show') as HTMLButtonElement;
  const row = () => document.getElementById('s-autokey-row')!;
  const tick = () => new Promise((r) => setTimeout(r, 0));

  it('offers nothing in a plain browser', async () => {
    const m = await import('../src/ts/settings-panel');
    m.openSettings();
    expect(btn().hidden).toBe(true);
    expect(row().hidden).toBe(true);
  });

  it('shows the public key and its fingerprint on request, and never anything else', async () => {
    const invoke = vi.fn(async () => ({
      public_key: 'ab'.repeat(32),
      fingerprint: 'cd'.repeat(32),
    }));
    tauri(invoke);
    const m = await import('../src/ts/settings-panel');
    m.openSettings();
    expect(btn().hidden).toBe(false);
    expect(row().hidden).toBe(true);
    btn().click();
    await tick();
    expect((document.getElementById('s-autokey-pub') as HTMLInputElement).value).toBe(
      'ab'.repeat(32),
    );
    expect(document.getElementById('s-autokey-fp')!.textContent).toContain('cd'.repeat(32));
    expect(row().hidden).toBe(false);
    expect(invoke).toHaveBeenCalledWith('hub_automation_key', undefined);
  });

  it('says so when the vault is locked and shows no key', async () => {
    tauri(async () => {
      throw new Error('Vault is locked');
    });
    const m = await import('../src/ts/settings-panel');
    m.openSettings();
    btn().click();
    await tick();
    await tick();
    expect(row().hidden).toBe(true);
    expect(document.body.textContent).toContain('Could not read the key');
  });

  it('does nothing when the markup lacks the row', async () => {
    document.getElementById('s-autokey-row')?.remove();
    const m = await import('../src/ts/settings-panel');
    expect(() => m.openSettings()).not.toThrow();
  });
});
