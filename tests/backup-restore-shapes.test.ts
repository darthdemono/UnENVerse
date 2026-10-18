/**
 * Restoring from an encrypted backup or a plain JSON export: what a damaged or
 * minimal file does. A restore replaces the whole vault, so every refusal must
 * leave the open vault alone, and a file missing optional parts must still give
 * a usable vault (with the default project).
 */
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { st } from '../src/ts/state';
import {
  exportEncryptedBackup,
  importEncryptedBackup,
  handleFileSelect,
} from '../src/ts/import-export';
import { loadRealIndexHtml, makeEntry, makeProject, makeVault, resetState } from './helpers';

const toasts: [string, string?][] = [];
const saved: string[] = [];
vi.mock('../src/ts/utils', async (importOriginal) => {
  const real = await importOriginal<typeof import('../src/ts/utils')>();
  return {
    ...real,
    showToast: (m: string, k?: string) => toasts.push([m, k]),
    showConfirm: async () => true,
    saveFile: async (c: string) => {
      saved.push(c);
      return { ok: true as const };
    },
  };
});

const flush = (ms = 30) => new Promise((r) => setTimeout(r, ms));
const PW = 'a long enough backup password';

beforeEach(() => {
  loadRealIndexHtml();
  resetState(st);
  toasts.length = 0;
  saved.length = 0;
  st.store = { save: async () => {}, isRemote: false } as never;
  st.vault = makeVault({
    api_keys: [makeEntry({ id: 'keep', provider: 'Keep', api_key: 'keep-value-1' } as any)],
    projects: [makeProject({ id: 'Universal', name: 'Universal' })],
  });
});

/** A real encrypted backup of the open vault, parsed into its envelope. */
async function envelopeFor(_unused: unknown) {
  await exportEncryptedBackup(PW);
  await vi.waitFor(() => expect(saved).toHaveLength(1), { timeout: 20000 });
  return { env: JSON.parse(saved[0]) };
}

describe('refusing a bad backup', () => {
  it('text that is not JSON, or JSON of another kind, or an envelope missing parts, leaves the vault alone', async () => {
    await importEncryptedBackup('not json', PW);
    expect(toasts[toasts.length - 1]).toEqual(['Not a valid backup file', 'err']);
    await importEncryptedBackup('{"magic":"other"}', PW);
    expect(toasts[toasts.length - 1]).toEqual(['Unrecognised backup format', 'err']);
    const { env } = await envelopeFor({});
    for (const missing of ['salt', 'iv', 'ct']) {
      const broken = { ...env };
      delete broken[missing];
      await importEncryptedBackup(JSON.stringify(broken), PW);
      expect(toasts[toasts.length - 1][0]).toContain('Decryption failed');
    }
    expect(st.vault.api_keys.map((e) => e.id)).toEqual(['keep']);
  });

  it('an envelope whose recorded iteration count is junk falls back to the default and still opens', async () => {
    const { env } = await envelopeFor({});
    const junk = { ...env, kdf: { ...env.kdf, iters: -5 } };
    await importEncryptedBackup(JSON.stringify(junk), PW);
    await vi.waitFor(() => expect(toasts.some(([m]) => m.startsWith('Restored'))).toBe(true), {
      timeout: 20000,
    });
  });

  it('a wrong password is "wrong password or corrupt file"', async () => {
    const { env } = await envelopeFor({});
    await importEncryptedBackup(JSON.stringify(env), 'the WRONG password entirely');
    expect(toasts[toasts.length - 1][0]).toContain('Decryption failed');
    expect(st.vault.api_keys.map((e) => e.id)).toEqual(['keep']);
  });
});

describe('plain JSON restores', () => {
  const pick = async (name: string, text: string) => {
    const input = document.createElement('input');
    Object.defineProperty(input, 'files', { value: [new File([text], name)], configurable: true });
    handleFileSelect(input);
    await flush(80);
  };

  it('a full vault replaces the open one; missing lists get defaults and entries get the default project and a type', async () => {
    await pick(
      'v.json',
      JSON.stringify({ api_keys: [{ id: 'n', provider: 'New', api_key: 'new-value-1' }] }),
    );
    expect(st.vault.api_keys.map((e) => e.provider)).toEqual(['New']);
    expect(st.vault.api_keys[0].projectIds).toEqual(['Universal']);
    expect(st.vault.api_keys[0].secretType).toBe('api_key');
    expect(st.vault.user_categories).toEqual([]);
    // Regression: a file with no projects left the vault with none, not even the default.
    expect(st.vault.projects.map((p) => p.id)).toEqual(['Universal']);
  });

  it('keeps the projects and categories a file brings, and adds Universal to an entry that lacks it', async () => {
    await pick(
      'v.json',
      JSON.stringify({
        api_keys: [
          {
            id: 'n',
            provider: 'New',
            api_key: 'new-value-1',
            projectIds: ['web'],
            secretType: 'password',
          },
        ],
        user_categories: ['infra', 7],
        projects: [{ id: 'web', name: 'Web' }],
      }),
    );
    expect(st.vault.api_keys[0].projectIds).toEqual(['web', 'Universal']);
    expect(st.vault.api_keys[0].secretType).toBe('password');
    expect(st.vault.user_categories).toEqual(['infra']);
    expect(st.vault.projects.map((p) => p.id)).toEqual(['web']);
  });

  it('a JSON file with no entry list is not treated as a vault', async () => {
    await pick('v.json', JSON.stringify({ something: 'else' }));
    expect(st.vault.api_keys.map((e) => e.id)).toEqual(['keep']);
  });
});
