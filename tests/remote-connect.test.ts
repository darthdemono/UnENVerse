// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
/**
 * Connecting to a remote vault from the Remote panel.
 *
 * The properties that matter are about trust and about not mixing vaults up:
 * a certificate is pinned only after the person has seen it and before a
 * password is typed, a changed certificate is never silently re-pinned, the
 * local vault's key does not stay in memory behind a remote session unless the
 * person opted in, and nothing from the vault being left is shown in the next.
 */
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { loadRealIndexHtml } from './helpers';

const toasts: [string, string?][] = [];
const confirms: string[] = [];
let confirmAnswer = true;
let passwordAnswer: string | null = 'pw';
let codeAnswer: string | null = '123456';
const passwordPrompts: string[] = [];
vi.mock('../src/ts/utils', async (importOriginal) => {
  const real = await importOriginal<typeof import('../src/ts/utils')>();
  return {
    ...real,
    showToast: (m: string, k?: string) => toasts.push([m, k]),
    showConfirm: async (m: string) => {
      confirms.push(m);
      return confirmAnswer;
    },
    showPasswordPrompt: async (m: string) => {
      passwordPrompts.push(m);
      return passwordAnswer;
    },
    showPrompt: async () => codeAnswer,
  };
});

const $ = <T extends HTMLElement = HTMLElement>(id: string) => document.getElementById(id) as T;
const flush = (ms = 30) => new Promise((r) => setTimeout(r, ms));

let st: (typeof import('../src/ts/state'))['st'];
let Settings: (typeof import('../src/ts/state'))['Settings'];
let RemoteVaultStore: (typeof import('../src/ts/state'))['RemoteVaultStore'];
let panel: typeof import('../src/ts/remote-panel');
let invoked: [string, any][] = [];
let finished = 0;
let tauriHandler: (cmd: string, args: any) => unknown;

async function boot(opts: { tauri?: boolean } = {}) {
  vi.resetModules();
  loadRealIndexHtml();
  invoked = [];
  if (opts.tauri) {
    (window as any).__TAURI__ = {
      core: {
        invoke: async (cmd: string, args: any) => {
          invoked.push([cmd, args]);
          const r = tauriHandler(cmd, args);
          if (r instanceof Error) throw r;
          return r;
        },
      },
    };
  } else delete (window as any).__TAURI__;
  ({ st, Settings, RemoteVaultStore } = await import('../src/ts/state'));
  await import('../src/ts/render');
  panel = await import('../src/ts/remote-panel');
  const { resetState, makeVault, makeEntry } = await import('./helpers');
  resetState(st);
  st.vault = makeVault({ api_keys: [makeEntry({ id: 'local', provider: 'LocalSecret' })] });
  Settings.set('remoteSaved', []);
  Settings.set('remote', { enabled: false, serverUrl: '' });
  Settings.set('keepLocalUnlocked', false);
  finished = 0;
  panel.setRemoteFinishInitFn(async () => {
    finished++;
  });
}

const saveRemote = (over: object = {}) => {
  const cfg = { id: 'r1', name: 'Prod', url: 'http://vault.test', username: '', ...over };
  Settings.set('remoteSaved', [cfg]);
  panel.initRemotePanel();
  return cfg;
};
const select = (id = 'r1') => {
  $('remote-panel-list').querySelector<HTMLElement>(`[data-remote-id="${id}"]`)!.click();
};

beforeEach(() => {
  toasts.length = 0;
  confirms.length = 0;
  passwordPrompts.length = 0;
  confirmAnswer = true;
  passwordAnswer = 'pw';
  tauriHandler = () => null;
});
afterEach(async () => {
  // Let anything a test left in flight (a disconnect repainting the panel, a status
  // refresh) finish before the next test builds its own document.
  await flush(120);
  vi.restoreAllMocks();
  delete (window as any).__TAURI__;
});

describe('the saved-remotes list', () => {
  it('says there are none, lists saved ones with their address, and marks the connected one', async () => {
    await boot();
    panel.initRemotePanel();
    expect($('remote-panel-list').textContent).toContain('No saved remotes');
    saveRemote({ lastConnectedAt: new Date(Date.now() - 3 * 3600_000).toISOString() });
    panel.renderRemotePanel();
    const t = $('remote-panel-list').textContent!.replace(/\s+/g, ' ');
    expect(t).toContain('Prod');
    expect(t).toContain('http://vault.test');
    expect(t).toContain('3 hours ago');
    expect($('remote-panel-list').querySelector('.remote-connected-dot')).toBeNull();
    expect($('remote-detail-host').textContent).toContain('Select a remote vault or add a new one');
  });

  it('selecting a remote shows it as disconnected, with editable details', async () => {
    await boot();
    saveRemote();
    select();
    expect(st.activeRemoteId).toBe('r1');
    expect($('remote-detail-host').textContent).toContain('Disconnected');
    expect($('remote-detail-host').textContent).toContain('owner (master password)');
    expect($('remote-connect-btn')).toBeTruthy();
    expect($<HTMLInputElement>('re-url').value).toBe('http://vault.test');
    saveRemote({ username: 'alice' });
    select();
    expect($('remote-detail-host').textContent).toContain('alice');
  });

  it('escapes a hostile name', async () => {
    await boot();
    saveRemote({ name: '<img src=x onerror=alert(1)>' });
    select();
    expect($('remote-panel-list').querySelector('img')).toBeNull();
    expect($('remote-detail-host').querySelector('img')).toBeNull();
  });
});

describe('editing and removing a saved remote', () => {
  beforeEach(async () => {
    await boot();
    saveRemote();
    select();
  });

  it('saves a changed name, address and user, trimming the trailing slash', () => {
    $<HTMLInputElement>('re-name').value = ' Staging ';
    $<HTMLInputElement>('re-url').value = 'http://staging.test/';
    $<HTMLInputElement>('re-username').value = ' bob ';
    $('re-save-btn').click();
    expect(Settings.get('remoteSaved')![0]).toMatchObject({
      id: 'r1',
      name: 'Staging',
      url: 'http://staging.test',
      username: 'bob',
    });
    expect(toasts).toContainEqual(['Saved', 'ok']);
  });

  it('refuses a blank name or address', () => {
    $<HTMLInputElement>('re-name').value = '';
    $('re-save-btn').click();
    expect(toasts).toContainEqual(['Name and URL are required', 'err']);
    expect(Settings.get('remoteSaved')![0].name).toBe('Prod');
  });

  it('removing asks first and forgets the remote', async () => {
    confirmAnswer = false;
    $('remote-delete-btn').click();
    await flush();
    expect(Settings.get('remoteSaved')).toHaveLength(1);
    confirmAnswer = true;
    $('remote-delete-btn').click();
    await flush();
    expect(confirms[confirms.length - 1]).toContain('Remove "Prod" from saved remotes?');
    expect(Settings.get('remoteSaved')).toEqual([]);
    expect(st.activeRemoteId).toBeNull();
  });
});

describe('adding a remote', () => {
  it('needs a name and an address, and selects what it saved', async () => {
    await boot();
    panel.initRemotePanel();
    $('add-remote-btn').click();
    $('add-remote-confirm').click();
    expect(toasts).toContainEqual(['Name is required', 'err']);
    $<HTMLInputElement>('add-remote-name').value = 'Home';
    $('add-remote-confirm').click();
    expect(toasts).toContainEqual(['URL is required', 'err']);
    $<HTMLInputElement>('add-remote-url').value = ' http://home.test:8743/ ';
    $<HTMLInputElement>('add-remote-username').value = ' ann ';
    $('add-remote-confirm').click();
    expect(Settings.get('remoteSaved')![0]).toMatchObject({
      name: 'Home',
      url: 'http://home.test:8743',
      username: 'ann',
    });
    expect(st.activeRemoteId).toBe(Settings.get('remoteSaved')![0].id);
    expect(toasts).toContainEqual(['"Home" saved', 'ok']);
  });

  it('Cancel puts the hint back', async () => {
    await boot();
    panel.initRemotePanel();
    $('add-remote-btn').click();
    $('add-remote-cancel').click();
    expect($('remote-detail-host').textContent).toContain('Select a remote vault or add a new one');
  });

  it('tests reachability before saving: needs an address, reports reachable, with or without a vault, and unreachable', async () => {
    await boot();
    panel.initRemotePanel();
    $('add-remote-btn').click();
    $('add-remote-test').click();
    await flush();
    expect(toasts).toContainEqual(['Enter a URL first', 'err']);
    $<HTMLInputElement>('add-remote-url').value = 'http://x.test/';
    const status = vi.spyOn(RemoteVaultStore.prototype, 'getStatus');
    status.mockResolvedValueOnce({
      vault_exists: true,
      unlocked: false,
      cert_fingerprint: 'AB',
    } as never);
    $('add-remote-test').click();
    await flush();
    expect($('add-test-status').textContent).toBe('✓ Reachable · TLS ✓');
    status.mockResolvedValueOnce({ vault_exists: false } as never);
    $('add-remote-test').click();
    await flush();
    expect($('add-test-status').textContent).toBe('✓ Reachable (no vault yet)');
    status.mockRejectedValueOnce(new Error('refused'));
    $('add-remote-test').click();
    await flush();
    expect($('add-test-status').textContent).toBe('✗ Unreachable');
  });
});

describe('testing a saved remote', () => {
  it('reports the vault state, a missing vault, an unreachable server and a missing address', async () => {
    await boot();
    saveRemote();
    select();
    const status = vi.spyOn(RemoteVaultStore.prototype, 'getStatus');
    status.mockResolvedValueOnce({ vault_exists: true, unlocked: true } as never);
    $('re-test-btn').click();
    await flush();
    expect($('re-test-status').textContent).toBe('✓ Server OK · vault unlocked');
    status.mockResolvedValueOnce({
      vault_exists: true,
      unlocked: false,
      cert_fingerprint: 'AB',
    } as never);
    $('re-test-btn').click();
    await flush();
    expect($('re-test-status').textContent).toBe('✓ Server OK · vault locked · TLS ✓');
    status.mockResolvedValueOnce({ vault_exists: false } as never);
    $('re-test-btn').click();
    await flush();
    expect($('re-test-status').textContent).toBe('✓ Server OK · no vault yet');
    status.mockRejectedValueOnce(new Error('x'));
    $('re-test-btn').click();
    await flush();
    expect($('re-test-status').textContent).toBe('✗ Unreachable');
    $<HTMLInputElement>('re-url').value = '';
    $('re-test-btn').click();
    await flush();
    expect(toasts).toContainEqual(['Enter a URL first', 'err']);
  });

  it('an https address with no pin asks to trust its certificate first, and declining says so', async () => {
    tauriHandler = (cmd) => (cmd === 'probe_cert_fingerprint' ? 'aabbcc' : null);
    await boot({ tauri: true });
    saveRemote({ url: 'https://vault.test' });
    select();
    confirmAnswer = false;
    $('re-test-btn').click();
    await flush();
    expect($('re-test-status').textContent).toBe('✗ Certificate not trusted');
    expect(confirms[0]).toContain('AA:BB:CC');
    expect(Settings.get('remoteSaved')![0].certFingerprint).toBeUndefined();
    confirmAnswer = true;
    vi.spyOn(RemoteVaultStore.prototype, 'getStatus').mockResolvedValue({
      vault_exists: true,
      unlocked: true,
    } as never);
    $('re-test-btn').click();
    await flush();
    expect(Settings.get('remoteSaved')![0].certFingerprint).toBe('aabbcc');
    expect($('re-test-status').textContent).toContain('✓ Server OK');
  });
});

describe('connecting', { timeout: 30000 }, () => {
  const connect = async () => {
    $('remote-connect-btn').click();
    // Connecting loads a few modules lazily; wait for it to settle rather than guess a delay.
    await flush(150);
  };

  it('as owner: asks for the master password, switches vault, clears the old view, locks the local key, and loads', async () => {
    tauriHandler = () => null;
    await boot({ tauri: true });
    saveRemote();
    select();
    st.expanded.add('x');
    st.currentSelectedProjectIds = ['some-local-project'];
    vi.spyOn(RemoteVaultStore.prototype, 'unlock').mockResolvedValue(true);
    vi.spyOn(RemoteVaultStore.prototype, 'getStatus').mockResolvedValue({
      cert_fingerprint: undefined,
    } as never);
    await connect();
    expect(passwordPrompts[0]).toBe('Master password on Prod:');
    expect(st.store).toBeInstanceOf(RemoteVaultStore);
    expect(st.activeRemoteId).toBe('r1');
    expect(Settings.get('remote')).toEqual({ enabled: true, serverUrl: 'http://vault.test' });
    expect(st.currentSelectedProjectIds).toEqual(['Universal']);
    expect(invoked.some(([c]) => c === 'lock_vault')).toBe(true);
    expect($('vault-name').textContent).toBe('Prod');
    expect(toasts).toContainEqual(['Connected to Prod', 'ok']);
    expect(Settings.get('remoteSaved')![0].lastConnectedAt).toBeTruthy();
    expect(finished).toBe(1);
    expect(st.vault.api_keys.map((e) => e.provider)).toEqual(['LocalSecret']);
  });

  it('keeps the local key resident only when the person chose that', async () => {
    await boot({ tauri: true });
    Settings.set('keepLocalUnlocked', true);
    saveRemote();
    select();
    vi.spyOn(RemoteVaultStore.prototype, 'unlock').mockResolvedValue(true);
    vi.spyOn(RemoteVaultStore.prototype, 'getStatus').mockResolvedValue({} as never);
    await connect();
    expect(invoked.some(([c]) => c === 'lock_vault')).toBe(false);
  });

  it('a lock that fails because the local vault was never unlocked is not an error', async () => {
    tauriHandler = (cmd) => (cmd === 'lock_vault' ? new Error('not unlocked') : null);
    await boot({ tauri: true });
    saveRemote();
    select();
    vi.spyOn(RemoteVaultStore.prototype, 'unlock').mockResolvedValue(true);
    vi.spyOn(RemoteVaultStore.prototype, 'getStatus').mockResolvedValue({} as never);
    await connect();
    expect(toasts).toContainEqual(['Connected to Prod', 'ok']);
  });

  it('as a named user: uses their password and signs in as them', async () => {
    await boot();
    saveRemote({ username: 'alice' });
    select();
    const auth = vi.spyOn(RemoteVaultStore.prototype, 'authUser').mockResolvedValue(true);
    vi.spyOn(RemoteVaultStore.prototype, 'getStatus').mockResolvedValue({} as never);
    await connect();
    expect(passwordPrompts[0]).toBe('Password for "alice" on Prod:');
    expect(auth).toHaveBeenCalledWith('alice', 'pw');
  });

  it('asks for a two-factor code when the server wants one, and gives up quietly if it is declined', async () => {
    await boot();
    saveRemote({ username: 'alice' });
    select();
    const { TotpRequiredError } = await import('../src/ts/state');
    const auth = vi
      .spyOn(RemoteVaultStore.prototype, 'authUser')
      .mockRejectedValueOnce(new TotpRequiredError())
      .mockResolvedValueOnce(true);
    vi.spyOn(RemoteVaultStore.prototype, 'getStatus').mockResolvedValue({} as never);
    await connect();
    expect(auth).toHaveBeenLastCalledWith('alice', 'pw', '123456');
    expect(toasts).toContainEqual(['Connected to Prod', 'ok']);
  });

  it('declining the two-factor prompt is no login, and says nothing about a failed one', async () => {
    await boot();
    saveRemote({ username: 'alice' });
    select();
    codeAnswer = null;
    const { TotpRequiredError } = await import('../src/ts/state');
    const auth = vi
      .spyOn(RemoteVaultStore.prototype, 'authUser')
      .mockRejectedValue(new TotpRequiredError());
    await connect();
    codeAnswer = '123456';
    expect(auth).toHaveBeenCalledTimes(1);
    expect(toasts.filter(([m]) => m === 'Authentication failed')).toHaveLength(1);
    expect(finished).toBe(0);
  });

  it('any other sign-in error is passed on rather than swallowed', async () => {
    await boot();
    const remote = new RemoteVaultStore('http://vault.test');
    vi.spyOn(remote, 'authUser').mockRejectedValue(new Error('server down'));
    await expect(panel.authUserInteractive(remote, 'alice', 'pw')).rejects.toThrow('server down');
  });

  it('a cancelled password prompt connects to nothing', async () => {
    await boot();
    saveRemote();
    select();
    passwordAnswer = null;
    const unlock = vi.spyOn(RemoteVaultStore.prototype, 'unlock');
    await connect();
    expect(unlock).not.toHaveBeenCalled();
    expect(st.store).not.toBeInstanceOf(RemoteVaultStore);
  });

  it('a wrong password leaves the current vault alone and says so', async () => {
    await boot();
    saveRemote();
    select();
    vi.spyOn(RemoteVaultStore.prototype, 'unlock').mockResolvedValue(false);
    await connect();
    expect(toasts).toContainEqual(['Authentication failed', 'err']);
    expect(st.store).not.toBeInstanceOf(RemoteVaultStore);
    expect(finished).toBe(0);
  });

  it('a network failure names the reason', async () => {
    await boot();
    saveRemote();
    select();
    vi.spyOn(RemoteVaultStore.prototype, 'unlock').mockRejectedValue(new Error('connection reset'));
    await connect();
    expect(
      toasts.some(([m, k]) => m === 'Connection failed: connection reset' && k === 'err'),
    ).toBe(true);
  });

  it('an https server with no pin is trusted only after the person sees its fingerprint, before any password is typed', async () => {
    tauriHandler = (cmd) => (cmd === 'probe_cert_fingerprint' ? '0a1b2c' : null);
    await boot({ tauri: true });
    saveRemote({ url: 'https://vault.test' });
    select();
    confirmAnswer = false;
    await connect();
    expect(passwordPrompts).toHaveLength(0);
    expect(Settings.get('remoteSaved')![0].certFingerprint).toBeUndefined();
    confirmAnswer = true;
    vi.spyOn(RemoteVaultStore.prototype, 'unlock').mockResolvedValue(true);
    vi.spyOn(RemoteVaultStore.prototype, 'getStatus').mockResolvedValue({
      cert_fingerprint: '0a1b2c',
    } as never);
    await connect();
    expect(confirms[0]).toContain('0A:1B:2C');
    expect(Settings.get('remoteSaved')![0].certFingerprint).toBe('0a1b2c');
    expect(passwordPrompts).toHaveLength(1);
    expect((st.store as InstanceType<typeof RemoteVaultStore>).fingerprint).toBe('0a1b2c');
  });

  it('a server that reports a different certificate than the pinned one is not re-pinned, and the person is warned', async () => {
    await boot();
    saveRemote({ url: 'https://vault.test', certFingerprint: 'pinned' });
    select();
    vi.spyOn(RemoteVaultStore.prototype, 'unlock').mockResolvedValue(true);
    vi.spyOn(RemoteVaultStore.prototype, 'getStatus').mockResolvedValue({
      cert_fingerprint: 'different',
    } as never);
    await connect();
    expect(toasts.some(([m]) => m.includes('does not match the pinned one'))).toBe(true);
    expect(Settings.get('remoteSaved')![0].certFingerprint).toBe('pinned');
  });

  it('the first fingerprint a server reports is remembered when none was pinned', async () => {
    await boot();
    saveRemote();
    select();
    vi.spyOn(RemoteVaultStore.prototype, 'unlock').mockResolvedValue(true);
    vi.spyOn(RemoteVaultStore.prototype, 'getStatus').mockResolvedValue({
      cert_fingerprint: 'first',
    } as never);
    await connect();
    expect(Settings.get('remoteSaved')![0].certFingerprint).toBe('first');
  });
});

describe('keeping a connection alive', { timeout: 30000 }, () => {
  it('pings the server every 90 seconds while connected, and stops once the vault is no longer remote', async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    try {
      await boot();
      saveRemote();
      select();
      const ping = vi
        .spyOn(RemoteVaultStore.prototype, 'ping')
        .mockResolvedValue(undefined as never);
      vi.spyOn(RemoteVaultStore.prototype, 'unlock').mockResolvedValue(true);
      vi.spyOn(RemoteVaultStore.prototype, 'getStatus').mockResolvedValue({} as never);
      vi.spyOn(RemoteVaultStore.prototype, 'load').mockResolvedValue(null as never);
      $('remote-connect-btn').click();
      await vi.advanceTimersByTimeAsync(200);
      expect(st.store).toBeInstanceOf(RemoteVaultStore);
      await vi.advanceTimersByTimeAsync(90_000);
      expect(ping).toHaveBeenCalledTimes(1);
      const { LocalVaultStore } = await import('../src/ts/state');
      st.store = new LocalVaultStore();
      await vi.advanceTimersByTimeAsync(90_000);
      expect(ping).toHaveBeenCalledTimes(1);
      await vi.advanceTimersByTimeAsync(180_000);
      expect(ping).toHaveBeenCalledTimes(1);
    } finally {
      vi.useRealTimers();
    }
  });
});

describe('connected state', { timeout: 30000 }, () => {
  const connected = async () => {
    await boot();
    saveRemote();
    select();
    vi.spyOn(RemoteVaultStore.prototype, 'unlock').mockResolvedValue(true);
    const status = vi
      .spyOn(RemoteVaultStore.prototype, 'getStatus')
      .mockResolvedValue({ vault_exists: true, unlocked: true } as never);
    $('remote-connect-btn').click();
    await vi.waitFor(
      () => expect(document.getElementById('remote-disconnect-btn')).not.toBeNull(),
      { timeout: 10000 },
    );
    return status;
  };

  it('shows the connected badge, the live status and a working Refresh', async () => {
    const status = await connected();
    const t = () => $('remote-detail-host').textContent!.replace(/\s+/g, ' ');
    expect(t()).toContain('Connected');
    expect(t()).toContain('Online');
    expect(t()).toContain('Exists');
    expect(t()).toContain('Unlocked');
    expect($('remote-panel-list').querySelector('.remote-connected-dot')).toBeTruthy();
    status.mockResolvedValue({ vault_exists: false, unlocked: false } as never);
    $('remote-refresh-status-btn').click();
    await flush();
    expect(t()).toContain('Not created');
    expect(t()).toContain('Locked');
    status.mockRejectedValue(new Error('gone'));
    $('remote-refresh-status-btn').click();
    await flush();
    expect(t()).toContain('Could not reach server.');
    $('remote-refresh-status-btn').click();
    await flush();
  });

  it('Disconnect returns to the local vault and shows the relock screen when it is still locked', async () => {
    tauriHandler = (cmd) => (cmd === 'vault_is_unlocked' ? false : null);
    await boot({ tauri: true });
    saveRemote();
    select();
    vi.spyOn(RemoteVaultStore.prototype, 'unlock').mockResolvedValue(true);
    vi.spyOn(RemoteVaultStore.prototype, 'getStatus').mockResolvedValue({} as never);
    vi.spyOn(RemoteVaultStore.prototype, 'lock').mockResolvedValue(undefined);
    $('remote-connect-btn').click();
    await vi.waitFor(
      () => expect(document.getElementById('remote-disconnect-btn')).not.toBeNull(),
      { timeout: 10000 },
    );
    $('remote-disconnect-btn').click();
    await vi.waitFor(() => expect($('relock-overlay').classList.contains('open')).toBe(true), {
      timeout: 10000,
    });
    expect(st.activeRemoteId).toBeNull();
    expect(st.vault.api_keys).toEqual([]);
    expect($('vault-name').textContent).toBe('Local Vault');
    expect($('relock-overlay').classList.contains('open')).toBe(true);
    expect($('relock-reason').textContent).toContain('Switched to the local vault');
  });

  it('removing the connected remote disconnects it first', async () => {
    await connected();
    vi.spyOn(RemoteVaultStore.prototype, 'lock').mockResolvedValue(undefined);
    $('remote-delete-btn').click();
    await flush(60);
    expect(st.store).not.toBeInstanceOf(RemoteVaultStore);
    expect(Settings.get('remoteSaved')).toEqual([]);
  });
});
