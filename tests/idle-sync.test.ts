// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { loadRealIndexHtml } from './helpers';

// R15: the page tells the Rust idle lock how long to wait and when auto-lock is paused.
describe('idle lock sync', () => {
  beforeEach(() => {
    loadRealIndexHtml();
    vi.resetModules();
    delete (window as unknown as { __TAURI__?: unknown }).__TAURI__;
  });

  const sent = (invoke: { mock: { calls: unknown[][] } }) =>
    invoke.mock.calls.filter((c) => c[0] === 'idle_lock_configure').map((c) => c[1]);

  it('sends the configured minutes once, and not again for activity inside ten seconds', async () => {
    const invoke = vi.fn(async () => null);
    (window as unknown as { __TAURI__: unknown }).__TAURI__ = { core: { invoke } };
    const state = await import('../src/ts/state');
    const lock = await import('../src/ts/lock');
    state.Settings.set('autoLockMinutes', 7);
    lock.resetLock();
    lock.resetLock();
    lock.resetLock();
    expect(sent(invoke)).toEqual([{ minutes: 7, paused: false }]);
    clearTimeout(state.st.lockTimer!);
  });

  it('says paused while a LAN server is running, and off when auto-lock is off', async () => {
    const invoke = vi.fn(async () => null);
    (window as unknown as { __TAURI__: unknown }).__TAURI__ = { core: { invoke } };
    const state = await import('../src/ts/state');
    const lock = await import('../src/ts/lock');
    state.Settings.set('autoLockMinutes', 5);
    state.st.lanServerRunning = true;
    lock.resetLock();
    state.st.lanServerRunning = false;
    state.Settings.set('autoLockMinutes', 0);
    lock.resetLock();
    expect(sent(invoke)).toEqual([
      { minutes: 5, paused: true },
      { minutes: 0, paused: false },
    ]);
  });

  it('sends nothing in a plain browser', async () => {
    const state = await import('../src/ts/state');
    const lock = await import('../src/ts/lock');
    state.Settings.set('autoLockMinutes', 7);
    expect(() => lock.resetLock()).not.toThrow();
    clearTimeout(state.st.lockTimer!);
  });
});
