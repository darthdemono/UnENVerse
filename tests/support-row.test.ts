// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { loadRealIndexHtml } from './helpers';

let saveResult: { ok: true; path: string } | { ok: false; error: string } = {
  ok: true,
  path: '/tmp/x',
};
const saveFile = vi.fn(async (_c: string, _n: string, _m?: string) => saveResult);
vi.mock('../src/ts/utils', async (importOriginal) => {
  const real = await importOriginal<typeof import('../src/ts/utils')>();
  return { ...real, saveFile: (c: string, n: string, m?: string) => saveFile(c, n, m) };
});

// Settings, Support (R25): the bundle is built by the desktop and saved where the person says.
describe('support bundle settings row', () => {
  beforeEach(() => {
    loadRealIndexHtml();
    vi.resetModules();
    saveFile.mockClear();
    saveResult = { ok: true, path: '/tmp/x' };
    delete (window as unknown as { __TAURI__?: unknown }).__TAURI__;
  });
  const btn = () => document.getElementById('s-support-bundle') as HTMLButtonElement;
  const tick = () => new Promise((r) => setTimeout(r, 0));

  it('is hidden in a plain browser', async () => {
    const m = await import('../src/ts/settings-panel');
    m.openSettings();
    expect(btn().hidden).toBe(true);
  });

  it('saves exactly the text the desktop built, under a fixed name', async () => {
    const bundle = '{\n  "bundle_version": 1\n}';
    (window as unknown as { __TAURI__: unknown }).__TAURI__ = {
      core: { invoke: vi.fn(async () => bundle) },
    };
    const m = await import('../src/ts/settings-panel');
    m.openSettings();
    expect(btn().hidden).toBe(false);
    btn().click();
    await tick();
    await tick();
    expect(saveFile).toHaveBeenCalledWith(
      `${bundle}\n`,
      'unenverse-support-bundle.json',
      'application/json',
    );
  });

  it('saves nothing when the vault is locked', async () => {
    (window as unknown as { __TAURI__: unknown }).__TAURI__ = {
      core: {
        invoke: vi.fn(async () => {
          throw new Error('Vault is locked');
        }),
      },
    };
    const m = await import('../src/ts/settings-panel');
    m.openSettings();
    btn().click();
    await tick();
    await tick();
    expect(saveFile).not.toHaveBeenCalled();
  });

  it('says so when the file could not be saved', async () => {
    saveResult = { ok: false, error: 'disk full' };
    (window as unknown as { __TAURI__: unknown }).__TAURI__ = {
      core: { invoke: vi.fn(async () => '{}') },
    };
    const m = await import('../src/ts/settings-panel');
    m.openSettings();
    btn().click();
    await tick();
    await tick();
    expect(saveFile).toHaveBeenCalledTimes(1);
    expect(document.body.textContent).toContain('Not saved: disk full');
  });

  it('does nothing when the markup lacks the row', async () => {
    btn().remove();
    const m = await import('../src/ts/settings-panel');
    expect(() => m.openSettings()).not.toThrow();
  });
});
