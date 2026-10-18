/**
 * The timer half of vault-watch: start, stop, the hidden-window skip, and the
 * branches of one check that the main file leaves alone. `inTauri` is fixed at
 * import time, so each test imports a fresh copy with the bridge already present.
 */
import { describe, it, expect, afterEach, vi } from 'vitest';

async function fresh(withBridge: boolean, version = 'v1') {
  vi.resetModules();
  if (withBridge) {
    (window as any).__TAURI__ = {
      core: { invoke: vi.fn().mockResolvedValue(version) },
    };
  } else {
    delete (window as any).__TAURI__;
  }
  const state = await import('../src/ts/state');
  const watch = await import('../src/ts/vault-watch');
  return { st: state.st, watch, invoke: (window as any).__TAURI__?.core?.invoke as any };
}

afterEach(() => {
  vi.useRealTimers();
  delete (window as any).__TAURI__;
});

describe('startVaultWatch / stopVaultWatch', () => {
  it('does nothing outside the desktop app', async () => {
    vi.useFakeTimers();
    const { watch } = await fresh(false);
    watch.startVaultWatch();
    expect(vi.getTimerCount()).toBe(0);
  });

  it('keeps one interval however often it is started, and stops cleanly', async () => {
    vi.useFakeTimers();
    const { watch } = await fresh(true);
    watch.startVaultWatch();
    watch.startVaultWatch();
    expect(vi.getTimerCount()).toBe(1);
    watch.stopVaultWatch();
    expect(vi.getTimerCount()).toBe(0);
    watch.stopVaultWatch();
  });

  it('polls on the tick, skips a hidden window, and checks again on focus', async () => {
    vi.useFakeTimers();
    const { st, watch, invoke } = await fresh(true, 'v1');
    st.vaultOpen = true;
    st.store = { isRemote: false, lastVersion: 'v1', load: vi.fn() } as any;
    watch.startVaultWatch();

    await vi.advanceTimersByTimeAsync(800);
    expect(invoke).toHaveBeenCalledTimes(1);

    Object.defineProperty(document, 'hidden', { value: true, configurable: true });
    await vi.advanceTimersByTimeAsync(800);
    expect(invoke).toHaveBeenCalledTimes(1);

    window.dispatchEvent(new Event('focus'));
    await vi.advanceTimersByTimeAsync(0);
    expect(invoke).toHaveBeenCalledTimes(2);

    Object.defineProperty(document, 'hidden', { value: false, configurable: true });
    watch.stopVaultWatch();
  });
});

describe('checkVaultVersion edges', () => {
  it('does not reload when our own version is unknown', async () => {
    const { st, watch } = await fresh(true, 'v2');
    st.vaultOpen = true;
    st.store = { isRemote: false, lastVersion: undefined, load: vi.fn() } as any;
    expect(await watch.checkVaultVersion()).toBe(false);
    expect((st.store as any).load).not.toHaveBeenCalled();
  });

  it('gives up quietly when the reload returns nothing', async () => {
    const { st, watch } = await fresh(true, 'v2');
    st.vaultOpen = true;
    st.store = { isRemote: false, lastVersion: 'v1', load: vi.fn().mockResolvedValue(null) } as any;
    expect(await watch.checkVaultVersion()).toBe(false);
  });
});
