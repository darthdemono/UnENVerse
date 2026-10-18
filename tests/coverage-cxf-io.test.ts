// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
/** Branch coverage for cxf-io.ts: the import and export flows around the IPC bridge. */
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { makeEntry, makeVault, resetState } from './helpers';

const toast = vi.fn();
let confirmAnswer = true;
const confirms: string[] = [];
let saveResult: any = { ok: true, path: '/x/f.json' };
const saved: unknown[] = [];
vi.mock('../src/ts/utils', async (importOriginal) => {
  const real = await importOriginal<typeof import('../src/ts/utils')>();
  return {
    ...real,
    showToast: (...a: unknown[]) => toast(...a),
    showConfirm: async (m: string) => {
      confirms.push(m);
      return confirmAnswer;
    },
    saveFile: async (...a: unknown[]) => {
      saved.push(a);
      return saveResult;
    },
  };
});

async function boot(
  invoke?: (cmd: string, args: any) => Promise<unknown>,
  entries = [makeEntry({ id: 'a', provider: 'A' })],
) {
  vi.resetModules();
  if (invoke) (window as any).__TAURI__ = { core: { invoke } };
  else delete (window as any).__TAURI__;
  const state = await import('../src/ts/state');
  resetState(state.st);
  state.st.vault = makeVault({ api_keys: entries });
  state.st.store = { save: async () => {}, isRemote: false } as any;
  return { st: state.st, mod: await import('../src/ts/cxf-io') };
}

beforeEach(() => {
  toast.mockClear();
  confirms.length = 0;
  saved.length = 0;
  confirmAnswer = true;
  saveResult = { ok: true, path: '/x/f.json' };
  delete (window as any).__TAURI__;
});

describe('CXF import', () => {
  it('needs the desktop app', async () => {
    const { mod } = await boot();
    await mod.runCxfImport('{}');
    expect(toast).toHaveBeenCalledWith(
      expect.stringContaining('needs the desktop app'),
      'err',
      3500,
    );
  });

  it('parses to an empty list when the bridge returns nothing', async () => {
    const { mod } = await boot(async () => undefined);
    expect(await mod.parseCxfFile('{}')).toEqual([]);
    await mod.runCxfImport('{}');
    expect(toast).toHaveBeenCalledWith('No items found in that file', 'err');
  });

  it('reports a parse failure, as an Error or as a string', async () => {
    let thrown: unknown = new Error('bad');
    const { mod } = await boot(async () => {
      throw thrown;
    });
    await mod.runCxfImport('{}');
    expect(toast).toHaveBeenCalledWith('bad', 'err', 6000);
    thrown = 'text';
    await mod.runCxfImport('{}');
    expect(toast).toHaveBeenCalledWith('text', 'err', 6000);
    thrown = null;
    await mod.runCxfImport('{}');
  });

  it('lists up to eight items, singular and plural, and appends only when confirmed', async () => {
    const many = Array.from({ length: 10 }, (_, i) =>
      makeEntry({
        id: `n${i}`,
        provider: `P${i}`,
        secretType: i === 0 ? 'password' : undefined,
      } as any),
    );
    const { mod, st } = await boot(async () => many);
    confirmAnswer = false;
    await mod.runCxfImport('{}');
    expect(confirms[0]).toContain('Import 10 items?');
    expect(confirms[0]).toContain('… and 2 more');
    expect(confirms[0]).toContain('P0 (password)');
    expect(st.vault.api_keys).toHaveLength(1);
    confirmAnswer = true;
    await mod.runCxfImport('{}');
    expect(st.vault.api_keys).toHaveLength(11);
    expect(toast).toHaveBeenCalledWith('Imported 10 items ✓', 'ok', 3500);

    const one = await boot(async () => [makeEntry({ id: 'solo', provider: 'Solo' })]);
    await one.mod.runCxfImport('{}');
    expect(toast).toHaveBeenCalledWith('Imported 1 item ✓', 'ok', 3500);
  });
});

describe('CXF export', () => {
  it('refuses an empty vault and a declined confirmation', async () => {
    const empty = await boot(async () => 'doc', []);
    await empty.mod.runCxfExport();
    expect(toast).toHaveBeenCalledWith('Nothing to export', 'err');
    const { mod } = await boot(async () => 'doc');
    confirmAnswer = false;
    await mod.runCxfExport();
    expect(saved).toHaveLength(0);
  });

  it('needs the desktop app to build the document', async () => {
    const { mod } = await boot();
    await mod.runCxfExport();
    expect(toast).toHaveBeenCalledWith(
      expect.stringContaining('Exporting a CXF file needs the desktop app'),
      'err',
      3500,
    );
    expect(saved).toHaveLength(0);
  });

  it('writes the file and says where, with singular and plural counts', async () => {
    const { mod } = await boot(async () => '{"cxf":1}');
    await mod.runCxfExport();
    expect(saved[0]).toEqual([
      '{"cxf":1}',
      expect.stringMatching(/^unenverse-\d{4}-\d{2}-\d{2}\.cxf\.json$/),
      'application/json',
    ]);
    expect(toast).toHaveBeenCalledWith('Exported 1 entry → /x/f.json ✓', 'ok', 3500);
    saveResult = { ok: true };
    const two = await boot(async () => 'd', [makeEntry({ id: '1' }), makeEntry({ id: '2' })]);
    await two.mod.runCxfExport();
    expect(toast).toHaveBeenCalledWith('Exported 2 entries ✓', 'ok', 3500);
  });

  it('reports a failed build and a failed write', async () => {
    let thrown: unknown = new Error('nope');
    const { mod } = await boot(async () => {
      throw thrown;
    });
    await mod.runCxfExport();
    expect(toast).toHaveBeenCalledWith('nope', 'err', 5000);
    thrown = 'plain';
    await mod.runCxfExport();
    expect(toast).toHaveBeenCalledWith('plain', 'err', 5000);
    const empty = await boot(async () => undefined);
    await empty.mod.runCxfExport();
    expect(saved).toHaveLength(0);
    saveResult = { ok: false, error: 'denied' };
    const bad = await boot(async () => 'doc');
    await bad.mod.runCxfExport();
    expect(toast).toHaveBeenCalledWith('Could not write the file: denied', 'err', 5000);
  });
});
