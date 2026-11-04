// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { loadRealIndexHtml } from './helpers';

// The Settings "Allow local scripts" switch (R18). Same fixture pattern as catalogue-row:
// the IPC branch is reached by installing a bridge before a fresh import.
describe('allow local scripts settings row', () => {
  beforeEach(() => {
    loadRealIndexHtml();
    vi.resetModules();
    delete (window as unknown as { __TAURI__?: unknown }).__TAURI__;
  });

  const tauri = (invoke: (cmd: string, args?: unknown) => Promise<unknown>) => {
    (window as unknown as { __TAURI__: unknown }).__TAURI__ = { core: { invoke } };
  };
  const box = () => document.getElementById('s-bridge') as HTMLInputElement;

  it('is off and disabled in a plain browser, where there is no desktop to serve', async () => {
    const m = await import('../src/ts/settings-panel');
    m.openSettings();
    expect(box().disabled).toBe(true);
    expect(box().checked).toBe(false);
  });

  it('shows the saved state and switches it through the desktop', async () => {
    const invoke = vi.fn(async (cmd: string, args?: unknown) => {
      if (cmd === 'bridge_status') return { enabled: true };
      if (cmd === 'bridge_set_enabled') return { enabled: (args as { enabled: boolean }).enabled };
      return null;
    });
    tauri(invoke);
    const m = await import('../src/ts/settings-panel');
    m.openSettings();
    await new Promise((r) => setTimeout(r, 0));
    expect(box().checked).toBe(true);
    box().checked = false;
    box().onchange?.(new Event('change'));
    await new Promise((r) => setTimeout(r, 0));
    expect(invoke).toHaveBeenCalledWith('bridge_set_enabled', { enabled: false });
    expect(box().checked).toBe(false);
  });

  it('puts the switch back when the desktop refuses', async () => {
    const invoke = vi.fn(async (cmd: string) => {
      if (cmd === 'bridge_status') return { enabled: false };
      throw new Error('socket busy');
    });
    tauri(invoke);
    const m = await import('../src/ts/settings-panel');
    m.openSettings();
    await new Promise((r) => setTimeout(r, 0));
    box().checked = true;
    box().onchange?.(new Event('change'));
    await new Promise((r) => setTimeout(r, 0));
    expect(box().checked).toBe(false);
  });
});
