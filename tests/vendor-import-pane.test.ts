// SPDX-License-Identifier: PolyForm-Strict-1.0.0
// Copyright Jubair Hasan.
/** R09 keeps source migration on the atomic local CLI path. */
import { describe, it, expect, vi } from 'vitest';
import { loadRealIndexHtml } from './helpers';

const $ = (id: string) => document.getElementById(id)!;
async function boot(tauri: boolean): Promise<void> {
  loadRealIndexHtml();
  vi.resetModules();
  delete (window as unknown as { __TAURI__?: unknown }).__TAURI__;
  if (tauri)
    (window as unknown as { __TAURI__: unknown }).__TAURI__ = { core: { invoke: vi.fn() } };
  (await import('../src/ts/tools-markup')).mountToolsPanes();
  (await import('../src/ts/vendor-import-pane')).initVendorImportPane();
}

describe('vendor import pane', () => {
  // The old renderer quick import only persisted api_keys. Before R09 it could
  // claim success while losing source identities and binary attachments.
  it('keeps the lossy desktop quick import disabled even in Tauri', async () => {
    await boot(true);
    expect(($('vi-vendor') as HTMLSelectElement).disabled).toBe(true);
    expect(($('vi-folders') as HTMLInputElement).disabled).toBe(true);
    expect(($('vi-file-btn') as HTMLButtonElement).disabled).toBe(true);
    expect(($('vi-apply-btn') as HTMLButtonElement).disabled).toBe(true);
    expect($('vi-status').textContent).toMatch(/CLI-only/);
    expect($('vi-status').textContent).toMatch(/sanitized real-export verification/);
  });

  it('gives browser viewers the same CLI direction', async () => {
    await boot(false);
    expect(($('vi-file-btn') as HTMLButtonElement).disabled).toBe(true);
    expect($('vi-status').textContent).toMatch(/unv import-vault/);
  });
});
