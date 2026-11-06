// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { loadRealIndexHtml, makeEntry, makeVault, resetState } from './helpers';

// Answers for the dialogs, set per test. showPasswordPrompt is asked in order.
const prompts: Array<string | null> = [];
let confirmAnswer = true;
vi.mock('../src/ts/utils', async (importOriginal) => {
  const real = await importOriginal<typeof import('../src/ts/utils')>();
  return {
    ...real,
    showPasswordPrompt: vi.fn(async () => (prompts.length ? prompts.shift()! : null)),
    showConfirm: vi.fn(async () => confirmAnswer),
  };
});

// Settings, Master password and history (R20).
describe('master password and forget rows', () => {
  beforeEach(() => {
    loadRealIndexHtml();
    vi.resetModules();
    prompts.length = 0;
    confirmAnswer = true;
    delete (window as unknown as { __TAURI__?: unknown }).__TAURI__;
  });

  const tick = () => new Promise((r) => setTimeout(r, 0));
  const el = (id: string) => document.getElementById(id) as HTMLButtonElement;
  const calls = (f: { mock: { calls: unknown[][] } }, cmd: string) =>
    f.mock.calls.filter((c) => c[0] === cmd);
  async function open(invoke: (cmd: string, args?: unknown) => Promise<unknown>) {
    (window as unknown as { __TAURI__: unknown }).__TAURI__ = { core: { invoke } };
    const state = await import('../src/ts/state');
    resetState(
      state.st,
      makeVault({
        api_keys: [
          makeEntry({ id: 'a1', provider: 'GitHub' }),
          makeEntry({ id: 'b2', provider: 'Stripe' }),
        ],
      }),
    );
    const m = await import('../src/ts/settings-panel');
    m.openSettings();
    await tick();
  }

  it('shows nothing in a plain browser', async () => {
    const m = await import('../src/ts/settings-panel');
    m.openSettings();
    expect(el('s-rekey').hidden).toBe(true);
    expect(el('s-forget').hidden).toBe(true);
  });

  it('lists the entries by name and keys the choice by id', async () => {
    await open(vi.fn(async () => null));
    const opts = Array.from((el('s-forget-entry') as unknown as HTMLSelectElement).options);
    expect(opts.map((o) => o.textContent)).toEqual(['GitHub', 'Stripe']);
    expect(opts.map((o) => o.value)).toEqual(['a1', 'b2']);
  });

  it('refuses two different new passwords before anything is sent', async () => {
    const invoke = vi.fn(async () => null);
    await open(invoke);
    prompts.push('old-password-123', 'new-password-1234', 'new-password-9999');
    el('s-rekey').click();
    await tick();
    await tick();
    expect(calls(invoke, 'change_master_password')).toHaveLength(0);
  });

  it('sends nothing when the person declines the final question', async () => {
    const invoke = vi.fn(async () => null);
    await open(invoke);
    confirmAnswer = false;
    prompts.push('old-password-123', 'new-password-1234', 'new-password-1234');
    el('s-rekey').click();
    await tick();
    await tick();
    expect(calls(invoke, 'change_master_password')).toHaveLength(0);
  });

  it('sends the current and new password together once everything is confirmed', async () => {
    const invoke = vi.fn(async () => null);
    await open(invoke);
    prompts.push('old-password-123', 'new-password-1234', 'new-password-1234');
    el('s-rekey').click();
    await tick();
    await tick();
    const sent = calls(invoke, 'change_master_password');
    expect(sent).toHaveLength(1);
    expect(sent[0][1]).toEqual({ current: 'old-password-123', newPassword: 'new-password-1234' });
  });

  it('forgets only after the confirm, for the chosen entry, and says what it cannot reach', async () => {
    const report = {
      history_versions: 2,
      snapshots: 1,
      pending_values: 0,
      unreachable: ['backups made before now'],
    };
    const invoke = vi.fn(async (cmd: string) => (cmd === 'forget_entry_history' ? report : null));
    await open(invoke);
    (el('s-forget-entry') as unknown as HTMLSelectElement).value = 'b2';
    confirmAnswer = false;
    el('s-forget').click();
    await tick();
    expect(calls(invoke, 'forget_entry_history')).toHaveLength(0);
    confirmAnswer = true;
    el('s-forget').click();
    await tick();
    await tick();
    const sent = calls(invoke, 'forget_entry_history');
    expect(sent).toHaveLength(1);
    expect(sent[0][1]).toEqual({ entryId: 'b2' });
    const hint = document.getElementById('s-forget-hint')!.textContent!;
    expect(hint).toContain('Forgot 2 old value(s) and 1 stored config copy');
    expect(hint).toContain('backups made before now');
  });
});
