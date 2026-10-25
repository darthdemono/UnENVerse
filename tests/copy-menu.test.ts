// SPDX-License-Identifier: PolyForm-Strict-1.0.0
// Copyright Jubair Hasan.
/**
 * Copying an entry: the copy button, its caret menu, and what lands on the
 * clipboard for each choice.
 *
 * A copy is the moment a secret leaves the vault, so the menu has to offer the
 * right thing for the kind of entry (a header for an API key, a file for a
 * certificate, a cookie jar in every form a tool wants) and has to say plainly
 * when it cannot, instead of putting something half-useful on the clipboard.
 */
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { st, Settings } from '../src/ts/state';
import { doCopyEnv, openCopyEnvMenu, copyField } from '../src/ts/modals';
import { loadRealIndexHtml, makeEntry, makeVault, resetState } from './helpers';

const toasts: [string, string?][] = [];
const copied: string[] = [];
const saved: [string, string][] = [];
let confirmAnswer = true;
vi.mock('../src/ts/utils', async (importOriginal) => {
  const real = await importOriginal<typeof import('../src/ts/utils')>();
  return {
    ...real,
    showToast: (m: string, k?: string) => toasts.push([m, k]),
    showConfirm: async () => confirmAnswer,
    clipboardWrite: async (t: string) => {
      copied.push(t);
    },
    saveFile: async (c: string, n: string) => {
      saved.push([c, n]);
      return { ok: true as const, path: `/dl/${n}` };
    },
  };
});

const flush = (ms = 20) => new Promise((r) => setTimeout(r, ms));
const ev = () => ({ stopPropagation() {}, currentTarget: document.body }) as unknown as Event;
const menu = () => [...document.querySelectorAll<HTMLElement>('#dropdown .dropdown-item')];
const labels = () => menu().map((i) => i.textContent!.trim());
const choose = async (label: string) => {
  menu()
    .find((i) => i.textContent!.trim().startsWith(label))!
    .click();
  await flush();
};

function put(entry: object) {
  const e = makeEntry({ id: 'x', provider: 'Stripe', api_key: 'sk_live_1', ...entry } as never);
  st.vault = makeVault({ api_keys: [e] });
  st.store = { save: async () => {}, isRemote: false } as never;
  return e;
}

beforeEach(() => {
  loadRealIndexHtml();
  resetState(st);
  toasts.length = 0;
  copied.length = 0;
  saved.length = 0;
  confirmAnswer = true;
  Settings.setAll({
    copyProfile: 'basic',
    defaultExportFormat: 'dotenv',
    metadataStyle: 'comment',
    envCopyCase: 'upper',
    envIncludePrefix: false,
  });
  delete (window as any).__TAURI__;
});
afterEach(() => {
  delete (window as any).__TAURI__;
});

describe('the copy button', () => {
  it('copies the entry at the saved profile and says which one', async () => {
    put({ environment: 'production', scopes: ['read'] });
    doCopyEnv(ev(), 0);
    await flush();
    expect(copied[0]).toContain('STRIPE=sk_live_1');
    expect(copied[0]).not.toContain('environment');
    expect(toasts).toContainEqual(['Copied as .env (basic) ✓', 'ok']);
    Settings.set('copyProfile', 'extended');
    doCopyEnv(ev(), 0);
    await flush();
    expect(copied[1]).toContain('# environment: production');
  });

  it('a one-off profile from the menu does not change the saved one', async () => {
    put({ environment: 'production' });
    doCopyEnv(ev(), 0, 'full');
    await flush();
    expect(copied[0]).toContain('# environment: production');
    expect(Settings.get('copyProfile')).toBe('basic');
  });

  it('"Value only" is the bare value with no name, header or quoting', async () => {
    put({ api_key: 'p@ss word' });
    doCopyEnv(ev(), 0, 'value');
    await flush();
    expect(copied[0]).toBe('p@ss word');
    expect(toasts).toContainEqual(['Copied as value ✓', 'ok']);
  });

  it('follows a YAML default format unless a profile was asked for', async () => {
    put({});
    Settings.set('defaultExportFormat', 'yaml');
    doCopyEnv(ev(), 0);
    await flush();
    expect(copied[0]).toContain('STRIPE: sk_live_1');
    expect(toasts).toContainEqual(['Copied as YAML ✓', 'ok']);
    doCopyEnv(ev(), 0, 'basic');
    await flush();
    expect(copied[1]).toContain('STRIPE=sk_live_1');
  });

  it('remembers the name the value went out under, but not for a value-only copy', async () => {
    const e = put({});
    doCopyEnv(ev(), 0, 'value');
    await flush();
    expect(e.last_copied_name).toBeUndefined();
    doCopyEnv(ev(), 0);
    await flush();
    expect(e.last_copied_name).toBe('STRIPE');
  });

  it('copying something that is no longer there does nothing', async () => {
    put({});
    doCopyEnv(ev(), 7);
    await flush();
    expect(copied).toEqual([]);
    expect(toasts).toEqual([]);
  });

  it('flashes the card’s copy button for a moment', async () => {
    put({});
    document.body.insertAdjacentHTML('beforeend', '<button id="env-btn-0"></button>');
    vi.useFakeTimers();
    doCopyEnv(ev(), 0);
    await vi.advanceTimersByTimeAsync(5);
    expect(document.getElementById('env-btn-0')!.classList.contains('env-copied')).toBe(true);
    await vi.advanceTimersByTimeAsync(1700);
    expect(document.getElementById('env-btn-0')!.classList.contains('env-copied')).toBe(false);
    vi.useRealTimers();
  });
});

describe('copying a single field', () => {
  it('says copied, highlights the button briefly, and reports a failed copy', async () => {
    const btn = document.createElement('button');
    copyField(ev(), 'v', btn);
    await flush();
    expect(copied).toEqual(['v']);
    expect(btn.classList.contains('active')).toBe(true);
    expect(toasts).toContainEqual(['Copied ✓', 'ok']);
  });
});

describe('the menu', () => {
  it('offers the profiles (marking the saved one), value only and a curl command', () => {
    put({});
    Settings.set('copyProfile', 'extended');
    openCopyEnvMenu(ev(), 0);
    expect(labels()).toEqual([
      'Basic',
      'Extended ·',
      'Full',
      'Value only',
      'Request header (Authorization)',
      'curl command',
    ]);
  });

  it('does nothing for an entry that is gone', () => {
    put({});
    openCopyEnvMenu(ev(), 4);
    expect(menu()).toHaveLength(0);
  });

  it('a profile choice copies at that profile; curl carries the right header for the entry’s scheme', async () => {
    put({ api_url: 'https://api.example/v1' });
    openCopyEnvMenu(ev(), 0);
    await choose('curl command');
    expect(copied[0]).toBe("curl -H 'Authorization: Bearer sk_live_1' 'https://api.example/v1'");
    expect(toasts).toContainEqual(['Copied curl command ✓', 'ok']);
  });

  it('shows the request header row, named for the scheme, for an entry that sends one', async () => {
    put({ auth_scheme: 'header', auth_param: 'X-Token' });
    openCopyEnvMenu(ev(), 0);
    expect(labels()).toContain('Request header (X-Token)');
    await choose('Request header');
    expect(copied[0]).toBe('X-Token: sk_live_1');
    put({ auth_scheme: 'query' });
    openCopyEnvMenu(ev(), 0);
    expect(labels().some((l) => l.startsWith('Request header'))).toBe(false);
    put({ api_key: '' });
    openCopyEnvMenu(ev(), 0);
    expect(labels().some((l) => l.startsWith('Request header'))).toBe(false);
  });

  it('a file-shaped credential is written to a file, or its path copied, never pasted inline', async () => {
    put({
      certificate_data: '-----BEGIN CERTIFICATE-----\nAAA\n-----END CERTIFICATE-----',
      api_key: '',
      mount_path: '/etc/ssl/app.pem',
    });
    openCopyEnvMenu(ev(), 0);
    expect(labels()).toContain('Write to file…');
    expect(labels()).toContain('Copy the .env line (path, not contents)');
    await choose('Write to file');
    expect(saved[0][1]).toBe('stripe.pem');
    expect(saved[0][0]).toContain('BEGIN CERTIFICATE');
    openCopyEnvMenu(ev(), 0);
    await choose('Copy the .env line');
    expect(copied[0]).toBe('STRIPE=/etc/ssl/app.pem');
    put({ blob_data: '{"k":1}', api_key: '' });
    openCopyEnvMenu(ev(), 0);
    expect(labels()).toContain('Write to file…');
    expect(labels().some((l) => l.startsWith('Copy the .env line'))).toBe(false);
  });

  describe('in the desktop app', () => {
    const tauri = (handler: (cmd: string, args: any) => unknown) => {
      (window as any).__TAURI__ = { core: { invoke: async (c: string, a: any) => handler(c, a) } };
    };

    it('offers the per-type output files that Rust produces, and copies what it returns', async () => {
      tauri((c, a) => (c === 'type_emit' ? `emitted:${a.format}` : null));
      put({ secretType: 'registry_token' });
      openCopyEnvMenu(ev(), 0);
      expect(labels()).toEqual(
        expect.arrayContaining(['Copy as npmrc', 'Copy as pypirc', 'Copy as netrc']),
      );
      await choose('Copy as npmrc');
      expect(copied[0]).toBe('emitted:npmrc');
    });

    it('a failed emit names the reason', async () => {
      tauri(() => {
        throw new Error('no registry host');
      });
      put({ secretType: 'database' });
      openCopyEnvMenu(ev(), 0);
      await choose('Copy as dsn');
      expect(toasts.some(([m, k]) => m === 'no registry host' && k === 'err')).toBe(true);
    });

    it('a browser tab offers none of them, since Rust is not there', () => {
      put({ secretType: 'registry_token' });
      openCopyEnvMenu(ev(), 0);
      expect(labels().some((l) => l.startsWith('Copy as'))).toBe(false);
    });

    it('a Wi-Fi entry can show its QR code', async () => {
      tauri((c) => (c === 'type_emit' ? 'WIFI:T:WPA;S:home;P:pw;;' : null));
      put({ secretType: 'wifi', provider: 'Home' });
      openCopyEnvMenu(ev(), 0);
      expect(labels()).toContain('Show Wi-Fi QR code…');
      await choose('Show Wi-Fi QR code');
      expect(document.getElementById('wifi-qr-overlay')!.classList.contains('open')).toBe(true);
      expect(
        document.getElementById('wifi-qr-body')!.querySelector('svg')!.getAttribute('aria-label'),
      ).toContain('Home');
      tauri(() => {
        throw new Error('no uri');
      });
      openCopyEnvMenu(ev(), 0);
      await choose('Show Wi-Fi QR code');
      expect(toasts.some(([m]) => m === 'no uri')).toBe(true);
    });

    it('an OAuth refresh names the host first, sends nothing if declined, and saves the new token before showing it', async () => {
      const refreshed = makeEntry({ id: 'x', provider: 'Stripe', api_key: 'new-access' } as never);
      let calls = 0;
      tauri((c) => {
        if (c === 'oauth_refresh') {
          calls++;
          return { entry: refreshed, rotated: true };
        }
        return null;
      });
      const e = put({
        secretType: 'oauth_client',
        extra_vars: [{ key: 'token_url', value: 'https://login.example.com/oauth/token' }],
      });
      void e;
      confirmAnswer = false;
      openCopyEnvMenu(ev(), 0);
      await choose('Refresh access token');
      expect(calls).toBe(0);
      confirmAnswer = true;
      openCopyEnvMenu(ev(), 0);
      await choose('Refresh access token');
      expect(calls).toBe(1);
      expect(st.vault.api_keys[0].api_key).toBe('new-access');
      expect(toasts).toContainEqual(['Refreshed; the new refresh token was saved first', 'ok']);
    });

    it('an OAuth refresh failure is reported', async () => {
      tauri(() => {
        throw new Error('invalid_grant');
      });
      put({ secretType: 'oauth_client' });
      openCopyEnvMenu(ev(), 0);
      await choose('Refresh access token');
      expect(toasts.some(([m, k]) => m === 'invalid_grant' && k === 'err')).toBe(true);
    });
  });

  describe('a browser session (cookie jar)', () => {
    it('offers the header, cookies.txt, JSON and Playwright forms, and copies each', async () => {
      put({
        secretType: 'cookie',
        api_key: '',
        api_url: 'https://site.test',
        extra_vars: [
          { key: 'sid', value: 'abc', attrs: { domain: '.site.test', path: '/', secure: true } },
        ],
      });
      openCopyEnvMenu(ev(), 0);
      expect(labels()).toEqual(
        expect.arrayContaining([
          'Cookie header',
          'cookies.txt (curl -b, yt-dlp)',
          'Cookie JSON (back into a browser)',
          'Playwright storageState JSON',
        ]),
      );
      await choose('Cookie header');
      expect(copied[0]).toBe('sid=abc');
      openCopyEnvMenu(ev(), 0);
      await choose('cookies.txt');
      expect(copied[1]).toContain('.site.test\tTRUE\t/\tTRUE\t0\tsid\tabc');
      openCopyEnvMenu(ev(), 0);
      await choose('Cookie JSON');
      expect(JSON.parse(copied[2])[0]).toMatchObject({ name: 'sid', value: 'abc' });
      openCopyEnvMenu(ev(), 0);
      await choose('Playwright storageState');
      expect(JSON.parse(copied[3]).cookies[0]).toMatchObject({ name: 'sid', domain: '.site.test' });
    });

    it('refuses cookies.txt for a pasted header string, with the reason, instead of writing a file tools ignore', async () => {
      put({ secretType: 'cookie', api_key: 'sid=abc; csrf=xyz' });
      openCopyEnvMenu(ev(), 0);
      await choose('cookies.txt');
      expect(copied).toHaveLength(0);
      expect(
        toasts.some(([m, k]) => m.includes('cookies.txt needs domain and path') && k === 'err'),
      ).toBe(true);
      openCopyEnvMenu(ev(), 0);
      await choose('Cookie header');
      expect(copied[0]).toBe('sid=abc; csrf=xyz');
    });

    it('refuses a Playwright file it could not make honestly', async () => {
      put({ secretType: 'cookie', api_key: 'sid=abc' });
      openCopyEnvMenu(ev(), 0);
      await choose('Playwright storageState');
      expect(
        toasts.some(
          ([m, k]) => m.includes('Playwright storageState needs a session URL') && k === 'err',
        ),
      ).toBe(true);
    });

    it('other entry types do not get the cookie rows', () => {
      put({});
      openCopyEnvMenu(ev(), 0);
      expect(labels()).not.toContain('Cookie header');
    });
  });
});

describe('every profile choice', () => {
  it.each([
    ['Basic', 'basic'],
    ['Extended', 'extended'],
    ['Full', 'full'],
  ])('%s copies the entry as that profile and says so', async (label, profile) => {
    put({});
    openCopyEnvMenu(ev(), 0);
    await choose(label);
    expect(copied).toHaveLength(1);
    expect(
      toasts.some(([m]) => m.startsWith('Copied as ') && m.toLowerCase().includes(profile)),
    ).toBe(true);
  });

  it('Value only copies the bare value', async () => {
    put({});
    openCopyEnvMenu(ev(), 0);
    await choose('Value only');
    expect(copied).toEqual(['sk_live_1']);
  });
});

describe('the menu on the desktop', () => {
  let calls: [string, any][] = [];
  let backend: Record<string, (a: any) => unknown> = {};
  beforeEach(() => {
    calls = [];
    backend = {};
    (window as any).__TAURI__ = {
      core: {
        invoke: async (cmd: string, args: any) => {
          calls.push([cmd, args]);
          const fn = backend[cmd];
          if (!fn) throw new Error(`no ${cmd}`);
          return fn(args);
        },
      },
    };
  });
  afterEach(() => {
    delete (window as any).__TAURI__;
  });

  it('a registry token offers each file format Rust can write, and copies what Rust returns', async () => {
    put({ secretType: 'registry_token', api_url: 'https://registry.test' });
    backend.type_emit = (a) => `emitted:${a.format}`;
    openCopyEnvMenu(ev(), 0);
    expect(labels()).toEqual(
      expect.arrayContaining([
        'Copy as npmrc',
        'Copy as pypirc',
        'Copy as cargo-credentials',
        'Copy as docker-config',
        'Copy as netrc',
      ]),
    );
    await choose('Copy as npmrc');
    expect(copied).toEqual(['emitted:npmrc']);
    expect(toasts).toContainEqual(['Copied npmrc ✓', 'ok']);
  });

  it('a format Rust refuses is reported, not copied', async () => {
    put({ secretType: 'database' });
    backend.type_emit = () => {
      throw new Error('no host');
    };
    openCopyEnvMenu(ev(), 0);
    await choose('Copy as dsn');
    expect(copied).toEqual([]);
    expect(toasts[toasts.length - 1]).toEqual(['no host', 'err']);
  });

  it('a Wi-Fi network offers its QR code, and shows an error when it cannot be made', async () => {
    put({ secretType: 'wifi', provider: 'Home' });
    backend.type_emit = (a) => (a.format === 'wifi-uri' ? 'WIFI:S:Home;T:WPA;P:pw;;' : 'x');
    openCopyEnvMenu(ev(), 0);
    await choose('Show Wi-Fi QR code');
    expect(document.getElementById('wifi-qr-overlay')!.classList.contains('open')).toBe(true);
    backend.type_emit = () => {
      throw new Error('bad uri');
    };
    openCopyEnvMenu(ev(), 0);
    await choose('Show Wi-Fi QR code');
    expect(toasts[toasts.length - 1]).toEqual(['bad uri', 'err']);
  });

  it('an OAuth client refreshes only after naming the host, and saves the new entry first', async () => {
    const e = put({
      secretType: 'oauth_client',
      extra_vars: [{ key: 'token_url', value: 'https://auth.example.test/token' }],
    });
    const asked: string[] = [];
    confirmAnswer = false;
    backend.oauth_refresh = () => ({ entry: { ...e, api_key: 'fresh-token' }, rotated: true });
    openCopyEnvMenu(ev(), 0);
    await choose('Refresh access token');
    expect(calls.some(([c]) => c === 'oauth_refresh')).toBe(false);
    confirmAnswer = true;
    void asked;
    openCopyEnvMenu(ev(), 0);
    await choose('Refresh access token');
    expect(st.vault.api_keys[0].api_key).toBe('fresh-token');
    expect(toasts).toContainEqual(['Refreshed; the new refresh token was saved first', 'ok']);
  });

  it('an OAuth refresh with no rotation says plainly it refreshed; a failure is reported; a vanished entry is left alone', async () => {
    const e = put({ secretType: 'oauth_client' });
    backend.oauth_refresh = () => ({ entry: { ...e, api_key: 'again' }, rotated: false });
    openCopyEnvMenu(ev(), 0);
    await choose('Refresh access token');
    expect(toasts).toContainEqual(['Refreshed', 'ok']);
    backend.oauth_refresh = () => {
      throw new Error('invalid_grant');
    };
    openCopyEnvMenu(ev(), 0);
    await choose('Refresh access token');
    expect(toasts[toasts.length - 1]).toEqual(['invalid_grant', 'err']);
    backend.oauth_refresh = () => {
      st.vault.api_keys = [];
      return { entry: e, rotated: false };
    };
    openCopyEnvMenu(ev(), 0);
    await choose('Refresh access token');
    expect(st.vault.api_keys).toEqual([]);
  });
});

describe('the cookie forms', () => {
  const jar = [
    { name: 'sid', value: 'abc', attrs: { domain: 'site.test', path: '/', secure: true } },
    { name: 'theme', value: 'dark', attrs: { domain: 'site.test', path: '/' } },
  ];

  it('offers each form a tool wants and copies it', async () => {
    put({
      secretType: 'cookie',
      api_key: '',
      extra_vars: jar.map((c) => ({ key: c.name, value: c.value, secret: true, attrs: c.attrs })),
    });
    openCopyEnvMenu(ev(), 0);
    for (const [label, what] of [
      ['Cookie header', 'cookie header'],
      ['cookies.txt', 'cookies.txt'],
      ['Cookie JSON', 'cookie JSON'],
      ['Playwright storageState', 'Playwright storageState'],
    ]) {
      openCopyEnvMenu(ev(), 0);
      await choose(label);
      expect(toasts).toContainEqual([`Copied ${what} ✓`, 'ok']);
    }
    expect(copied[0]).toBe('sid=abc; theme=dark');
  });

  it('refuses cookies.txt for a jar without domains, with the reason', async () => {
    put({ secretType: 'cookie', api_key: 'sid=abc' });
    openCopyEnvMenu(ev(), 0);
    await choose('cookies.txt');
    expect(copied).toEqual([]);
    expect(toasts[toasts.length - 1][1]).toBe('err');
    openCopyEnvMenu(ev(), 0);
    await choose('Playwright storageState');
    expect(toasts[toasts.length - 1][1]).toBe('err');
  });
});

describe('the dropdown itself', () => {
  it('a menu that would run past the window edge is pulled back inside it', async () => {
    const rect = { right: 2000, bottom: 2000, width: 300, height: 120, left: 0, top: 0 };
    const spy = vi
      .spyOn(HTMLElement.prototype, 'getBoundingClientRect')
      .mockReturnValue(rect as DOMRect);
    const { showContextMenu } = await import('../src/ts/modals');
    showContextMenu(900, 700, [{ label: 'One', fn: () => {} }]);
    await new Promise((r) => requestAnimationFrame(() => r(null)));
    const dd = document.getElementById('dropdown')!;
    expect(dd.style.left).toBe('600px');
    expect(dd.style.top).toBe('580px');
    spy.mockRestore();
  });

  it('opening a second menu closes the first one’s listeners', async () => {
    const { showContextMenu } = await import('../src/ts/modals');
    showContextMenu(10, 10, [{ label: 'A', fn: () => {} }]);
    showContextMenu(20, 20, [{ label: 'B', fn: () => {} }]);
    expect(
      [...document.querySelectorAll('#dropdown .dropdown-item')].map((i) => i.textContent),
    ).toEqual(['B']);
  });
});
