// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
/**
 * Authenticator import and export, from the user's side.
 *
 * Seeds are second factors that cannot be recovered once lost, so the flow has
 * to show what will happen before it happens, never overwrite a differing seed
 * unasked, and treat the export as the plaintext secrets it is.
 */
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { loadRealIndexHtml, makeEntry, makeVault, resetState } from './helpers';

const toasts: [string, string?][] = [];
const confirms: string[] = [];
let confirmAnswer = true;
const downloads: unknown[][] = [];
vi.mock('../src/ts/utils', async (importOriginal) => {
  const real = await importOriginal<typeof import('../src/ts/utils')>();
  return {
    ...real,
    showToast: (m: string, k?: string) => toasts.push([m, k]),
    showConfirm: async (m: string) => {
      confirms.push(m);
      return confirmAnswer;
    },
  };
});
vi.mock('../src/ts/import-export', async (importOriginal) => {
  const real = await importOriginal<typeof import('../src/ts/import-export')>();
  return { ...real, downloadText: async (...a: unknown[]) => void downloads.push(a) };
});

async function boot(
  invoke?: (cmd: string, args: any) => Promise<unknown>,
  entries = [makeEntry({ id: 'a', provider: 'Old' })],
) {
  vi.resetModules();
  loadRealIndexHtml();
  if (invoke) (window as any).__TAURI__ = { core: { invoke } };
  else delete (window as any).__TAURI__;
  const state = await import('../src/ts/state');
  resetState(state.st);
  state.st.vault = makeVault({ api_keys: entries });
  state.st.store = { save: async () => {}, isRemote: false } as any;
  return { st: state.st, mod: await import('../src/ts/totp-io') };
}

const ref = (provider: string, account = '') => ({ provider, account });
const report = (over: object = {}) => ({
  entries: [makeEntry({ id: 'n1', provider: 'Merged' })],
  format: 'Aegis',
  created: [ref('GitHub', 'me')],
  updated: [],
  unchanged: [],
  conflicts: [],
  skipped: [],
  ...over,
});

beforeEach(() => {
  toasts.length = 0;
  confirms.length = 0;
  downloads.length = 0;
  confirmAnswer = true;
});

describe('importing', () => {
  it('shows the counts and asks before anything is written, then replaces the entries with the merge', async () => {
    const { mod, st } = await boot(async () => report());
    await mod.runTotpImport('file');
    expect(confirms[0]).toContain('Read a Aegis export: 1 added.');
    expect(confirms[0].endsWith('Import?')).toBe(true);
    expect(st.vault.api_keys.map((e) => e.provider)).toEqual(['Merged']);
    expect(toasts.map(([m]) => m)).toContain('Imported: 1 added');
  });

  it('declining leaves the vault exactly as it was', async () => {
    confirmAnswer = false;
    const { mod, st } = await boot(async () => report());
    await mod.runTotpImport('file');
    expect(st.vault.api_keys.map((e) => e.provider)).toEqual(['Old']);
    expect(toasts).toEqual([]);
  });

  it('names the entries that keep their existing seed, before and after', async () => {
    const { mod } = await boot(async () =>
      report({ conflicts: [ref('AWS', 'root'), ref('Gmail')] }),
    );
    await mod.runTotpImport('file');
    expect(confirms[0]).toContain(
      '2 of these already hold a different seed and will NOT be touched',
    );
    expect(confirms[0]).toContain('• AWS (root)');
    expect(confirms[0]).toContain('• Gmail\n');
    expect(confirms[0]).toContain('newer than what is in the vault');
    expect(
      toasts.some(([m, k]) => m.includes('2 entries kept their existing seed') && k === 'err'),
    ).toBe(true);
  });

  it('lists up to eight skipped entries with their reasons', async () => {
    const skipped = Array.from({ length: 10 }, (_, i) => ({ name: `N${i}`, reason: 'bad secret' }));
    const { mod } = await boot(async () => report({ skipped }));
    await mod.runTotpImport('file');
    expect(confirms[0]).toContain('• N7 — bad secret');
    expect(confirms[0]).not.toContain('N8');
    expect(confirms[0]).toContain('10 skipped');
  });

  it('says what happened when nothing could be imported', async () => {
    const empty = report({ created: [], skipped: [{ name: 'x', reason: 'r' }] });
    const a = await boot(async () => empty);
    await a.mod.runTotpImport('file');
    expect(toasts.map(([m]) => m)).toContain('Nothing importable — 1 entries were skipped');
    const b = await boot(async () => report({ created: [] }));
    await b.mod.runTotpImport('file');
    expect(toasts.map(([m]) => m)).toContain('No seeds found in that file');
    expect(confirms).toHaveLength(0);
  });

  it('passes the reader’s own message through when the file cannot be read', async () => {
    const { mod } = await boot(async () => {
      throw new Error('Encrypted Aegis export: decrypt it in Aegis first');
    });
    await mod.runTotpImport('file');
    expect(toasts[0]).toEqual(['Encrypted Aegis export: decrypt it in Aegis first', 'err']);
  });

  it('needs the desktop app, and does nothing when the reader returns nothing', async () => {
    const a = await boot();
    await a.mod.runTotpImport('file');
    expect(toasts[0][0]).toContain('needs the desktop app');
    toasts.length = 0;
    const b = await boot(async () => undefined);
    await b.mod.runTotpImport('file');
    expect(toasts).toEqual([]);
  });

  it('asks the merge never to force a differing seed', async () => {
    const invoke = vi.fn(async () => report());
    const { mod } = await boot(invoke);
    await mod.runTotpImport('file');
    expect(invoke).toHaveBeenCalledWith(
      'totp_import_merge',
      expect.objectContaining({ force: false, text: 'file' }),
    );
  });
});

describe('exporting', () => {
  const withSeed = (over: object = {}) =>
    makeEntry({ id: 's', provider: 'GitHub', totp_secret: 'JBSWY3DP', ...over } as any);

  it('warns that the file is unencrypted, then writes a dated file in the chosen format', async () => {
    const invoke = vi.fn(async () => 'BODY');
    const { mod } = await boot(invoke, [withSeed(), makeEntry({ id: 'x', provider: 'NoSeed' })]);
    await mod.runTotpExport('aegis');
    expect(confirms[0]).toContain('Write 1 authenticator seeds as Aegis (unencrypted JSON)');
    expect(confirms[0]).toContain('NOT encrypted');
    expect(downloads[0][0]).toBe('BODY');
    expect(downloads[0][1]).toMatch(/^unenverse-2fa-aegis-\d{4}-\d{2}-\d{2}\.json$/);
    expect(downloads[0][2]).toBe('Exported 1 seeds');
  });

  it('only entries with a real seed are exported, with the standard parameters filled in', async () => {
    const invoke = vi.fn(async () => 'BODY');
    const { mod } = await boot(invoke, [
      withSeed({ account_name: 'me', totp_algorithm: 'SHA256', totp_digits: 8, totp_period: 60 }),
      withSeed({ id: 't', provider: 'Blank', totp_secret: '   ' }),
      withSeed({ id: 'u', provider: 'Plain' }),
    ]);
    await mod.runTotpExport('otpauth');
    const items = (invoke.mock.calls[0] as unknown as [string, { items: any[] }])[1].items;
    expect(items).toEqual([
      {
        issuer: 'GitHub',
        account: 'me',
        secret: 'JBSWY3DP',
        algorithm: 'SHA256',
        digits: 8,
        period: 60,
        note: null,
      },
      {
        issuer: 'Plain',
        account: null,
        secret: 'JBSWY3DP',
        algorithm: 'SHA1',
        digits: 6,
        period: 30,
        note: null,
      },
    ]);
    expect(downloads[0][1]).toMatch(/\.txt$/);
  });

  it('an unknown format falls back to the first one on offer', async () => {
    const { mod } = await boot(async () => 'BODY', [withSeed()]);
    await mod.runTotpExport('mystery');
    expect(confirms[0]).toContain('otpauth:// list');
  });

  it('writes nothing when declined, when there is nothing to write, or when the build fails', async () => {
    confirmAnswer = false;
    const a = await boot(async () => 'BODY', [withSeed()]);
    await a.mod.runTotpExport('2fas');
    expect(downloads).toHaveLength(0);
    confirmAnswer = true;
    const b = await boot(async () => 'BODY', [makeEntry({ id: 'x' })]);
    await b.mod.runTotpExport('2fas');
    expect(toasts.map(([m]) => m)).toContain('No entry carries an authenticator seed');
    const c = await boot(async () => {
      throw new Error('writer failed');
    }, [withSeed()]);
    await c.mod.runTotpExport('2fas');
    expect(toasts.map(([m]) => m)).toContain('writer failed');
    expect(downloads).toHaveLength(0);
    const d = await boot(async () => undefined, [withSeed()]);
    await d.mod.runTotpExport('2fas');
    expect(downloads).toHaveLength(0);
  });

  it('building directly needs the desktop app and a seed', async () => {
    const a = await boot(undefined, [withSeed()]);
    expect(await a.mod.buildTotpExport('aegis')).toBeNull();
    const b = await boot(async () => 'BODY', [makeEntry({ id: 'x' })]);
    expect(await b.mod.buildTotpExport('aegis')).toBeNull();
  });
});
