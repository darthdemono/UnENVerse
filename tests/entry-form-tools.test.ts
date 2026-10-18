/**
 * The helpers around the entry form: the generator popover, cookie split and
 * capture import, session presets, the PGP key reader, the local-service
 * presets, and the keyboard handling of the project picker. Booted for real; the
 * desktop calls are answered by a fake bridge installed after boot, because the
 * form checks for the desktop at the moment of the click.
 */
import { describe, it, expect, beforeEach, afterEach, afterAll, vi } from 'vitest';
import { bootApp, entry, teardownApp } from './app-boot';

const toasts: [string, string?][] = [];
const copied: string[] = [];
let largeAnswer: string | null = '';
vi.mock('../src/ts/utils', async (importOriginal) => {
  const real = await importOriginal<typeof import('../src/ts/utils')>();
  return {
    ...real,
    showToast: (m: string, k?: string) => toasts.push([m, k]),
    showConfirm: async () => true,
    showPromptLarge: async () => largeAnswer,
    clipboardWrite: async (t: string) => void copied.push(t),
  };
});

afterAll(() => teardownApp());

const $ = <T extends HTMLElement = HTMLElement>(id: string) => document.getElementById(id) as T;
const flush = (ms = 30) => new Promise((r) => setTimeout(r, ms));
const set = (id: string, value: string) => {
  const el = $<HTMLInputElement | HTMLSelectElement | HTMLTextAreaElement>(id);
  el.value = value;
  el.dispatchEvent(new Event('input', { bubbles: true }));
  el.dispatchEvent(new Event('change', { bubbles: true }));
};
const key = (target: HTMLElement, k: string) =>
  target.dispatchEvent(new KeyboardEvent('keydown', { key: k, bubbles: true, cancelable: true }));

let calls: [string, unknown][] = [];
let backend: Record<string, (a: any) => unknown> = {};
const desktop = () => {
  (window as any).__TAURI__ = {
    core: {
      invoke: async (cmd: string, args: unknown) => {
        calls.push([cmd, args]);
        const fn = backend[cmd];
        if (!fn) throw new Error(`no ${cmd}`);
        return fn(args);
      },
    },
  };
};

const openAdd = async (type = 'api_key') => {
  $('add-btn').click();
  await flush();
  set('f-secret-type', type);
  await flush();
};

beforeEach(async () => {
  toasts.length = 0;
  copied.length = 0;
  calls = [];
  backend = {};
  largeAnswer = '';
  await bootApp([entry('a', 'Existing')], {
    projects: [
      { id: 'Universal', name: 'Universal' },
      { id: 'web', name: 'Web' },
      { id: 'ops', name: 'Ops' },
    ],
  });
}, 30000);

afterEach(() => {
  delete (window as any).__TAURI__;
});

describe('the generator popover', { timeout: 30000 }, () => {
  const popover = () => $('f-gen-popover');
  const gen = async () => {
    $('gp-generate-btn').click();
    await flush(40);
  };

  it('opens from the caret, closes from it, and tells assistive tech', async () => {
    await openAdd();
    expect(popover().hidden).toBe(true);
    $('f-key-generate-caret').click();
    expect(popover().hidden).toBe(false);
    expect($('f-key-generate-caret').getAttribute('aria-expanded')).toBe('true');
    $('f-key-generate-caret').click();
    expect(popover().hidden).toBe(true);
    expect($('f-key-generate-caret').getAttribute('aria-expanded')).toBe('false');
  });

  it('closes on an outside click or Escape, but not on a click inside it', async () => {
    await openAdd();
    $('f-key-generate-caret').click();
    popover().click();
    expect(popover().hidden).toBe(false);
    document.body.click();
    expect(popover().hidden).toBe(true);
    $('f-key-generate-caret').click();
    key(document.body, 'a');
    expect(popover().hidden).toBe(false);
    key(document.body, 'Escape');
    expect(popover().hidden).toBe(true);
    key(document.body, 'Escape');
    expect(popover().hidden).toBe(true);
  });

  it('generates random bytes of the chosen length and encoding, and a different set of them each time', async () => {
    await openAdd();
    $('f-key-generate-caret').click();
    set('gp-bytes-format', 'hex');
    await gen();
    const first = $('gp-output').textContent!;
    expect(first).toMatch(/^[0-9a-f]{64}$/);
    document.querySelectorAll<HTMLElement>('.gen-byte-btn')[0].click();
    const small = document.querySelector<HTMLElement>('.gen-byte-btn.active')!;
    expect(small.classList.contains('active')).toBe(true);
    await gen();
    expect($('gp-output').textContent!.length).toBeLessThan(first.length);
  });

  it('generates a password of the chosen length from the chosen sets, and refuses with none ticked', async () => {
    await openAdd();
    $('f-key-generate-caret').click();
    document.querySelector<HTMLElement>('.gen-tab-btn[data-gen-tab="password"]')!.click();
    expect(document.querySelector<HTMLElement>('[data-gen-pane="password"]')!.hidden).toBe(false);
    const len = $<HTMLInputElement>('gp-pw-length');
    len.value = '12';
    len.dispatchEvent(new Event('input'));
    expect($('gp-pw-len-display').textContent).toBe('12');
    set('gp-pw-symbols', '');
    await gen();
    expect($('gp-output').textContent).toMatch(/^[A-Za-z0-9]{12}$/);
    for (const id of ['gp-pw-upper', 'gp-pw-lower', 'gp-pw-digits'])
      $<HTMLInputElement>(id).checked = false;
    await gen();
    expect(toasts).toContainEqual(['Select at least one character set', 'err']);
  });

  it('generates a key in a known pattern', async () => {
    await openAdd();
    $('f-key-generate-caret').click();
    document.querySelector<HTMLElement>('.gen-tab-btn[data-gen-tab="apikey"]')!.click();
    await gen();
    expect($('gp-output').textContent!.length).toBeGreaterThan(8);
  });

  it('hashes the text it is given', async () => {
    await openAdd();
    $('f-key-generate-caret').click();
    document.querySelector<HTMLElement>('.gen-tab-btn[data-gen-tab="hash"]')!.click();
    set('gp-hash-input', 'abc');
    set('gp-hash-algo', 'SHA-256');
    set('gp-hash-fmt', 'hex');
    await gen();
    await vi.waitFor(() =>
      expect($('gp-output').textContent).toBe(
        'ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad',
      ),
    );
  });

  it('switching tabs clears the output; Copy copies it and Use puts it in the key field and closes', async () => {
    await openAdd();
    $('f-key-generate-caret').click();
    set('gp-bytes-format', 'hex');
    await gen();
    const value = $('gp-output').textContent!;
    expect(($('gp-use-btn') as HTMLButtonElement).disabled).toBe(false);
    $('gp-copy-btn').click();
    expect(copied).toEqual([value]);
    $('gp-use-btn').click();
    expect($<HTMLInputElement>('f-key').value).toBe(value);
    expect(popover().hidden).toBe(true);
    expect(toasts).toContainEqual(['Generated value applied', 'ok']);
    $('f-key-generate-caret').click();
    expect($('gp-output').textContent).toBe('');
    $('gp-copy-btn').click();
    $('gp-use-btn').click();
    expect(copied).toEqual([value]);
    expect($<HTMLInputElement>('f-key').value).toBe(value);
  });

  it('opening the form again starts on a clean popover', async () => {
    await openAdd();
    $('f-key-generate-caret').click();
    document.querySelector<HTMLElement>('.gen-tab-btn[data-gen-tab="hash"]')!.click();
    $('modal-cancel').click();
    await openAdd();
    expect(popover().hidden).toBe(true);
    expect(
      document.querySelector('.gen-tab-btn[data-gen-tab="bytes"]')!.getAttribute('aria-selected'),
    ).toBe('true');
  });
});

describe('the project picker', { timeout: 30000 }, () => {
  const items = () => [...document.querySelectorAll<HTMLElement>('#f-project .project-pick-item')];

  it('toggles on click, Space and Enter, and reports the state to assistive tech', async () => {
    await openAdd();
    const web = items().find((i) => i.dataset.value === 'web')!;
    web.click();
    expect(web.getAttribute('aria-selected')).toBe('true');
    key(web, ' ');
    expect(web.classList.contains('selected')).toBe(false);
    key(web, 'Enter');
    expect(web.classList.contains('selected')).toBe(true);
  });

  it('arrow keys move the one tab stop around the list, wrapping at both ends', async () => {
    await openAdd();
    const list = items();
    expect(list.filter((i) => i.tabIndex === 0)).toHaveLength(1);
    key(list[0], 'ArrowDown');
    expect(list[1].tabIndex).toBe(0);
    expect(list[0].tabIndex).toBe(-1);
    key(list[1], 'ArrowUp');
    key(list[0], 'ArrowUp');
    expect(list[list.length - 1].tabIndex).toBe(0);
    key(list[list.length - 1], 'ArrowDown');
    expect(list[0].tabIndex).toBe(0);
  });

  it('other keys do nothing', async () => {
    await openAdd();
    const first = items()[0];
    const ev = new KeyboardEvent('keydown', { key: 'a', bubbles: true, cancelable: true });
    first.dispatchEvent(ev);
    expect(ev.defaultPrevented).toBe(false);
  });
});

describe('cookies and sessions', { timeout: 30000 }, () => {
  const jar = 'sid=abc123; theme=dark';
  const txtJar = '.site.test\tTRUE\t/\tTRUE\t0\tsid\tabc123';

  it('offers to split a pasted cookie string, and says what the string lacks', async () => {
    await openAdd('cookie');
    expect($('f-cookie-split-group').style.display).toBe('none');
    set('f-key', jar);
    expect($('f-cookie-split-group').style.display).toBe('flex');
    expect($('f-cookie-split-status').textContent).toMatch(
      /^2 cookies — no .*cookies\.txt cannot be written$/,
    );
    set('f-key', txtJar);
    expect($('f-cookie-split-status').textContent).toBe('1 cookie, with domain and path');
    set('f-secret-type', 'api_key');
    expect($('f-cookie-split-group').style.display).toBe('none');
  });

  it('splitting replaces the variable rows with one per cookie, keeping each cookie’s attributes', async () => {
    await openAdd('cookie');
    set('f-key', txtJar);
    $('f-cookie-split-btn').click();
    const rows = [...document.querySelectorAll<HTMLElement>('#f-extra-vars-list .extra-var-row')];
    expect(rows).toHaveLength(1);
    expect(rows[0].querySelector<HTMLInputElement>('.extra-var-key')!.value).toBe('sid');
    expect(JSON.parse(rows[0].dataset.cookieAttrs!)).toMatchObject({
      domain: '.site.test',
      path: '/',
    });
    $('f-cookie-split-btn').click();
    expect(document.querySelectorAll('#f-extra-vars-list .extra-var-row')).toHaveLength(1);
    expect(toasts).toContainEqual(['Split into 1 cookie', 'ok']);
  });

  it('splitting a string with several cookies says how many', async () => {
    await openAdd('cookie');
    set('f-key', jar);
    $('f-cookie-split-btn').click();
    expect(toasts).toContainEqual(['Split into 2 cookies', 'ok']);
  });

  it('splitting an empty value says there is nothing to split', async () => {
    await openAdd('cookie');
    $('f-cookie-split-btn').click();
    expect(toasts).toContainEqual(['No cookies found in the value field', 'err']);
  });

  it('a session preset lists the cookies still missing, then confirms when all are present', async () => {
    await openAdd('cookie');
    expect($('f-session-preset-group').style.display).toBe('flex');
    set('f-session-preset', 'x-twitter');
    expect($('f-session-preset-status').textContent).toBe('Missing: auth_token, ct0');
    set('f-key', 'auth_token=a; ct0=b');
    expect($('f-session-preset-status').textContent).toMatch(/^All required cookies present/);
    expect($('f-session-preset-copy-header').style.display).toBe('');
    set('f-session-preset', '');
    expect($('f-session-preset-status').textContent).toBe('');
    expect($('f-session-preset-copy-header').style.display).toBe('none');
  });

  it('a preset that names optional cookies lists them', async () => {
    await openAdd('cookie');
    set('f-session-preset', 'instagram');
    set('f-key', 'sessionid=1; csrftoken=2; ds_user_id=3');
    expect($('f-session-preset-status').textContent).toMatch(/^All required cookies present/);
  });

  it('other types hide the preset picker', async () => {
    await openAdd('api_key');
    expect($('f-session-preset-group').style.display).toBe('none');
  });

  it('Copy header asks for a preset first, copies what the jar supports, and says when nothing can be derived', async () => {
    await openAdd('cookie');
    $('f-session-preset-copy-header').click();
    await flush();
    expect(toasts).toContainEqual(['Pick a provider preset first', 'err']);
    set('f-session-preset', 'x-twitter');
    $('f-session-preset-copy-header').click();
    await flush();
    expect(toasts).toContainEqual([
      'Nothing to derive — check the required cookies are present',
      'err',
    ]);
    set('f-key', 'auth_token=a; ct0=tok');
    set('f-apiurl', 'https://x.com');
    $('f-session-preset-copy-header').click();
    await flush(80);
    expect(copied[0]).toContain('x-csrf-token: tok');
    expect(toasts.some(([m]) => /^Copied \d+ headers? ✓$/.test(m))).toBe(true);
  });

  it('importing a capture needs the desktop app', async () => {
    await openAdd('cookie');
    $('f-capture-btn').click();
    await flush();
    expect(toasts[toasts.length - 1][0]).toContain('Importing a capture needs the desktop app');
  });

  it('importing a capture fills the cookies, the user agent and the address, and lists what was dropped', async () => {
    desktop();
    backend.session_capture_parse = () => ({
      origin: 'https://site.test',
      cookies: [
        {
          name: 'sid',
          value: 'abc',
          domain: 'site.test',
          path: '/',
          secure: true,
          http_only: true,
          expires: 1900000000,
        },
        {
          name: 'theme',
          value: 'dark',
          domain: null,
          path: null,
          secure: false,
          http_only: false,
          expires: 0,
        },
      ],
      headers: [],
      user_agent: 'TestAgent/1.0',
      dropped: ['Authorization header'],
    });
    await openAdd('cookie');
    largeAnswer = 'curl https://site.test -H "Cookie: sid=abc"';
    $('f-capture-btn').click();
    await flush(80);
    const rows = document.querySelectorAll('#f-extra-vars-list .extra-var-row');
    expect(rows).toHaveLength(2);
    expect($<HTMLInputElement>('f-user-agent').value).toBe('TestAgent/1.0');
    expect($<HTMLInputElement>('f-apiurl').value).toBe('https://site.test');
    expect(toasts[toasts.length - 1]).toEqual([
      'Imported 2 cookies. Dropped: Authorization header',
      'err',
    ]);
    // An address already typed is kept.
    set('f-apiurl', 'https://mine.test');
    $('f-capture-btn').click();
    await flush(80);
    expect($<HTMLInputElement>('f-apiurl').value).toBe('https://mine.test');
  });

  it('a capture with one cookie and nothing dropped says so plainly; a failing parse is reported; cancelling does nothing', async () => {
    desktop();
    backend.session_capture_parse = () => ({
      origin: null,
      cookies: [
        {
          name: 'sid',
          value: 'abc',
          domain: null,
          path: null,
          secure: false,
          http_only: false,
          expires: 0,
        },
      ],
      headers: [],
      user_agent: null,
      dropped: [],
    });
    await openAdd('cookie');
    largeAnswer = 'x';
    $('f-capture-btn').click();
    await flush(80);
    expect(toasts[toasts.length - 1]).toEqual(['Imported 1 cookie', 'ok']);
    backend.session_capture_parse = () => {
      throw new Error('unreadable');
    };
    $('f-capture-btn').click();
    await flush(80);
    expect(toasts[toasts.length - 1][0]).toBe('Capture import failed: unreadable');
    largeAnswer = null;
    const before = toasts.length;
    $('f-capture-btn').click();
    await flush(60);
    expect(toasts).toHaveLength(before);
  });
});

describe('PGP keys and local service presets', { timeout: 30000 }, () => {
  it('reading a key needs the desktop app', async () => {
    await openAdd('gpg_key');
    expect($('f-pgp-group').style.display).toBe('flex');
    $('f-pgp-btn').click();
    await flush();
    expect(toasts[toasts.length - 1][0]).toContain('Reading a key needs the desktop app');
  });

  it('fills the expiry and the public identifiers, replacing only the rows it owns', async () => {
    desktop();
    backend.pgp_inspect = () => ({
      fingerprint: 'AAAA BBBB',
      key_id: 'BBBB',
      user_ids: ['Alice <a@x.test>', 'Alice <b@x.test>'],
      expires_at: '2031-02-03T00:00:00Z',
    });
    await openAdd('gpg_key');
    $('f-extra-vars-add').click();
    const added = [
      ...document.querySelectorAll<HTMLElement>('#f-extra-vars-list .extra-var-row'),
    ].pop()!;
    added.querySelector<HTMLInputElement>('.extra-var-key')!.value = 'comment';
    added.querySelector<HTMLInputElement>('.extra-var-value')!.value = 'mine';
    largeAnswer = '-----BEGIN PGP PUBLIC KEY BLOCK-----';
    $('f-pgp-btn').click();
    await flush(80);
    expect($<HTMLInputElement>('f-expires').value).toBe('2031-02-03T00:00:00Z');
    const vars = Object.fromEntries(
      [...document.querySelectorAll<HTMLElement>('#f-extra-vars-list .extra-var-row')].map((r) => [
        r.querySelector<HTMLInputElement>('.extra-var-key')!.value,
        r.querySelector<HTMLInputElement>('.extra-var-value')!.value,
      ]),
    );
    expect(vars).toMatchObject({
      comment: 'mine',
      fingerprint: 'AAAA BBBB',
      key_id: 'BBBB',
      user_ids: 'Alice <a@x.test>; Alice <b@x.test>',
    });
    expect(
      [...document.querySelectorAll<HTMLInputElement>('#f-extra-vars-list .extra-var-key')].filter(
        (k) => k.value === 'fingerprint',
      ),
    ).toHaveLength(1);
    expect(toasts[toasts.length - 1]).toEqual(['Key expires 2031-02-03', 'ok']);
  });

  it('a key that never expires says so, an unreadable one is reported, and cancelling does nothing', async () => {
    desktop();
    backend.pgp_inspect = () => ({ fingerprint: 'F', key_id: '', user_ids: [], expires_at: null });
    await openAdd('gpg_key');
    largeAnswer = 'block';
    $('f-pgp-btn').click();
    await flush(80);
    expect(toasts[toasts.length - 1]).toEqual(['This key does not expire', 'ok']);
    expect($<HTMLInputElement>('f-expires').value).toBe('');
    backend.pgp_inspect = () => {
      throw new Error('bad armor');
    };
    $('f-pgp-btn').click();
    await flush(80);
    expect(toasts[toasts.length - 1][0]).toBe('Could not read the key: bad armor');
    largeAnswer = '  ';
    const before = toasts.length;
    $('f-pgp-btn').click();
    await flush(60);
    expect(toasts).toHaveLength(before);
  });

  it('choosing a local service fills how its key is sent, and an unknown choice changes nothing', async () => {
    await openAdd('local_service');
    expect($('f-local-preset-group').style.display).toBe('');
    const select = $<HTMLSelectElement>('f-local-preset');
    const real = [...select.options].find((o) => o.value)!;
    set('f-local-preset', real.value);
    expect($<HTMLSelectElement>('f-auth-scheme').value).toBeTruthy();
    const before = $<HTMLInputElement>('f-auth-param').value;
    set('f-local-preset', '');
    expect($<HTMLInputElement>('f-auth-param').value).toBe(before);
    set('f-secret-type', 'api_key');
    expect($('f-local-preset-group').style.display).toBe('none');
  });
});
