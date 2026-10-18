/**
 * Entering a browser session in the add/edit form: splitting a pasted jar,
 * importing a captured request, checking it against a site's preset, and the
 * presets for self-hosted apps.
 *
 * These helpers fill in or check fields; none of them may change the jar behind
 * the person's back, and each says what it did.
 */
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { loadRealIndexHtml, resetState } from './helpers';

// The form binds several handlers once per module load, so each test gets a fresh
// copy of the modules to go with its fresh document.
let st: (typeof import('../src/ts/state'))['st'];
let openModal: (typeof import('../src/ts/modals'))['openModal'];
let formToEntry: (typeof import('../src/ts/modals'))['formToEntry'];
let LOCAL_SERVICE_PRESETS: (typeof import('../src/ts/modals'))['LOCAL_SERVICE_PRESETS'];

const toasts: [string, string?][] = [];
const copied: string[] = [];
let large: string | null = '';
vi.mock('../src/ts/utils', async (importOriginal) => {
  const real = await importOriginal<typeof import('../src/ts/utils')>();
  return {
    ...real,
    showToast: (m: string, k?: string) => toasts.push([m, k]),
    showPromptLarge: async () => large,
    clipboardWrite: async (t: string) => void copied.push(t),
  };
});

const $ = <T extends HTMLElement = HTMLElement>(id: string) => document.getElementById(id) as T;
const flush = (ms = 20) => new Promise((r) => setTimeout(r, ms));
const type = (t: string) => {
  $<HTMLSelectElement>('f-secret-type').value = t;
  $('f-secret-type').dispatchEvent(new Event('change'));
};
const typeKey = (v: string) => {
  $<HTMLInputElement>('f-key').value = v;
  $('f-key').dispatchEvent(new Event('input', { bubbles: true }));
};
const rows = () => [...document.querySelectorAll<HTMLElement>('#f-extra-vars-list .extra-var-row')];

beforeEach(async () => {
  vi.resetModules();
  loadRealIndexHtml();
  ({ st } = await import('../src/ts/state'));
  ({ openModal, formToEntry, LOCAL_SERVICE_PRESETS } = await import('../src/ts/modals'));
  resetState(st);
  toasts.length = 0;
  copied.length = 0;
  large = '';
  delete (window as any).__TAURI__;
  openModal('Add Secret', -1);
});
afterEach(() => {
  delete (window as any).__TAURI__;
});

describe('splitting a pasted jar', () => {
  it('offers the split only for a cookie entry that actually holds cookies, and says what is missing for cookies.txt', () => {
    type('cookie');
    expect($('f-cookie-split-group').style.display).toBe('none');
    typeKey('sid=abc; csrf=xyz');
    expect($('f-cookie-split-group').style.display).toBe('flex');
    expect($('f-cookie-split-status').textContent).toBe(
      '2 cookies — no domain or path, so cookies.txt cannot be written',
    );
    typeKey('a=1');
    expect($('f-cookie-split-status').textContent).toContain('1 cookie —');
    type('api_key');
    expect($('f-cookie-split-group').style.display).toBe('none');
  });

  it('a jar that carries its attributes says cookies.txt can be written', () => {
    type('cookie');
    typeKey('[{"name":"sid","value":"abc","domain":".x.test","path":"/"}]');
    expect($('f-cookie-split-status').textContent).toBe('1 cookie, with domain and path');
  });

  it('splitting makes one row per cookie, carrying its attributes, and pressing it twice does not duplicate', () => {
    type('cookie');
    typeKey(
      '[{"name":"sid","value":"abc","domain":".x.test","path":"/","secure":true},{"name":"csrf","value":"z"}]',
    );
    $('f-cookie-split-btn').click();
    expect(rows()).toHaveLength(2);
    expect(rows()[0].dataset.cookieAttrs).toBe(
      JSON.stringify({ domain: '.x.test', path: '/', secure: true }),
    );
    expect(rows()[1].dataset.cookieAttrs).toBeUndefined();
    expect(toasts).toContainEqual(['Split into 2 cookies', 'ok']);
    $('f-cookie-split-btn').click();
    expect(rows()).toHaveLength(2);
    const entry = formToEntry();
    expect(entry.extra_vars!.map((v) => [v.key, v.value])).toEqual([
      ['sid', 'abc'],
      ['csrf', 'z'],
    ]);
    expect(entry.extra_vars![0].attrs).toMatchObject({ domain: '.x.test', secure: true });
  });

  it('with no cookies to split it says so', () => {
    type('cookie');
    typeKey('');
    $('f-cookie-split-btn').click();
    expect(toasts).toContainEqual(['No cookies found in the value field', 'err']);
    expect(rows()).toHaveLength(0);
  });
});

describe('importing a capture', () => {
  const capture = {
    origin: 'https://site.test',
    cookies: [
      {
        name: 'sid',
        value: 'abc',
        domain: '.site.test',
        path: '/',
        secure: true,
        http_only: true,
        expires: 1900000000,
      },
      {
        name: 'plain',
        value: 'x',
        domain: null,
        path: null,
        secure: false,
        http_only: false,
        expires: 0,
      },
    ],
    headers: [],
    user_agent: 'Mozilla/5.0 test',
    dropped: [],
  };
  const tauri = (handler: () => unknown) => {
    (window as any).__TAURI__ = { core: { invoke: async () => handler() } };
  };

  it('is offered only for cookies and needs the desktop app', async () => {
    type('api_key');
    expect($('f-capture-group').style.display).toBe('none');
    type('cookie');
    expect($('f-capture-group').style.display).toBe('flex');
    $('f-capture-btn').click();
    await flush();
    expect(toasts.some(([m, k]) => m.includes('needs the desktop app') && k === 'err')).toBe(true);
  });

  it('fills the cookie rows, user agent and address, never overwriting an address already typed', async () => {
    tauri(() => capture);
    large = 'curl ...';
    type('cookie');
    $('f-capture-btn').click();
    await flush();
    expect(rows().map((r) => r.querySelector<HTMLInputElement>('.extra-var-key')!.value)).toEqual([
      'sid',
      'plain',
    ]);
    expect(rows()[0].dataset.cookieAttrs).toBe(
      JSON.stringify({
        domain: '.site.test',
        path: '/',
        secure: true,
        http_only: true,
        expires: 1900000000,
      }),
    );
    expect($<HTMLInputElement>('f-user-agent').value).toBe('Mozilla/5.0 test');
    expect($<HTMLInputElement>('f-apiurl').value).toBe('https://site.test');
    expect(toasts).toContainEqual(['Imported 2 cookies', 'ok']);
    $<HTMLInputElement>('f-apiurl').value = 'https://mine.test';
    $('f-capture-btn').click();
    await flush();
    expect(rows()).toHaveLength(2);
    expect($<HTMLInputElement>('f-apiurl').value).toBe('https://mine.test');
  });

  it('names what was left out, without values, and warns about it', async () => {
    tauri(() => ({
      ...capture,
      cookies: [capture.cookies[0]],
      dropped: ['Authorization header', 'a tracking cookie'],
    }));
    large = 'curl ...';
    type('cookie');
    $('f-capture-btn').click();
    await flush();
    expect(toasts).toContainEqual([
      'Imported 1 cookie. Dropped: Authorization header; a tracking cookie',
      'err',
    ]);
  });

  it('does nothing for an empty paste, and reports a capture it cannot read', async () => {
    tauri(() => capture);
    large = '   ';
    type('cookie');
    $('f-capture-btn').click();
    await flush();
    expect(rows()).toHaveLength(0);
    tauri(() => {
      throw new Error('not a capture');
    });
    large = 'garbage';
    $('f-capture-btn').click();
    await flush();
    expect(
      toasts.some(([m, k]) => m === 'Capture import failed: not a capture' && k === 'err'),
    ).toBe(true);
  });
});

describe('site presets', () => {
  const pick = (id: string) => {
    $<HTMLSelectElement>('f-session-preset').value = id;
    $('f-session-preset').dispatchEvent(new Event('change'));
  };

  it('is offered only for a cookie entry, listing the known sites', () => {
    type('api_key');
    expect($('f-session-preset-group').style.display).toBe('none');
    type('cookie');
    expect($('f-session-preset-group').style.display).toBe('flex');
    const labels = [...$<HTMLSelectElement>('f-session-preset').options].map((o) => o.textContent);
    expect(labels).toEqual(expect.arrayContaining(['X / Twitter', 'Instagram', 'LinkedIn']));
  });

  it('says which required cookies are missing, and when all are present, which are optional', () => {
    type('cookie');
    pick('x-twitter');
    expect($('f-session-preset-status').textContent).toBe('Missing: auth_token, ct0');
    typeKey('auth_token=a; ct0=b');
    expect($('f-session-preset-status').textContent).toBe('All required cookies present');
    pick('instagram');
    typeKey('sessionid=1; csrftoken=2; ds_user_id=3');
    expect($('f-session-preset-status').textContent).toContain(
      'All required cookies present (optional: mid, ig_did)',
    );
  });

  it('counts cookies kept as separate variables too, and never edits the jar itself', () => {
    type('cookie');
    pick('x-twitter');
    typeKey('auth_token=a');
    const before = $<HTMLInputElement>('f-key').value;
    $('f-extra-vars-add').click();
    const key = document.querySelector<HTMLInputElement>('#f-extra-vars-list .extra-var-key')!;
    key.value = 'ct0';
    key.dispatchEvent(new Event('input', { bubbles: true }));
    expect($('f-session-preset-status').textContent).toBe('All required cookies present');
    expect($<HTMLInputElement>('f-key').value).toBe(before);
  });

  it('choosing nothing clears the message and hides the copy button', () => {
    type('cookie');
    pick('x-twitter');
    expect($('f-session-preset-copy-header').style.display).toBe('');
    pick('');
    expect($('f-session-preset-status').textContent).toBe('');
    expect($('f-session-preset-copy-header').style.display).toBe('none');
  });

  it('copies the header the preset derives from the cookies; says so if it cannot', async () => {
    type('cookie');
    $('f-session-preset-copy-header').click();
    await flush();
    expect(toasts).toContainEqual(['Pick a provider preset first', 'err']);
    pick('x-twitter');
    $('f-session-preset-copy-header').click();
    await flush();
    expect(toasts).toContainEqual([
      'Nothing to derive — check the required cookies are present',
      'err',
    ]);
    typeKey('auth_token=a; ct0=csrf123');
    $('f-session-preset-copy-header').click();
    await flush();
    expect(copied[0]).toBe('x-csrf-token: csrf123');
    expect(toasts).toContainEqual(['Copied 1 header ✓', 'ok']);
  });
});

describe('self-hosted app presets', () => {
  it('only for the self-hosted type, filling how the credential is sent', () => {
    type('api_key');
    expect($('f-local-preset-group').style.display).toBe('none');
    type('local_service');
    expect($('f-local-preset-group').style.display).toBe('');
    const select = $<HTMLSelectElement>('f-local-preset');
    expect([...select.options].map((o) => o.value)).toEqual([
      '',
      ...LOCAL_SERVICE_PRESETS.map((p) => p.id),
    ]);
    select.value = 'plex';
    select.dispatchEvent(new Event('change'));
    expect($<HTMLSelectElement>('f-auth-scheme').value).toBe('header');
    expect($<HTMLInputElement>('f-auth-param').value).toBe('X-Plex-Token');
    expect($<HTMLInputElement>('f-auth-template').value).toBe('');
    select.value = 'jellyfin';
    select.dispatchEvent(new Event('change'));
    expect($<HTMLInputElement>('f-auth-template').value).toBe('MediaBrowser Token="{key}"');
    select.value = 'homeassistant';
    select.dispatchEvent(new Event('change'));
    expect($<HTMLSelectElement>('f-auth-scheme').value).toBe('');
    select.value = '';
    select.dispatchEvent(new Event('change'));
    expect($<HTMLSelectElement>('f-auth-scheme').value).toBe('');
  });
});
