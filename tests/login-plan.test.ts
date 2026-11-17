// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
// The login steps on a bundle card (R36). The order is decided in Rust; these tests stand in for it
// with a recording stub and check what the section shows, sends and stores.
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { loginPlanSection } from '../src/ts/login-plan';
import { st } from '../src/ts/state';
import type { VaultEntry } from '../src/ts/types';
import { makeVault, resetState } from './helpers';

const toasts: string[] = [];
vi.mock('../src/ts/utils', async (importOriginal) => {
  const real = await importOriginal<typeof import('../src/ts/utils')>();
  return { ...real, showToast: (m: string) => toasts.push(m) };
});

const tick = () => new Promise((r) => setTimeout(r, 0));
let calls: { cmd: string; args: Record<string, unknown> }[] = [];
let answer: (cmd: string) => unknown;
let saves = 0;

const bundle = { id: 'b1', provider: 'Bank', secretType: 'bundle' } as unknown as VaultEntry;
const members = [
  { id: 'm1', provider: 'Bank', secretType: 'password', bundle_id: 'b1' },
  { id: 'm2', provider: 'Bank codes', secretType: 'recovery_codes', bundle_id: 'b1' },
] as unknown as VaultEntry[];
const PLAN = {
  steps: [
    { step: 'password', label: 'Password' },
    { step: 'recovery_code', label: 'Recovery code' },
  ],
  terminate: 'ask',
};

beforeEach(() => {
  resetState(st);
  toasts.length = 0;
  calls = [];
  saves = 0;
  answer = () => PLAN;
  st.vault = makeVault({ api_keys: [bundle, ...members] } as never);
  st.vaultOpen = true;
  st.store = {
    save: async () => {
      saves += 1;
    },
    isRemote: false,
  } as never;
  (window as unknown as { __TAURI__: unknown }).__TAURI__ = {
    core: {
      invoke: async (cmd: string, args: Record<string, unknown>) => {
        calls.push({ cmd, args });
        return answer(cmd);
      },
    },
  };
});

afterEach(() => {
  delete (window as unknown as { __TAURI__?: unknown }).__TAURI__;
});

describe('the section', () => {
  it('shows the steps in the order Rust gave and offers the three choices', async () => {
    const s = loginPlanSection(bundle, members)!;
    await tick();
    expect(calls[0]).toMatchObject({ cmd: 'login_plan' });
    expect(s.textContent).toContain('Password → Recovery code');
    expect([...s.querySelectorAll('option')].map((o) => o.value)).toEqual(['ask', 'auto', 'never']);
    expect(s.querySelector<HTMLSelectElement>('select')!.value).toBe('ask');
  });

  it('shows the stored choice and has no choice at all when there is no recovery code step', async () => {
    answer = () => ({ ...PLAN, terminate: 'never' });
    const s = loginPlanSection(bundle, members)!;
    await tick();
    expect(s.querySelector<HTMLSelectElement>('select')!.value).toBe('never');
    answer = () => ({
      steps: [
        { step: 'password', label: 'Password' },
        { step: '2fa', label: '2FA code' },
      ],
      terminate: 'ask',
    });
    const t = loginPlanSection(bundle, members)!;
    await tick();
    expect(t.querySelector('select')).toBeNull();
    expect(t.textContent).toContain('Password → 2FA code');
  });

  it('is absent without the desktop or with fewer than two members', () => {
    expect(loginPlanSection(bundle, [members[0]])).toBeNull();
    delete (window as unknown as { __TAURI__?: unknown }).__TAURI__;
    expect(loginPlanSection(bundle, members)).toBeNull();
  });

  it('removes itself when there is one step or Rust cannot answer', async () => {
    const host = document.createElement('div');
    answer = () => ({ steps: [{ step: 'password', label: 'Password' }], terminate: 'ask' });
    host.append(loginPlanSection(bundle, members)!);
    await tick();
    expect(host.children).toHaveLength(0);
    answer = () => {
      throw new Error('no');
    };
    host.append(loginPlanSection(bundle, members)!);
    await tick();
    expect(host.children).toHaveLength(0);
  });
});

describe('choosing what happens to a used code', () => {
  const choose = async (value: string) => {
    const s = loginPlanSection(bundle, members)!;
    await tick();
    const select = s.querySelector<HTMLSelectElement>('select')!;
    select.value = value;
    select.dispatchEvent(new Event('change'));
    await tick();
  };

  it('stores it on the bundle that is in the vault now and saves', async () => {
    await choose('auto');
    expect(st.vault.api_keys.find((e) => e.id === 'b1')!.terminate_codes).toBe('auto');
    expect(saves).toBe(1);
    expect(toasts[toasts.length - 1]).toBe('Saved');
  });

  it('does nothing when the bundle was removed meanwhile', async () => {
    const s = loginPlanSection(bundle, members)!;
    await tick();
    st.vault.api_keys = [];
    const select = s.querySelector<HTMLSelectElement>('select')!;
    select.value = 'never';
    select.dispatchEvent(new Event('change'));
    await tick();
    expect(saves).toBe(0);
  });

  it('shows the reason when saving fails', async () => {
    st.store = {
      save: async () => {
        throw new Error('disk full');
      },
      isRemote: false,
    } as never;
    await choose('never');
    expect(toasts.some((t) => t.includes('disk full') || t === 'Saved')).toBe(true);
  });
});
