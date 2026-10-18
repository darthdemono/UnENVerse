/** Branch coverage for lan.ts: status polling, start/stop, the card in every state and its buttons. */
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { loadRealIndexHtml, resetState } from './helpers';

const toasts: [string, string?][] = [];
let confirmAnswer = true;
const confirms: string[] = [];
vi.mock('../src/ts/utils', async (importOriginal) => {
  const real = await importOriginal<typeof import('../src/ts/utils')>();
  return {
    ...real,
    showToast: (m: string, k?: string) => toasts.push([m, k]),
    showConfirm: async (m: string) => {
      confirms.push(m);
      return confirmAnswer;
    },
    clipboardWrite: async () => true,
  };
});

const $ = (id: string) => document.getElementById(id)!;
const flush = (ms = 10) => new Promise((r) => setTimeout(r, ms));
const status = (over: object = {}) => ({
  running: true,
  port: 8443,
  url: 'https://10.0.0.2:8443',
  fingerprint: 'AB'.repeat(32),
  peers: 0,
  idle_secs: 0,
  ...over,
});

type Handler = (cmd: string, args?: any) => unknown;
async function boot(handler: Handler | null, remote = false) {
  vi.resetModules();
  loadRealIndexHtml();
  if (handler)
    (window as any).__TAURI__ = {
      core: { invoke: vi.fn(async (c: string, a: any) => handler(c, a)) },
    };
  else delete (window as any).__TAURI__;
  const state = await import('../src/ts/state');
  resetState(state.st);
  state.st.store = (
    remote ? new state.RemoteVaultStore('https://r.example') : new state.LocalVaultStore()
  ) as any;
  const lan = await import('../src/ts/lan');
  return { st: state.st, lan, invoke: (window as any).__TAURI__?.core?.invoke as any };
}

beforeEach(() => {
  toasts.length = 0;
  confirms.length = 0;
  confirmAnswer = true;
});
afterEach(() => {
  vi.useRealTimers();
  delete (window as any).__TAURI__;
});

describe('card states', () => {
  it('is empty in a plain browser, muted on a remote, and offers start locally', async () => {
    const plain = await boot(null);
    plain.lan.refreshLanPanel();
    expect($('lan-card').textContent).toBe('');
    const remote = await boot(async () => status({ running: false }), true);
    remote.lan.refreshLanPanel();
    expect($('lan-card').textContent).toContain('Switch to the local vault');
    const local = await boot(async () => status({ running: false }));
    local.lan.refreshLanPanel();
    expect($('lan-card').textContent).toContain('Not serving');
    expect($('lan-start-btn')).toBeTruthy();
    $('lan-start-btn').click();
    await flush(30);
    expect(local.invoke).toHaveBeenCalledWith('lan_start', { port: null, tls: true });
  });
});

describe('start', () => {
  it('refuses with the right reason when unavailable', async () => {
    const plain = await boot(null);
    await plain.lan.startLan();
    expect(toasts[0][0]).toContain('only available in the desktop app');
    const remote = await boot(async () => status(), true);
    await remote.lan.startLan();
    expect(toasts[1][0]).toContain('switch to the local vault first');
  });

  it('starts, shows the card with a fingerprint, plural peers, and wires the buttons', async () => {
    const { lan, st, invoke } = await boot((cmd) => {
      if (cmd === 'lan_start') return status({ peers: 2 });
      if (cmd === 'lan_status') return status({ peers: 2 });
      return null;
    });
    await lan.startLan();
    await lan.startLan();
    expect(st.lanServerRunning).toBe(true);
    expect($('lan-card').textContent).toContain('Serving · 2 peers');
    expect($('lan-card').textContent).toContain('TLS fingerprint');
    expect(toasts.some(([m]) => m.startsWith('Serving at'))).toBe(true);
    $('lan-copy-url').click();
    $('lan-copy-fp').click();
    await flush();
    expect(toasts.map(([m]) => m)).toEqual(
      expect.arrayContaining(['Address copied ✓', 'Fingerprint copied ✓']),
    );
    confirmAnswer = false;
    $('lan-stop-btn').click();
    await flush();
    expect(confirms[0]).toContain('2 peers are connected. Disconnect all?');
    expect(invoke).not.toHaveBeenCalledWith('lan_stop', undefined);
    confirmAnswer = true;
    $('lan-stop-btn').click();
    await flush(30);
    expect(invoke).toHaveBeenCalledWith('lan_stop', undefined);
    expect(st.lanServerRunning).toBe(false);
  });

  it('single peer and no TLS wording; stop without peers asks nothing', async () => {
    const { lan } = await boot((cmd) =>
      cmd === 'lan_start' || cmd === 'lan_status' ? status({ peers: 1, fingerprint: null }) : null,
    );
    await lan.startLan();
    expect($('lan-card').textContent).toContain('Serving · 1 peer');
    expect($('lan-card').textContent).toContain('Running without TLS');
    expect($('lan-copy-fp')).toBeNull();
    $('lan-stop-btn').click();
    await flush();
    expect(confirms[0]).toContain('1 peer is connected. Disconnect them?');
    const none = await boot((cmd) => (cmd === 'lan_start' ? status({ peers: 0 }) : null));
    await none.lan.startLan();
    $('lan-stop-btn').click();
    await flush(30);
    expect(confirms).toHaveLength(1);
  });

  it('explains a missing user account, and any other failure', async () => {
    let msg = 'No user account exists yet';
    const { lan } = await boot((cmd) => {
      if (cmd === 'lan_start') throw new Error(msg);
      return null;
    });
    await lan.startLan();
    expect(toasts[0][0]).toContain('Create a user account first');
    msg = 'port busy';
    await lan.startLan();
    expect(toasts[1][0]).toBe('Could not open to LAN: port busy');
  });
});

describe('status polling', () => {
  it('init reads a status left running, polls every five seconds, and survives a failing call', async () => {
    vi.useFakeTimers();
    let calls = 0;
    let fail = false;
    const { lan, st } = await boot((cmd) => {
      if (cmd === 'lan_status') {
        calls++;
        if (fail) throw new Error('x');
        return status();
      }
      return null;
    });
    lan.initLanPanel();
    await vi.advanceTimersByTimeAsync(10);
    expect(st.lanServerRunning).toBe(true);
    expect(lan.lanStatus().running).toBe(true);
    const before = calls;
    await vi.advanceTimersByTimeAsync(5100);
    expect(calls).toBeGreaterThan(before);
    fail = true;
    await vi.advanceTimersByTimeAsync(5100);
    expect(lan.lanStatus().running).toBe(false);
    await lan.stopLan(true);
  });

  it('init with nothing running does not poll; a null status counts as stopped', async () => {
    vi.useFakeTimers();
    const { lan } = await boot(() => null);
    lan.initLanPanel();
    await vi.advanceTimersByTimeAsync(10);
    expect(vi.getTimerCount()).toBe(0);
  });

  it('closes an abandoned server after eight idle hours', async () => {
    vi.useFakeTimers();
    const { lan, invoke } = await boot((cmd) =>
      cmd === 'lan_status' ? status({ idle_secs: 9 * 3600 }) : null,
    );
    lan.initLanPanel();
    await vi.advanceTimersByTimeAsync(50);
    vi.useRealTimers();
    await flush(100);
    expect(invoke).toHaveBeenCalledWith('lan_stop', undefined);
    expect(toasts.some(([m]) => m.includes('closed after 8h'))).toBe(true);
  });
});

describe('stop and lock', () => {
  it('stop tolerates an already-down server and announces unless silent', async () => {
    const { lan } = await boot((cmd) => {
      if (cmd === 'lan_stop') throw new Error('down');
      return null;
    });
    await lan.stopLan();
    expect(toasts.map(([m]) => m)).toContain('LAN server closed');
    toasts.length = 0;
    await lan.stopLan(true);
    expect(toasts.map(([m]) => m)).not.toContain('LAN server closed');
  });

  it('locking is free when not serving, asks when peers are connected, and respects a refusal', async () => {
    const idle = await boot(() => null);
    expect(await idle.lan.confirmStopForLock()).toBe(true);

    const busy = await boot((cmd) => (cmd === 'lan_start' ? status({ peers: 3 }) : null));
    await busy.lan.startLan();
    confirmAnswer = false;
    expect(await busy.lan.confirmStopForLock()).toBe(false);
    expect(confirms[confirms.length - 1]).toContain('3 peers are connected');
    expect(confirms[confirms.length - 1]).toContain('disconnects all of them');
    confirmAnswer = true;
    expect(await busy.lan.confirmStopForLock()).toBe(true);

    const one = await boot((cmd) => (cmd === 'lan_start' ? status({ peers: 1 }) : null));
    await one.lan.startLan();
    expect(await one.lan.confirmStopForLock()).toBe(true);
    expect(confirms[confirms.length - 1]).toContain('1 peer is connected');
    expect(confirms[confirms.length - 1]).toContain('disconnects them');

    const quiet = await boot((cmd) => (cmd === 'lan_start' ? status({ peers: 0 }) : null));
    await quiet.lan.startLan();
    const n = confirms.length;
    expect(await quiet.lan.confirmStopForLock()).toBe(true);
    expect(confirms).toHaveLength(n);
  });
});
