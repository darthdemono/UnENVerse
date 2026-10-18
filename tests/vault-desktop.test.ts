/**
 * Starting the app inside the desktop shell, and what happens to a vault as it
 * opens: the lock screen it starts on, how an older vault is brought up to date
 * (project, type, id and creation-date backfill), an empty or unreadable vault,
 * the "push your entries to an empty remote?" question, the first-run wizard,
 * and the pool copy button that needs the desktop's rotation state.
 */
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { loadRealIndexHtml } from './helpers';
import { entry } from './app-boot';

const toasts: [string, string?][] = [];
const copied: string[] = [];
let confirmAnswer = true;
vi.mock('../src/ts/utils', async (importOriginal) => {
  const real = await importOriginal<typeof import('../src/ts/utils')>();
  return {
    ...real,
    showToast: (m: string, k?: string) => toasts.push([m, k]),
    showConfirm: async () => confirmAnswer,
    showPrompt: async () => null,
    clipboardWrite: async (t: string) => void copied.push(t),
  };
});

type Calls = [string, any][];
let calls: Calls = [];
let backend: Record<string, (args: any) => unknown> = {};
let finishInit: (() => Promise<void>) | null = null;
vi.mock('../src/ts/lock', async (importOriginal) => {
  const real = await importOriginal<typeof import('../src/ts/lock')>();
  return {
    ...real,
    setFinishInitFn: (fn: () => Promise<void>) => {
      finishInit = fn;
      real.setFinishInitFn(fn);
    },
  };
});

const $ = <T extends HTMLElement = HTMLElement>(id: string) => document.getElementById(id) as T;
const flush = (ms = 40) => new Promise((r) => setTimeout(r, ms));

let state: typeof import('../src/ts/state');

async function bootDesktop(
  opts: {
    exists?: boolean;
    vault?: object | null | 'throw';
    settings?: object;
    audit?: object[];
    unlockOk?: boolean;
    path?: string;
  } = {},
) {
  vi.resetModules();
  loadRealIndexHtml();
  localStorage.clear();
  sessionStorage.clear();
  calls = [];
  finishInit = null;
  localStorage.setItem(
    'unenverse-settings',
    JSON.stringify({ onboardingCompleted: true, groupPools: false, ...opts.settings }),
  );
  backend = {
    vault_exists: () => opts.exists ?? true,
    get_vault_path: () => opts.path ?? '/home/me/io.unenverse/vault.db',
    unlock_vault: () => {
      if (opts.unlockOk === false) throw new Error('Wrong password');
      return true;
    },
    load_vault: () => {
      if (opts.vault === 'throw') throw new Error('disk on fire');
      return opts.vault === null ? null : { data: opts.vault, version: 'v1' };
    },
    save_vault: () => 'v2',
    get_audit_log: () => opts.audit ?? [],
  };
  (window as any).__TAURI__ = {
    core: {
      invoke: async (cmd: string, args: any) => {
        calls.push([cmd, args]);
        return backend[cmd]?.(args);
      },
    },
  };
  state = await import('../src/ts/state');
  await import('../src/ts/vault');
  await vi.waitFor(() => expect(finishInit).not.toBeNull(), { timeout: 15000 });
  await flush(80);
  return state;
}

async function unlock(password = 'a long enough master password') {
  $<HTMLInputElement>('unlock-password').value = password;
  $('unlock-submit-btn').click();
  await vi.waitFor(() => expect(state.st.vaultOpen).toBe(true), { timeout: 15000 });
  await flush(80);
}

const vault = (api_keys: object[], extra: object = {}) => ({
  api_keys,
  user_categories: ['infra'],
  projects: [{ id: 'Universal', name: 'Universal' }],
  ...extra,
});

beforeEach(() => {
  toasts.length = 0;
  copied.length = 0;
  confirmAnswer = true;
});

afterEach(() => {
  delete (window as any).__TAURI__;
});

describe('starting in the desktop app', { timeout: 40000 }, () => {
  it('opens on the unlock screen for an existing vault and shows where the vault file lives', async () => {
    await bootDesktop({ exists: true });
    expect($('unlock-overlay').classList.contains('open')).toBe(true);
    expect($('unlock-title').textContent).toBe('Unlock Vault');
    expect($('vault-name').textContent).toBe('Local Vault');
    expect($('lock-status').title).toBe('Vault file: /home/me/io.unenverse/vault.db');
    expect(document.documentElement.classList.contains('booting')).toBe(false);
  });

  it('shows no vault-file hint when the path is unknown', async () => {
    await bootDesktop({ exists: true, path: '' });
    expect($('lock-status').title).toBe('');
  });

  it('opens on "Create Master Password" when no vault exists yet', async () => {
    await bootDesktop({ exists: false });
    expect($('unlock-title').textContent).toBe('Create Master Password');
    expect($('unlock-confirm-group').style.display).toBe('block');
  });

  it('a wrong password leaves it locked and says so', async () => {
    await bootDesktop({ exists: true, unlockOk: false });
    $<HTMLInputElement>('unlock-password').value = 'not the master password';
    $('unlock-submit-btn').click();
    await flush(100);
    expect(state.st.vaultOpen).toBe(false);
    expect($('unlock-error').textContent).toContain('Wrong password');
    expect($('unlock-overlay').classList.contains('open')).toBe(true);
  });

  it('unlocking loads the vault and says how many keys it holds', async () => {
    await bootDesktop({ vault: vault([entry('a', 'Alpha'), entry('b', 'Beta')]) });
    await unlock();
    expect(toasts).toContainEqual(['Loaded 2 keys', 'ok']);
    expect(document.querySelectorAll('#card-grid .card')).toHaveLength(2);
  });
});

describe('opening an older or damaged vault', { timeout: 40000 }, () => {
  it('fills in what an older build did not write, and saves the result once', async () => {
    const old = [
      { id: 'o1', provider: 'Old one', api_key: 'k1-longenough' },
      { id: 'o2', provider: 'Old two', api_key: 'k2-longenough', projectIds: ['web'] },
      {
        id: 'o3',
        provider: 'Old three',
        api_key: 'k3-longenough',
        secretType: 'chunk',
        projectIds: ['Universal'],
      },
    ];
    await bootDesktop({ vault: { api_keys: old } });
    await unlock();
    const keys = state.st.vault.api_keys;
    expect(keys[0].projectIds).toEqual(['Universal']);
    expect(keys[0].secretType).toBe('api_key');
    expect(keys[1].projectIds).toEqual(['web', 'Universal']);
    expect(keys[2].secretType).toBe('env_var');
    expect(state.st.vault.user_categories).toEqual([]);
    expect(state.st.vault.projects.map((p) => p.id)).toContain('Universal');
    expect(calls.filter(([c]) => c === 'save_vault').length).toBeGreaterThanOrEqual(1);
  });

  it('gives an entry without an id one, and a duplicated id a new one', async () => {
    await bootDesktop({
      vault: vault([
        { ...entry('dup', 'A'), id: 'dup' },
        { ...entry('dup', 'B'), id: 'dup' },
        { ...entry('x', 'C'), id: undefined },
      ]),
    });
    await unlock();
    const ids = state.st.vault.api_keys.map((e) => e.id);
    expect(new Set(ids).size).toBe(3);
    expect(ids.every(Boolean)).toBe(true);
  });

  it('dates an entry from the audit log when its provider is unique', async () => {
    await bootDesktop({
      vault: vault([entry('a', 'Alpha'), entry('b', 'Beta')]),
      audit: [{ id: 1, action: 'add', entry_provider: 'Alpha', timestamp: '2026-01-02T03:04:05Z' }],
    });
    await unlock();
    expect(state.st.vault.api_keys[0].created_at).toBe('2026-01-02T03:04:05.000Z');
    expect(state.st.vault.api_keys[1].created_at).toBeUndefined();
  });

  it('opens anyway when the audit log cannot be read', async () => {
    await bootDesktop({ vault: vault([entry('a', 'Alpha')]) });
    backend.get_audit_log = () => {
      throw new Error('403');
    };
    await unlock();
    expect(state.st.vaultOpen).toBe(true);
    expect(state.st.vault.api_keys[0].created_at).toBeUndefined();
  });

  it('a vault with nothing in it opens empty rather than failing', async () => {
    await bootDesktop({ vault: {} });
    await unlock();
    expect(state.st.vault.api_keys).toEqual([]);
    expect(state.st.vault.projects.map((p) => p.id)).toEqual(['Universal']);
  });

  it('a vault that cannot be read leaves the load banner up and the app closed', async () => {
    await bootDesktop({ vault: 'throw' });
    $<HTMLInputElement>('unlock-password').value = 'a long enough master password';
    $('unlock-submit-btn').click();
    await flush(150);
    expect($('load-banner').style.display).toBe('flex');
    expect(state.st.vaultOpen).toBe(false);
  });

  it('a vault that comes back empty leaves the load banner up', async () => {
    await bootDesktop({ vault: null });
    $<HTMLInputElement>('unlock-password').value = 'a long enough master password';
    $('unlock-submit-btn').click();
    await flush(150);
    expect($('load-banner').style.display).toBe('flex');
    expect(state.st.vaultOpen).toBe(false);
  });
});

describe('opening an empty remote vault', { timeout: 40000 }, () => {
  async function openRemote(remoteKeys: object[]) {
    await bootDesktop({ vault: vault(remoteKeys) });
    state.st.vault.api_keys = [entry('mine', 'Mine') as never];
    const store = Object.assign(new state.RemoteVaultStore('https://x.test'), {
      load: async () => vault(remoteKeys),
      save: vi.fn(async () => {}),
    }) as never;
    state.st.store = store;
    return store as unknown as { save: ReturnType<typeof vi.fn> };
  }

  it('offers to push the local entries up, and keeps them when you agree', async () => {
    const store = await openRemote([]);
    await finishInit!();
    expect(store.save).toHaveBeenCalled();
    expect(state.st.vault.api_keys.map((e) => e.provider)).toEqual(['Mine']);
    expect(toasts).toContainEqual(['Pushed 1 entries to remote ✓', 'ok']);
  });

  it('empties the local copy instead when you decline', async () => {
    confirmAnswer = false;
    const store = await openRemote([]);
    await finishInit!();
    expect(store.save).not.toHaveBeenCalled();
    expect(state.st.vault.api_keys).toEqual([]);
    expect(state.st.vault.projects.map((p) => p.id)).toEqual(['Universal']);
  });

  it('does not ask when the remote has entries; it shows the remote ones', async () => {
    await openRemote([entry('r1', 'Remote one')]);
    await finishInit!();
    expect(state.st.vault.api_keys.map((e) => e.provider)).toEqual(['Remote one']);
  });
});

describe('the first-run wizard', { timeout: 40000 }, () => {
  // Each Next commits its step asynchronously before painting the following one.
  const toLastStep = async () => {
    for (let n = 0; n < 12 && !document.getElementById('onboard-add'); n++) {
      document.getElementById('onboard-next')!.click();
      await flush(10);
    }
  };

  it('opens over a freshly unlocked local vault; its last step can start an add', async () => {
    await bootDesktop({
      vault: vault([entry('a', 'Alpha')]),
      settings: { onboardingCompleted: false },
    });
    await unlock();
    expect(document.getElementById('onboard-title')).not.toBeNull();
    await toLastStep();
    $('onboard-add').click();
    await flush();
    expect(document.getElementById('onboard-title')).toBeNull();
    expect($('modal-overlay').classList.contains('open')).toBe(true);
  });

  it('its last step can also start an import, which opens the file picker', async () => {
    await bootDesktop({
      vault: vault([entry('a', 'Alpha')]),
      settings: { onboardingCompleted: false },
    });
    await unlock();
    await toLastStep();
    $('onboard-import').click();
    await flush();
    expect(document.getElementById('onboard-title')).toBeNull();
    expect(document.querySelector<HTMLInputElement>('input[type=file]')!.accept).toBe(
      '.json,.env,text/plain',
    );
  });

  it('does not open once it has been completed', async () => {
    await bootDesktop({ vault: vault([entry('a', 'Alpha')]) });
    await unlock();
    expect(document.getElementById('onboard-title')).toBeNull();
  });
});

describe('the pool Copy button in the desktop app', { timeout: 40000 }, () => {
  const pooled = () =>
    vault([
      entry('p1', 'First', { pool: 'main', api_key: 'first-secret-value' }),
      entry('p2', 'Second', { pool: 'main', api_key: 'second-secret-value' }),
    ]);
  const copyBtn = () =>
    document.querySelector<HTMLElement>('[data-action="pool-card-copy"][data-pool="main"]')!;

  it('copies whichever member the rotation lands on', async () => {
    backend.pool_next = () => 1;
    await bootDesktop({ vault: pooled(), settings: { groupPools: true } });
    backend.pool_next = () => 1;
    await unlock();
    copyBtn().click();
    await flush(80);
    expect(calls.some(([c]) => c === 'pool_next')).toBe(true);
    expect(copied.pop()).toContain('second-secret-value');
  });

  it('says every key is rate limited when the rotation has nothing to give', async () => {
    await bootDesktop({ vault: pooled(), settings: { groupPools: true } });
    backend.pool_next = () => null;
    await unlock();
    copyBtn().click();
    await flush(80);
    expect(toasts).toContainEqual(['Every key in "main" is rate limited', 'err']);
    expect(copied).toEqual([]);
  });

  it('shows how many members are ready and how many are cooling once the desktop answers', async () => {
    await bootDesktop({ vault: pooled(), settings: { groupPools: true } });
    backend.pool_state = () => [{ cooling: false }, { cooling: true }];
    await unlock();
    await vi.waitFor(() =>
      expect(document.querySelector('.pool-card-badge')!.textContent).toBe(
        'pool: 2 · 1 ready · 1 cooling',
      ),
    );
  });

  it('leaves out the cooling count when nothing is cooling', async () => {
    await bootDesktop({ vault: pooled(), settings: { groupPools: true } });
    backend.pool_state = () => [{ cooling: false }, { cooling: false }];
    await unlock();
    await vi.waitFor(() =>
      expect(document.querySelector('.pool-card-badge')!.textContent).toBe('pool: 2 · 2 ready'),
    );
  });

  it('keeps the plain member count when the desktop cannot say', async () => {
    await bootDesktop({ vault: pooled(), settings: { groupPools: true } });
    backend.pool_state = () => {
      throw new Error('state file unreadable');
    };
    await unlock();
    await flush(100);
    expect(document.querySelector('.pool-card-badge')!.textContent).toBe('pool: 2');
  });

  it('copies nothing when the member it landed on was deleted in the meantime', async () => {
    await bootDesktop({ vault: pooled(), settings: { groupPools: true } });
    await unlock();
    backend.pool_next = () => {
      state.st.vault.api_keys = state.st.vault.api_keys.filter((e) => e.id !== 'p1');
      return 0;
    };
    copyBtn().click();
    await flush(80);
    expect(copied).toEqual([]);
    expect(toasts.some(([, k]) => k === 'err')).toBe(false);
  });

  it('a button with no pool name does nothing', async () => {
    await bootDesktop({ vault: pooled(), settings: { groupPools: true } });
    await unlock();
    const btn = copyBtn();
    btn.dataset.pool = '';
    btn.click();
    await flush();
    expect(calls.some(([c]) => c === 'pool_next')).toBe(false);
  });
});

describe('copying an authenticator code', { timeout: 40000 }, () => {
  it('asks you to try again in a moment when the ticker has not painted a code yet', async () => {
    await bootDesktop({ vault: vault([entry('a', 'Alpha')]) });
    await unlock();
    document.body.insertAdjacentHTML(
      'beforeend',
      '<div data-totp-for="x"><button id="t-btn" data-action="copy-totp">copy</button></div>',
    );
    const grid = $('card-grid');
    grid.insertAdjacentHTML(
      'beforeend',
      '<div data-totp-for="a"><button id="t-in-grid" data-action="copy-totp">copy</button></div>',
    );
    $('t-in-grid').click();
    expect(toasts).toContainEqual(['No code yet — try again in a moment', 'err']);
    (grid.querySelector('[data-totp-for]') as HTMLElement).dataset.totpCode = '123456';
    $('t-in-grid').click();
    expect(copied.pop()).toBe('123456');
    expect(toasts).toContainEqual(['Code copied ✓', 'ok']);
  });
});

describe('the schema and the vault switcher', { timeout: 40000 }, () => {
  afterEach(() => vi.unstubAllGlobals());

  it('keeps the schema the page ships alongside it for the forms to use', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(async () => ({ ok: true, json: async () => ({ fields: { api_key: 'x' } }) })),
    );
    await bootDesktop({ vault: vault([entry('a', 'Alpha')]) });
    await unlock();
    expect((state.st.schema as unknown as { fields: object }).fields).toEqual({ api_key: 'x' });
  });

  it('ignores a schema the server answers with an error for', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(async () => ({ ok: false, json: async () => ({ fields: {} }) })),
    );
    await bootDesktop({ vault: vault([entry('a', 'Alpha')]) });
    const before = state.st.schema;
    await unlock();
    expect(state.st.schema).toBe(before);
  });

  it('opens fine when the schema cannot be fetched', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(async () => {
        throw new Error('offline');
      }),
    );
    await bootDesktop({ vault: vault([entry('a', 'Alpha')]) });
    await unlock();
    expect(state.st.vaultOpen).toBe(true);
  });

  const leaveRemote = async (stillUnlocked: boolean) => {
    await bootDesktop({ vault: vault([entry('a', 'Alpha')]) });
    await unlock();
    backend.vault_is_unlocked = () => stillUnlocked;
    state.st.store = new state.RemoteVaultStore('https://x.test');
    $('vault-switcher').click();
    [...document.querySelectorAll<HTMLElement>('#dropdown .dropdown-item')]
      .find((i) => i.textContent?.includes('Local Vault'))!
      .click();
    await vi.waitFor(() => expect(state.st.store).toBeInstanceOf(state.TauriVaultStore));
    await flush(100);
    expect(toasts).toContainEqual(['Switched to local vault', 'ok']);
  };

  it('choosing Local Vault while the local vault is locked clears the remote’s entries and asks for the password', async () => {
    await leaveRemote(false);
    expect(state.st.vault.api_keys).toEqual([]);
    expect(state.st.vaultOpen).toBe(false);
    expect($('relock-overlay').classList.contains('open')).toBe(true);
  });

  it('choosing Local Vault while the local vault is unlocked brings its entries back', async () => {
    await leaveRemote(true);
    expect(state.st.vaultOpen).toBe(true);
    expect(state.st.vault.api_keys.map((e) => e.provider)).toEqual(['Alpha']);
  });
});

describe('a start-up that fails', { timeout: 40000 }, () => {
  it('still reveals the shell and says it failed to start', async () => {
    const err = vi.spyOn(console, 'error').mockImplementation(() => {});
    try {
      vi.resetModules();
      vi.doMock('../src/ts/tools-markup', () => ({
        mountToolsPanes: () => {
          throw new Error('markup exploded');
        },
      }));
      loadRealIndexHtml();
      document.documentElement.classList.add('booting');
      delete (window as any).__TAURI__;
      await import('../src/ts/vault');
      await vi.waitFor(() => expect(err).toHaveBeenCalled(), { timeout: 15000 });
      expect(document.documentElement.classList.contains('booting')).toBe(false);
      expect(toasts).toContainEqual(['Failed to start — see the console', 'err']);
    } finally {
      vi.doUnmock('../src/ts/tools-markup');
      err.mockRestore();
    }
  });
});
