// SPDX-License-Identifier: PolyForm-Strict-1.0.0
// Copyright Jubair Hasan.
/**
 * Unlocking and auto-locking: what a person is told, and what is kept or wiped.
 *
 * The lock screen is the app's front door, so each test is a thing a user would
 * notice if it were wrong: a password rule that silently loosened, an auto-lock
 * that did not reset on activity, an error that blamed the wrong thing.
 */
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { st, Settings, TauriVaultStore, RemoteVaultStore, LocalVaultStore } from '../src/ts/state';
import {
  lockVault,
  resetLock,
  setFinishInitFn,
  showRelockScreen,
  showUnlockModal,
} from '../src/ts/lock';
import { loadRealIndexHtml, makeEntry, makeVault, resetState } from './helpers';

const toasts: [string, string?][] = [];
vi.mock('../src/ts/utils', async (importOriginal) => {
  const real = await importOriginal<typeof import('../src/ts/utils')>();
  return {
    ...real,
    showToast: (m: string, k?: string) => toasts.push([m, k]),
    showConfirm: async () => true,
    showPrompt: async () => null,
  };
});

const $ = <T extends HTMLElement = HTMLElement>(id: string) => document.getElementById(id) as T;
const flush = (ms = 10) => new Promise((r) => setTimeout(r, ms));

function tauriStore(unlock: (pw: string) => Promise<boolean>) {
  const store = new TauriVaultStore();
  store.unlock = unlock;
  store.lock = vi.fn().mockResolvedValue(undefined);
  return store;
}

let finished = 0;

beforeEach(() => {
  loadRealIndexHtml();
  resetState(st);
  st.vault = makeVault({ api_keys: [makeEntry({ id: 'a', provider: 'A' })] });
  st.vaultOpen = true;
  toasts.length = 0;
  finished = 0;
  setFinishInitFn(async () => {
    finished++;
  });
  Settings.set('remoteSaved', []);
  Settings.set('remote', { enabled: false, serverUrl: '' });
});

afterEach(() => {
  vi.useRealTimers();
  vi.restoreAllMocks();
});

describe('auto-lock', () => {
  it('says what it is set to, and "Off" when disabled', () => {
    Settings.set('autoLockMinutes', 5);
    resetLock();
    expect($('lock-status').textContent).toBe('Auto-lock: 5min');
    Settings.set('autoLockMinutes', 0);
    resetLock();
    expect($('lock-status').textContent).toBe('Auto-lock: Off');
    expect(st.lockTimer).toBeNull();
  });

  it('locks after the idle time and warns one minute before', async () => {
    vi.useFakeTimers();
    st.store = new LocalVaultStore();
    Settings.set('autoLockMinutes', 3);
    resetLock();
    await vi.advanceTimersByTimeAsync(2 * 60_000 + 1000);
    expect(toasts.some(([m]) => m.includes('locks in 1 minute'))).toBe(true);
    expect(st.vaultOpen).toBe(true);
    await vi.advanceTimersByTimeAsync(60_000);
    expect(st.vaultOpen).toBe(false);
    expect(st.vault.api_keys).toEqual([]);
  });

  it('neither warns nor locks early when the timeout is one minute', async () => {
    vi.useFakeTimers();
    st.store = new LocalVaultStore();
    Settings.set('autoLockMinutes', 1);
    resetLock();
    await vi.advanceTimersByTimeAsync(59_000);
    expect(toasts).toHaveLength(0);
    expect(st.vaultOpen).toBe(true);
  });

  it('is restarted by activity, so a person who is working is never locked out', async () => {
    vi.useFakeTimers();
    st.store = new LocalVaultStore();
    Settings.set('autoLockMinutes', 2);
    resetLock();
    await vi.advanceTimersByTimeAsync(90_000);
    document.dispatchEvent(new MouseEvent('mousemove'));
    await vi.advanceTimersByTimeAsync(90_000);
    expect(st.vaultOpen).toBe(true);
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'a' }));
    await vi.advanceTimersByTimeAsync(119_000);
    expect(st.vaultOpen).toBe(true);
    await vi.advanceTimersByTimeAsync(2000);
    expect(st.vaultOpen).toBe(false);
  });

  it('is paused while the vault is served to the LAN', () => {
    st.lanServerRunning = true;
    Settings.set('autoLockMinutes', 5);
    resetLock();
    expect($('lock-status').textContent).toBe('Auto-lock: paused (serving LAN)');
    expect(st.lockTimer).toBeNull();
    st.lanServerRunning = false;
  });
});

describe('locking', () => {
  it('locks the store it holds, local or remote, and shows the lock screen for the desktop one', async () => {
    const desktop = tauriStore(async () => true);
    const desktopLock = vi.fn().mockResolvedValue(undefined);
    desktop.lock = desktopLock;
    st.store = desktop;
    await lockVault('auto');
    expect(desktopLock).toHaveBeenCalled();
    expect($('relock-overlay').classList.contains('open')).toBe(true);
    expect($('relock-reason').textContent).toContain('Auto-locked after inactivity');

    const remote = new RemoteVaultStore('http://localhost:1');
    const remoteLock = vi.fn().mockResolvedValue(undefined);
    remote.lock = remoteLock;
    st.store = remote;
    st.vaultOpen = true;
    await lockVault('manual');
    expect(remoteLock).toHaveBeenCalled();
    expect($('load-banner').style.display).toBe('flex');
    expect(toasts.some(([m]) => m === 'Vault locked')).toBe(true);
  });

  it('swallows a failing backend lock rather than leaving the screen unlocked', async () => {
    const desktop = tauriStore(async () => true);
    desktop.lock = vi.fn().mockRejectedValue(new Error('gone'));
    st.store = desktop;
    await lockVault('manual');
    expect(st.vaultOpen).toBe(false);
    expect(st.vault.api_keys).toEqual([]);
  });

  it('every lock reason has its own wording', () => {
    st.store = tauriStore(async () => true);
    for (const [reason, text] of [
      ['visibility', 'locked while the app was in the background'],
      ['manual', 'Vault locked. Enter your master password'],
      ['switch', 'Switched to the local vault'],
    ] as const) {
      showRelockScreen(reason);
      expect($('relock-reason').textContent).toContain(text);
    }
  });
});

describe('relock screen', () => {
  const submit = async (pw: string) => {
    $<HTMLInputElement>('relock-password').value = pw;
    $('relock-submit-btn').click();
    await flush();
  };

  it('refuses an empty password without asking the backend', async () => {
    const unlock = vi.fn(async () => true);
    st.store = tauriStore(unlock);
    showRelockScreen('manual');
    await submit('');
    expect(unlock).not.toHaveBeenCalled();
    expect($('relock-error').textContent).toBe('Password cannot be empty.');
  });

  it('shows the backend message for a wrong password and lets the person try again', async () => {
    st.store = tauriStore(async () => {
      throw new Error('Wrong password');
    });
    showRelockScreen('manual');
    await submit('nope');
    expect($('relock-error').textContent).toBe('Wrong password');
    expect($<HTMLButtonElement>('relock-submit-btn').disabled).toBe(false);
    expect($('relock-submit-btn').textContent).toBe('Unlock');
    expect(finished).toBe(0);
  });

  it('unlocks on the right password, reopens the app and re-arms auto-lock; Enter submits too', async () => {
    Settings.set('autoLockMinutes', 5);
    const unlock = vi.fn(async () => true);
    st.store = tauriStore(unlock);
    showRelockScreen('manual');
    $<HTMLInputElement>('relock-password').value = 'right';
    $('relock-password').dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter' }));
    await flush();
    expect(unlock).toHaveBeenCalledWith('right');
    expect($('relock-overlay').classList.contains('open')).toBe(false);
    expect(finished).toBe(1);
    expect($('lock-status').textContent).toBe('Auto-lock: 5min');
    clearTimeout(st.lockTimer!);
  });
});

describe('first run: creating the master password', () => {
  const fill = (pw: string, confirm: string) => {
    $<HTMLInputElement>('unlock-server').value = '';
    $<HTMLInputElement>('unlock-password').value = pw;
    $<HTMLInputElement>('unlock-confirm').value = confirm;
    $('unlock-submit-btn').click();
  };

  it('asks for a confirmation and says there is no recovery', () => {
    st.store = tauriStore(async () => true);
    showUnlockModal(true);
    expect($('unlock-title').textContent).toBe('Create Master Password');
    expect($('unlock-subtitle').textContent).toContain('no recovery');
    expect($('unlock-confirm-group').style.display).toBe('block');
    expect($('unlock-submit-btn').textContent).toBe('Create Vault');
  });

  it('requires 12 characters, a matching confirmation, and a non-empty password', async () => {
    const unlock = vi.fn(async () => true);
    st.store = tauriStore(unlock);
    showUnlockModal(true);
    fill('', '');
    await flush();
    expect($('unlock-error').textContent).toBe('Password cannot be empty.');
    fill('short-pw', 'short-pw');
    await flush();
    expect($('unlock-error').textContent).toContain('at least 12 characters');
    fill('a-long-enough-password', 'a-different-one-here');
    await flush();
    expect($('unlock-error').textContent).toBe('Passwords do not match.');
    expect(unlock).not.toHaveBeenCalled();
  });

  it('creates the vault and opens the app with a good password', async () => {
    const unlock = vi.fn(async () => true);
    st.store = tauriStore(unlock);
    showUnlockModal(true);
    fill('a-long-enough-password', 'a-long-enough-password');
    await flush();
    expect(unlock).toHaveBeenCalledWith('a-long-enough-password');
    expect(finished).toBe(1);
    expect($('unlock-overlay').classList.contains('open')).toBe(false);
  });

  it('does not enforce the length rule when unlocking an existing vault', async () => {
    const unlock = vi.fn(async () => true);
    st.store = tauriStore(unlock);
    showUnlockModal(false);
    expect($('unlock-title').textContent).toBe('Unlock Vault');
    expect($('unlock-confirm-group').style.display).toBe('none');
    $<HTMLInputElement>('unlock-password').value = 'old';
    $('unlock-submit-btn').click();
    await flush();
    expect(unlock).toHaveBeenCalledWith('old');
  });

  it('reports a failed unlock and re-enables the button; falls back to "Wrong password" without a message', async () => {
    st.store = tauriStore(async () => {
      throw new Error('');
    });
    showUnlockModal(false);
    $<HTMLInputElement>('unlock-password').value = 'bad';
    $('unlock-submit-btn').click();
    await flush();
    expect($('unlock-error').textContent).toBe('Wrong password');
    expect($<HTMLButtonElement>('unlock-submit-btn').disabled).toBe(false);
  });
});

describe('connecting to a remote vault from the lock screen', () => {
  const connect = async (pw: string, url = 'http://vault.test/', user = '') => {
    $<HTMLInputElement>('unlock-server').value = url;
    $<HTMLInputElement>('unlock-username').value = user;
    $<HTMLInputElement>('unlock-password').value = pw;
    $('unlock-submit-btn').click();
    await flush(20);
  };

  beforeEach(() => {
    st.store = new LocalVaultStore();
  });

  it('switches the wording to a remote connection as soon as a server is typed', () => {
    showUnlockModal(false);
    $<HTMLInputElement>('unlock-server').value = 'http://x';
    $('unlock-server').dispatchEvent(new Event('input'));
    expect($('unlock-title').textContent).toBe('Connect to Remote Vault');
    expect($('unlock-submit-btn').textContent).toBe('Connect');
    expect($('unlock-confirm-group').style.display).toBe('none');
  });

  it('connects as owner, pins the certificate, remembers the server and opens the app', async () => {
    vi.spyOn(RemoteVaultStore.prototype, 'unlock').mockResolvedValue(true);
    vi.spyOn(RemoteVaultStore.prototype, 'getStatus').mockResolvedValue({
      cert_fingerprint: 'AA:BB',
    } as never);
    showUnlockModal(false);
    await connect('pw');
    expect(st.store).toBeInstanceOf(RemoteVaultStore);
    expect((st.store as RemoteVaultStore).baseUrl).toBe('http://vault.test');
    expect((st.store as RemoteVaultStore).fingerprint).toBe('AA:BB');
    expect(Settings.get('remoteSaved')![0]).toMatchObject({
      url: 'http://vault.test',
      certFingerprint: 'AA:BB',
    });
    expect(Settings.get('remoteSaved')![0].lastConnectedAt).toBeTruthy();
    expect(finished).toBe(1);
    expect($('unlock-overlay').classList.contains('open')).toBe(false);
  });

  it('connects as a named user and still opens when the status call fails', async () => {
    const auth = vi.spyOn(RemoteVaultStore.prototype, 'authUser').mockResolvedValue(true);
    vi.spyOn(RemoteVaultStore.prototype, 'getStatus').mockRejectedValue(new Error('no status'));
    showUnlockModal(false);
    await connect('pw', 'http://vault.test', 'joy');
    expect(auth).toHaveBeenCalledWith('joy', 'pw');
    expect(finished).toBe(1);
    expect(Settings.get('remoteSaved')![0].certFingerprint).toBeUndefined();
  });

  it('says authentication failed and stays on the lock screen', async () => {
    vi.spyOn(RemoteVaultStore.prototype, 'unlock').mockResolvedValue(false);
    showUnlockModal(false);
    await connect('wrong');
    expect($('unlock-error').textContent).toContain('Authentication failed');
    expect($<HTMLButtonElement>('unlock-submit-btn').disabled).toBe(false);
    expect($('unlock-submit-btn').textContent).toBe('Connect');
    expect(Settings.get('remoteSaved')).toEqual([]);
    expect(finished).toBe(0);
  });

  it('shows a network failure, and never sends an empty password', async () => {
    const unlock = vi
      .spyOn(RemoteVaultStore.prototype, 'unlock')
      .mockRejectedValue(new Error('connection refused'));
    showUnlockModal(false);
    await connect('');
    expect(unlock).not.toHaveBeenCalled();
    expect($('unlock-error').textContent).toBe('Password cannot be empty.');
    await connect('pw');
    expect($('unlock-error').textContent).toContain('connection refused');
  });
});

describe('the lock screen’s remembered servers', () => {
  const server = (over: object = {}) => ({
    id: 'r1',
    name: 'Prod',
    url: 'https://prod.test',
    username: 'alice',
    lastConnectedAt: new Date(Date.now() - 3600_000).toISOString(),
    ...over,
  });

  beforeEach(() => {
    st.store = new LocalVaultStore();
  });

  it('pre-fills the most recently connected server and its user', () => {
    Settings.set('remoteSaved', [
      server({ id: 'old', url: 'https://old.test', lastConnectedAt: '2020-01-01T00:00:00Z' }),
      server(),
    ] as never);
    showUnlockModal(false);
    expect($<HTMLInputElement>('unlock-server').value).toBe('https://prod.test');
    expect($<HTMLInputElement>('unlock-username').value).toBe('alice');
  });

  it('falls back to the legacy single server setting, and to a connected remote’s own address', () => {
    Settings.set('remoteSaved', []);
    Settings.set('remote', { enabled: true, serverUrl: 'https://legacy.test' });
    showUnlockModal(false);
    expect($<HTMLInputElement>('unlock-server').value).toBe('https://legacy.test');
    st.store = new RemoteVaultStore('https://live.test');
    showUnlockModal(false);
    expect($<HTMLInputElement>('unlock-server').value).toBe('https://live.test');
  });

  it('the picker lists Local Vault and each server; choosing one fills the address and user, choosing Local clears them', () => {
    Settings.set('remoteSaved', [server()] as never);
    showUnlockModal(false);
    $('unlock-server-recent').click();
    const items = () => [...document.querySelectorAll<HTMLElement>('#dropdown .dropdown-item')];
    // Regression: the label was a plain string, so the menu showed the text "&nbsp;&nbsp;".
    expect(items().map((i) => i.textContent!.replace(/\s+/g, ' ').trim())).toEqual([
      'Local Vault',
      expect.stringContaining('Prod'),
    ]);
    items()[0].click();
    expect($<HTMLInputElement>('unlock-server').value).toBe('');
    expect($<HTMLInputElement>('unlock-username').value).toBe('');
    $('unlock-server-recent').click();
    items()[1].click();
    expect($<HTMLInputElement>('unlock-server').value).toBe('https://prod.test');
    expect($<HTMLInputElement>('unlock-username').value).toBe('alice');
    expect($('unlock-title').textContent).toBe('Connect to Remote Vault');
  });

  it('with no servers yet the picker says so', () => {
    Settings.set('remoteSaved', []);
    Settings.set('remote', { enabled: false, serverUrl: '' });
    showUnlockModal(false);
    $('unlock-server-recent').click();
    expect(document.querySelector('#dropdown')!.textContent).toContain('No servers connected yet');
    document.querySelectorAll<HTMLElement>('#dropdown .dropdown-item').forEach((i) => i.click());
  });

  it('Enter in the password or confirmation box submits', async () => {
    const unlock = vi.fn(async () => true);
    st.store = tauriStore(unlock);
    showUnlockModal(false);
    $<HTMLInputElement>('unlock-password').value = 'a long enough password';
    $('unlock-password').dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter' }));
    await flush();
    expect(unlock).toHaveBeenCalledTimes(1);
    st.store = tauriStore(unlock);
    showUnlockModal(true);
    $<HTMLInputElement>('unlock-password').value = 'a long enough password';
    $<HTMLInputElement>('unlock-confirm').value = 'a long enough password';
    $('unlock-confirm').dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter' }));
    await flush();
    expect(unlock).toHaveBeenCalledTimes(2);
    $('unlock-password').dispatchEvent(new KeyboardEvent('keydown', { key: 'a' }));
    $('unlock-confirm').dispatchEvent(new KeyboardEvent('keydown', { key: 'a' }));
    expect(unlock).toHaveBeenCalledTimes(2);
  });
});

describe('locking while the vault is served to the LAN', () => {
  it('asks before stopping the server, and stays unlocked if the answer is no', async () => {
    vi.doMock('../src/ts/lan', () => ({ confirmStopForLock: async () => false }));
    st.store = new LocalVaultStore();
    st.lanServerRunning = true;
    st.vaultOpen = true;
    await lockVault('manual');
    expect(st.vaultOpen).toBe(true);
    expect(st.vault.api_keys).toHaveLength(1);
    vi.doMock('../src/ts/lan', () => ({ confirmStopForLock: async () => true }));
    await lockVault('manual');
    expect(st.vaultOpen).toBe(false);
    vi.doUnmock('../src/ts/lan');
    st.lanServerRunning = false;
  });
});
