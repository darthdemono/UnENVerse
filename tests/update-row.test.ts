// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { loadRealIndexHtml } from './helpers';

// Settings, Updates (R26): nothing is asked or installed until a button is pressed.
describe('desktop update settings row', () => {
  beforeEach(() => {
    loadRealIndexHtml();
    vi.resetModules();
    delete (window as unknown as { __TAURI__?: unknown }).__TAURI__;
  });
  const el = (id: string) => document.getElementById(id) as HTMLButtonElement;
  const hint = () => document.getElementById('s-update-hint')?.textContent ?? '';
  const tick = () => new Promise((r) => setTimeout(r, 0));
  const open = async (invoke: (cmd: string, args?: unknown) => Promise<unknown>) => {
    (window as unknown as { __TAURI__: unknown }).__TAURI__ = { core: { invoke } };
    const m = await import('../src/ts/settings-panel');
    m.openSettings();
  };

  it('is hidden in a plain browser', async () => {
    const m = await import('../src/ts/settings-panel');
    m.openSettings();
    expect(el('s-update-check').hidden).toBe(true);
    expect(el('s-update-install').hidden).toBe(true);
  });

  it('asks nothing when the pane opens', async () => {
    const asked: string[] = [];
    await open(async (cmd) => {
      asked.push(cmd);
      return null;
    });
    expect(el('s-update-check').hidden).toBe(false);
    expect(asked.filter((c) => c.startsWith('update_'))).toEqual([]);
  });

  it('shows the version and notes before it offers the install, then installs that version', async () => {
    const invoke = vi.fn(async (cmd: string) =>
      cmd === 'update_check' ? { version: '9.9.9', current: '0.83.0', notes: 'Fixes.' } : undefined,
    );
    await open(invoke);
    el('s-update-check').click();
    await tick();
    expect(hint()).toContain('9.9.9');
    expect(hint()).toContain('Fixes.');
    expect(el('s-update-install').hidden).toBe(false);
    expect(invoke).not.toHaveBeenCalledWith('update_install', expect.anything());
    el('s-update-install').click();
    await tick();
    await tick();
    expect(invoke).toHaveBeenCalledWith('update_install', { version: '9.9.9' });
    expect(hint()).toContain('Restart');
    expect(el('s-update-install').hidden).toBe(true);
  });

  it('says so when there is no update and offers no install', async () => {
    await open(async () => null);
    el('s-update-check').click();
    await tick();
    expect(hint()).toContain('newest');
    expect(el('s-update-install').hidden).toBe(true);
  });

  it('reports a failed check and a refused install', async () => {
    let checks = 0;
    const invoke = vi.fn(async (cmd: string) => {
      if (cmd === 'update_check') {
        if (++checks === 1) throw new Error('offline');
        return { version: '9.9.9', current: '0.83.0', notes: '' };
      }
      throw new Error('bad signature');
    });
    await open(invoke);
    el('s-update-check').click();
    await tick();
    expect(hint()).toContain('Could not check: Error: offline');
    el('s-update-check').click();
    await tick();
    el('s-update-install').click();
    await tick();
    await tick();
    expect(hint()).toContain('Not installed: Error: bad signature');
    expect(el('s-update-install').disabled).toBe(false);
  });
});
