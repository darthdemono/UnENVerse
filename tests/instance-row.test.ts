// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { loadRealIndexHtml } from './helpers';

// Settings, Vault instance (R19): the standby notice and the activate button.
describe('vault instance settings row', () => {
  beforeEach(() => {
    loadRealIndexHtml();
    vi.resetModules();
    delete (window as unknown as { __TAURI__?: unknown }).__TAURI__;
  });

  const tauri = (invoke: (cmd: string, args?: unknown) => Promise<unknown>) => {
    (window as unknown as { __TAURI__: unknown }).__TAURI__ = { core: { invoke } };
  };
  const btn = () => document.getElementById('s-instance-activate') as HTMLButtonElement;
  const hint = () => document.getElementById('s-instance-hint')!;
  const called = (f: { mock: { calls: unknown[][] } }) =>
    f.mock.calls.some((c) => c[0] === 'vault_activate');
  const tick = () => new Promise((r) => setTimeout(r, 0));
  const id = (role: string, epoch: number) => ({ vault_id: 'v', instance_id: 'i', epoch, role });

  it('offers nothing in a plain browser', async () => {
    const m = await import('../src/ts/settings-panel');
    m.openSettings();
    expect(btn().hidden).toBe(true);
  });

  it('says a live copy is live and shows no button', async () => {
    tauri(vi.fn(async () => id('active', 3)));
    const m = await import('../src/ts/settings-panel');
    m.openSettings();
    await tick();
    expect(btn().hidden).toBe(true);
    expect(hint().textContent).toContain('Live (epoch 3)');
  });

  it('activates a standby copy only after the person confirms the old one is retired', async () => {
    const invoke = vi.fn(async (cmd: string) =>
      cmd === 'vault_activate' ? id('active', 5) : id('standby', 4),
    );
    tauri(invoke);
    const m = await import('../src/ts/settings-panel');
    m.openSettings();
    await tick();
    expect(btn().hidden).toBe(false);
    expect(hint().textContent).toContain('Standby');
    // Refuse first: nothing may be called.
    btn().click();
    await tick();
    document.getElementById('confirm-cancel')?.dispatchEvent(new Event('click'));
    await tick();
    expect(called(invoke)).toBe(false);
    btn().click();
    await tick();
    document.getElementById('confirm-ok')?.dispatchEvent(new Event('click'));
    await tick();
    expect(called(invoke)).toBe(true);
    expect(hint().textContent).toContain('Live (epoch 5)');
    expect(btn().hidden).toBe(true);
  });

  it('asks to unlock when the identity cannot be read', async () => {
    tauri(
      vi.fn(async () => {
        throw new Error('Vault is locked');
      }),
    );
    const m = await import('../src/ts/settings-panel');
    m.openSettings();
    await tick();
    expect(btn().hidden).toBe(true);
    expect(hint().textContent).toContain('Unlock the vault');
  });

  it('reports a refused activation and stays standby', async () => {
    const invoke = vi.fn(async (cmd: string) => {
      if (cmd === 'vault_activate') throw new Error('Vault is locked');
      return id('standby', 4);
    });
    tauri(invoke);
    const m = await import('../src/ts/settings-panel');
    m.openSettings();
    await tick();
    btn().click();
    await tick();
    document.getElementById('confirm-ok')?.dispatchEvent(new Event('click'));
    await tick();
    await tick();
    expect(called(invoke)).toBe(true);
    expect(btn().hidden).toBe(false);
    expect(document.body.textContent).toContain('Could not activate');
  });

  it('does nothing when the markup lacks the row', async () => {
    document.getElementById('s-instance-activate')?.remove();
    const m = await import('../src/ts/settings-panel');
    expect(() => m.openSettings()).not.toThrow();
  });
});
