// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
/** Branch coverage for enrich-pane.ts: online edge cases, apply skips, doctor on a remote. */
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { loadRealIndexHtml, makeEntry, makeVault, resetState } from './helpers';

let confirmAnswer = true;
vi.mock('../src/ts/utils', async (importOriginal) => {
  const real = await importOriginal<typeof import('../src/ts/utils')>();
  return { ...real, showConfirm: async () => confirmAnswer };
});

const $ = (id: string) => document.getElementById(id)!;
const flush = () => new Promise((r) => setTimeout(r, 30));

beforeEach(() => {
  loadRealIndexHtml();
  vi.resetModules();
  confirmAnswer = true;
  delete (window as any).__TAURI__;
});

async function boot(
  invoke?: (cmd: string, args?: unknown) => Promise<unknown>,
  entries = [makeEntry({ id: 'a', provider: 'Alpha' })],
) {
  if (invoke) (window as any).__TAURI__ = { core: { invoke } };
  (await import('../src/ts/tools-markup')).mountToolsPanes();
  const state = await import('../src/ts/state');
  resetState(state.st);
  state.st.vault = makeVault({ api_keys: entries });
  const mod = await import('../src/ts/enrich-pane');
  mod.initEnrichPane();
  mod.initDoctorPane();
  return state;
}

const proposal = (field: string, value: unknown) => ({ field, value, reason: 'r' });

describe('enrich online edges', () => {
  it('says so when no secret has a recognised issuer', async () => {
    await boot(async (cmd) => (cmd === 'enrich_plan' ? [] : []));
    ($('enrich-online') as HTMLInputElement).checked = true;
    $('enrich-preview-btn').click();
    await flush();
    expect($('enrich-status').textContent).toBeTruthy();
  });

  it('plural consent text, a live answer for a new plan, a non-ok detail, replaced proposals', async () => {
    const targets = [
      { id: 'a', provider: 'Alpha', issuer: 'GitHub' },
      { id: 'b', provider: 'Beta', issuer: 'GitHub' },
    ];
    const lives = [
      {
        id: 'a',
        provider: 'Alpha',
        issuer: 'GitHub',
        status: 'revoked',
        detail: 'was revoked',
        proposals: [proposal('tags', ['x'])],
      },
      { id: 'zzz', provider: 'New', issuer: 'GitHub', status: 'ok', detail: '', proposals: [] },
    ];
    const seen: string[] = [];
    const state = await boot(
      async (cmd) => {
        if (cmd === 'enrich_plan')
          return [
            {
              id: 'a',
              provider: 'Alpha',
              fingerprint: 'fp',
              proposals: [proposal('tags', ['old']), proposal('keep', 1)],
            },
          ];
        if (cmd === 'enrich_online_targets') return targets;
        seen.push(cmd);
        return lives;
      },
      [makeEntry({ id: 'a', provider: 'Alpha' }), makeEntry({ id: 'b', provider: 'Beta' })],
    );
    ($('enrich-online') as HTMLInputElement).checked = true;
    $('enrich-preview-btn').click();
    await flush();
    expect(seen).toContain('enrich_online');
    expect($('enrich-results').textContent).toContain('was revoked');
    expect($('enrich-results').textContent).toContain('GitHub: ok');
    expect($('enrich-results').textContent).not.toContain('old');
    expect($('enrich-status').textContent).toBe('2 entries with proposals.');
    void state;
  });
});

describe('enrich apply edges', () => {
  it('previews one entry (singular), skips a proposal whose entry is gone, writes one field', async () => {
    const plans = [
      { id: 'a', provider: 'Alpha', fingerprint: 'fp', proposals: [proposal('tags', ['x'])] },
      { id: 'gone', provider: 'Gone', fingerprint: 'fp', proposals: [proposal('tags', ['y'])] },
    ];
    const state = await boot(async (cmd) => (cmd === 'enrich_plan' ? plans : null));
    $('enrich-preview-btn').click();
    await flush();
    document
      .querySelectorAll<HTMLInputElement>('#enrich-results input[data-enrich]')
      .forEach((b) => (b.checked = true));
    $('enrich-apply-btn').click();
    await flush();
    expect((state.st.vault.api_keys[0] as any).tags).toEqual(['x']);
    expect($('toast').textContent).toContain('1 field written');
  });

  it('a stale index (plan gone) is skipped without throwing', async () => {
    await boot(async (cmd) =>
      cmd === 'enrich_plan'
        ? [{ id: 'a', provider: 'A', fingerprint: 'f', proposals: [proposal('tags', ['x'])] }]
        : null,
    );
    $('enrich-preview-btn').click();
    await flush();
    const box = document.querySelector<HTMLInputElement>('#enrich-results input[data-enrich]')!;
    box.dataset.enrich = '9:9';
    box.checked = true;
    $('enrich-apply-btn').click();
    await flush();
    expect($('toast').textContent).toContain('0 fields written');
  });

  it('a non-Error planner failure is shown as text', async () => {
    await boot(async () => Promise.reject('plain' as unknown as Error));
    $('enrich-preview-btn').click();
    await flush();
    expect($('enrich-status').textContent).toContain('Could not plan: plain');
  });
});

describe('doctor edges', () => {
  it('on a remote vault skips the file half and points at the server; plural and all-clear', async () => {
    const state = await boot(async (cmd) => {
      if (cmd === 'doctor_document')
        return [
          { check: 'a', level: 'fail', message: 'm', remedy: '' },
          { check: 'b', level: 'warn', message: 'm', remedy: 'fix' },
        ];
      throw new Error('doctor_file must not run');
    });
    state.st.store = new state.RemoteVaultStore('http://localhost:1');
    $('doctor-run-btn').click();
    await flush();
    expect($('doctor-status').textContent).toContain('2 things need attention.');
    expect($('doctor-status').textContent).toContain('run on the server');

    (window as any).__TAURI__.core.invoke = async () => [
      { check: 'ok', level: 'ok', message: 'fine', remedy: '' },
    ];
    $('doctor-run-btn').click();
    await flush();
    expect($('doctor-status').textContent).toContain('All checks passed.');
  });

  it('reports a failure as text, Error or not', async () => {
    await boot(async () => Promise.reject(new Error('nope')));
    $('doctor-run-btn').click();
    await flush();
    expect($('doctor-status').textContent).toContain('nope');
    (window as any).__TAURI__.core.invoke = async () => Promise.reject('str' as unknown as Error);
    $('doctor-run-btn').click();
    await flush();
    expect($('doctor-status').textContent).toContain('str');
  });
});
