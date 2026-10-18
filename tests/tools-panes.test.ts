// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
/**
 * The Tools panel's small panes: generators, hash, JWT decoder, converters,
 * certificate and SSH key generation, the activity-bar keyboard, the sidebar
 * section toggles and restoring the last pane. Each pane is a button that must
 * say what it did, including when it could do nothing.
 */
import { describe, it, expect, beforeAll, beforeEach, afterEach, vi } from 'vitest';
import { st, resetViewState, Settings } from '../src/ts/state';
import { loadRealIndexHtml, resetState } from './helpers';

const toasts: [string, string?][] = [];
const copied: string[] = [];
vi.mock('../src/ts/utils', async (importOriginal) => {
  const real = await importOriginal<typeof import('../src/ts/utils')>();
  return {
    ...real,
    showToast: (m: string, k?: string) => toasts.push([m, k]),
    clipboardWrite: async (t: string) => void copied.push(t),
  };
});

const $ = <T extends HTMLElement = HTMLElement>(id: string) => document.getElementById(id) as T;
const flush = (ms = 30) => new Promise((r) => setTimeout(r, ms));
let boundNodes: ChildNode[] = [];

beforeAll(async () => {
  loadRealIndexHtml();
  (await import('../src/ts/tools-markup')).mountToolsPanes();
  (await import('../src/ts/tools')).initTools();
  boundNodes = Array.from(document.body.childNodes);
});

beforeEach(() => {
  document.body.innerHTML = '';
  for (const n of boundNodes) document.body.appendChild(n);
  resetState(st);
  resetViewState();
  toasts.length = 0;
  copied.length = 0;
  delete (window as any).__TAURI__;
});

afterEach(() => {
  delete (window as any).__TAURI__;
});

const press = (k: string, target: HTMLElement) =>
  target.dispatchEvent(new KeyboardEvent('keydown', { key: k, bubbles: true, cancelable: true }));

describe('the activity bar', () => {
  const tabs = () => [...document.querySelectorAll<HTMLButtonElement>('.activity-btn')];

  it('arrow keys move to the next and previous tab, wrapping, and Home and End go to the ends', () => {
    const t = tabs();
    press('ArrowDown', t[0]);
    expect(document.activeElement).toBe(t[1]);
    press('ArrowLeft', t[1]);
    expect(document.activeElement).toBe(t[0]);
    press('ArrowUp', t[0]);
    expect(document.activeElement).toBe(t[t.length - 1]);
    press('ArrowRight', t[t.length - 1]);
    expect(document.activeElement).toBe(t[0]);
    press('End', t[0]);
    expect(document.activeElement).toBe(t[t.length - 1]);
    press('Home', t[t.length - 1]);
    expect(document.activeElement).toBe(t[0]);
  });

  it('other keys leave the focus alone', () => {
    const t = tabs();
    t[0].focus();
    const ev = new KeyboardEvent('keydown', { key: 'x', bubbles: true, cancelable: true });
    t[0].dispatchEvent(ev);
    expect(ev.defaultPrevented).toBe(false);
    expect(document.activeElement).toBe(t[0]);
  });
});

describe('the sidebar section toggles', () => {
  it('collapse a section, tell assistive tech, and remember it; a second press opens it again', () => {
    const btn = document.querySelector<HTMLButtonElement>(
      '.section-collapse-btn[data-section="price"]',
    )!;
    const section = $('sidebar-section-price');
    btn.click();
    expect(section.classList.contains('collapsed')).toBe(true);
    expect(btn.getAttribute('aria-expanded')).toBe('false');
    expect(Settings.get('collapsedSections')).toContain('price');
    btn.click();
    expect(section.classList.contains('collapsed')).toBe(false);
    expect(btn.getAttribute('aria-expanded')).toBe('true');
    expect(Settings.get('collapsedSections')).not.toContain('price');
  });
});

describe('the generators', () => {
  it('secret bytes: hex of the chosen length, copied or sent to the open form', async () => {
    document.querySelectorAll<HTMLButtonElement>('.tool-byte-btn')[0].click();
    ($('sg-format') as unknown as HTMLSelectElement).value = 'hex';
    $('sg-generate').click();
    const out = $<HTMLTextAreaElement>('sg-output').value;
    expect(out).toMatch(/^[0-9a-f]+$/);
    $('sg-copy').click();
    await flush();
    expect(copied).toEqual([out]);
    $('sg-inject').click();
    $<HTMLTextAreaElement>('sg-output').value = '';
    $('sg-inject').click();
  });

  it('every generator copy says "Nothing to copy yet" while its output is empty', () => {
    for (const id of [
      'sg-copy',
      'pg-copy',
      'ak-copy',
      'hg-copy',
      'uu-copy',
      'st-copy',
      'b64-copy',
    ]) {
      toasts.length = 0;
      $(id).click();
      expect(toasts, id).toContainEqual(['Nothing to copy yet', 'err']);
    }
  });

  it('a password: refuses with no character set, otherwise honours the length and shows its strength', () => {
    const set = (id: string, on: boolean) => ($<HTMLInputElement>(id).checked = on);
    for (const id of ['pg-upper', 'pg-lower', 'pg-digits', 'pg-symbols', 'pg-noambig'])
      set(id, false);
    $('pg-generate').click();
    expect(toasts).toContainEqual(['Select at least one character set', 'err']);

    const fill = $('pg-strength-fill');
    const length = $<HTMLInputElement>('pg-length');
    const run = (len: number) => {
      length.value = String(len);
      length.dispatchEvent(new Event('input'));
      $('pg-generate').click();
      return $<HTMLInputElement>('pg-output').value;
    };
    set('pg-digits', true);
    expect(run(8)).toMatch(/^\d{8}$/);
    expect($('pg-len-display').textContent).toBe('8');
    expect(fill.style.background).toBe('rgb(238, 68, 68)');
    set('pg-digits', false);
    set('pg-lower', true);
    run(12);
    expect(fill.style.background).toBe('rgb(255, 170, 0)');
    set('pg-upper', true);
    set('pg-digits', true);
    run(16);
    expect(fill.style.background).toBe('rgb(68, 170, 255)');
    run(40);
    expect(fill.style.background).toBe('rgb(0, 204, 153)');
    $('pg-inject').click();
  });

  it('an API key in a known pattern', () => {
    $('ak-generate').click();
    expect($<HTMLInputElement>('ak-output').value.length).toBeGreaterThan(8);
    $('ak-inject').click();
  });

  it('hashes the input with the chosen algorithm and format', async () => {
    $<HTMLTextAreaElement>('hg-input').value = 'abc';
    $('hg-hash').click();
    await flush();
    expect($<HTMLInputElement>('hg-output').value).toBe(
      'ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad',
    );
    document.querySelectorAll<HTMLButtonElement>('.tool-hash-algo-btn')[0].click();
    document.querySelectorAll<HTMLButtonElement>('.tool-hash-fmt-btn')[0].click();
    $('hg-hash').click();
    await flush();
    $('hg-copy').click();
    $('hg-inject').click();
  });

  it('UUIDs and ULIDs, one per line, in the requested number', () => {
    $<HTMLInputElement>('uu-count').value = '3';
    $('uu-generate').click();
    const lines = $<HTMLTextAreaElement>('uu-output').value.split('\n');
    expect(lines).toHaveLength(3);
    expect(lines.every((l) => /^[0-9a-f-]{36}$/.test(l))).toBe(true);
    document.querySelector<HTMLButtonElement>('.tool-id-type-btn:not(.active)')!.click();
    $<HTMLInputElement>('uu-count').value = '';
    $('uu-generate').click();
    expect($<HTMLTextAreaElement>('uu-output').value).toMatch(/^[0-9A-Z]{26}$/);
    $('uu-copy').click();
  });

  it('a source that is still filling says so instead of falling back to the OS generator', async () => {
    const { setEntropySource } = await import('../src/ts/generators');
    await setEntropySource('hardware').catch(() => {});
    $('sg-generate').click();
    $('pg-generate').click();
    $('ak-generate').click();
    const messages = toasts.filter(([, k]) => k === 'err').map(([m]) => m);
    expect(messages.filter((m) => m.includes('still filling'))).toHaveLength(3);
    await setEntropySource('os');
  });
});

describe('the JWT decoder', () => {
  const b64 = (o: object) =>
    btoa(JSON.stringify(o)).replace(/\+/g, '-').replace(/\//g, '_').replace(/=/g, '');
  const decode = (jwt: string) => {
    $<HTMLTextAreaElement>('tv-input').value = jwt;
    $('tv-decode').click();
  };

  it('shows header and payload and whether the token is still valid', () => {
    const future = Math.floor(Date.now() / 1000) + 3600;
    decode(`${b64({ alg: 'HS256' })}.${b64({ exp: future, sub: 'u' })}.sig`);
    expect($('tv-header').textContent).toContain('HS256');
    expect($('tv-status').textContent).toMatch(/^Valid until: /);
    expect($('tv-status').className).toBe('tool-status ok');
    decode(`${b64({ alg: 'HS256' })}.${b64({ exp: 1000 })}.sig`);
    expect($('tv-status').textContent).toMatch(/^Expired: /);
    decode(`${b64({ alg: 'HS256' })}.${b64({ sub: 'u' })}.sig`);
    expect($('tv-status').textContent).toBe('No expiry claim (exp) found');
  });

  it('says what is wrong with something that is not a token', () => {
    decode('not.a');
    expect($('tv-status').textContent).toBe('Invalid JWT: expected 3 parts');
    expect($('tv-header').textContent).toBe('');
    decode('%%%.%%%.%%%');
    expect($('tv-status').textContent).toBe('Failed to decode JWT');
  });

  it('copies the header or payload once decoded, and says to decode first otherwise', async () => {
    $('tv-copy-header').click();
    $('tv-copy-payload').click();
    expect(toasts.filter(([m]) => m === 'Decode a JWT first')).toHaveLength(2);
    decode(`${b64({ alg: 'HS256' })}.${b64({ sub: 'u' })}.sig`);
    $('tv-copy-header').click();
    $('tv-copy-payload').click();
    await flush();
    expect(toasts).toContainEqual(['Header copied ✓', 'ok']);
    expect(toasts).toContainEqual(['Payload copied ✓', 'ok']);
  });
});

describe('certificate and SSH key generation', () => {
  const desktop = (handlers: Record<string, (a: any) => unknown>) => {
    (window as any).__TAURI__ = {
      core: {
        invoke: async (cmd: string, args: any) => {
          const fn = handlers[cmd];
          if (!fn) throw new Error(`no ${cmd}`);
          return fn(args);
        },
      },
    };
  };

  it('both need the desktop app', async () => {
    $('pc-generate').click();
    $('sk-generate').click();
    await flush();
    expect(toasts.filter(([m]) => m === 'Tauri not available')).toHaveLength(2);
  });

  it('they need the bridge when the page loaded, so these are exercised through a fresh init', async () => {
    vi.resetModules();
    loadRealIndexHtml();
    desktop({
      generate_certificate: (a) => ({
        cert_pem: `CERT ${a.commonName} ${a.validityDays}`,
        key_pem: 'KEY',
      }),
      generate_ssh_keypair: (a) => ({ public_key: `PUB ${a.comment}`, private_key: 'PRIV' }),
    });
    (await import('../src/ts/tools-markup')).mountToolsPanes();
    (await import('../src/ts/tools')).initTools();
    const days = $<HTMLInputElement>('pc-days');
    days.value = '30';
    days.dispatchEvent(new Event('input'));
    expect($('pc-days-display').textContent).toBe('30');
    $<HTMLInputElement>('pc-cn').value = '';
    $('pc-generate').click();
    await flush();
    expect($<HTMLTextAreaElement>('pc-cert-output').value).toBe('CERT localhost 30');
    expect($('pc-loading').style.display).toBe('none');
    $<HTMLInputElement>('sk-comment').value = ' me@host ';
    $('sk-generate').click();
    await flush();
    expect($<HTMLTextAreaElement>('sk-pub-output').value).toBe('PUB me@host');
    expect($<HTMLTextAreaElement>('sk-priv-output').value).toBe('PRIV');
    for (const id of ['pc-copy-cert', 'pc-copy-key', 'sk-copy-pub', 'sk-copy-priv']) $(id).click();
    await flush();
    expect(copied).toEqual(['CERT localhost 30', 'KEY', 'PUB me@host', 'PRIV']);
  });

  it('a failing generator is reported and the spinner goes away', async () => {
    vi.resetModules();
    loadRealIndexHtml();
    desktop({
      generate_certificate: () => {
        throw new Error('no openssl');
      },
      generate_ssh_keypair: () => {
        throw new Error('no entropy');
      },
    });
    (await import('../src/ts/tools-markup')).mountToolsPanes();
    (await import('../src/ts/tools')).initTools();
    $('pc-generate').click();
    $('sk-generate').click();
    await flush();
    expect(toasts.map(([m]) => m)).toEqual(
      expect.arrayContaining(['Error: no openssl', 'Error: no entropy']),
    );
    expect($('pc-loading').style.display).toBe('none');
    expect($('sk-loading').style.display).toBe('none');
  });
});

describe('the converters', () => {
  const convert = (op: string, input: string) => {
    ($('st-op') as unknown as HTMLSelectElement).value = op;
    $<HTMLTextAreaElement>('st-input').value = input;
    $('st-convert').click();
    return $<HTMLTextAreaElement>('st-output').value;
  };

  it.each([
    ['url-encode', 'a b&c', 'a%20b%26c'],
    ['url-decode', 'a%20b', 'a b'],
    ['dotenv-escape', 'plain', 'plain'],
    ['dotenv-escape', 'has space', '"has space"'],
    ['dotenv-escape', 'say "hi"\nthere', '"say \\"hi\\"\\nthere"'],
    ['shell-quote', "it's", "'it'\\''s'"],
    ['json-escape', 'a"b\nc', 'a\\"b\\nc'],
    ['json-unescape', 'a\\"b\\nc', 'a"b\nc'],
  ])('%s turns %j into %j', (op, input, expected) => {
    expect(convert(op, input)).toBe(expected);
  });

  it('reports a conversion that cannot be done', () => {
    convert('url-decode', '%E0%A4%A');
    expect(toasts[toasts.length - 1][0]).toMatch(/^Conversion error: /);
  });

  it('Base64 encodes and decodes text, in standard or URL-safe form', () => {
    const run = (input: string) => {
      $<HTMLTextAreaElement>('b64-input').value = input;
      $('b64-convert').click();
      return $<HTMLTextAreaElement>('b64-output').value;
    };
    expect(run('héllo ?>')).toBe(btoa(unescape(encodeURIComponent('héllo ?>'))));
    document.querySelector<HTMLButtonElement>('.tool-b64-var-btn[data-var="url"]')!.click();
    expect(run('??>>')).toBe('Pz8-Pg');
    document.querySelector<HTMLButtonElement>('.tool-b64-op-btn[data-op="decode"]')!.click();
    expect(run('Pz8-Pg')).toBe('??>>');
    document.querySelector<HTMLButtonElement>('.tool-b64-var-btn[data-var="std"]')!.click();
    run('!!!not base64!!!');
    expect(toasts[toasts.length - 1][0]).toMatch(/^Base64 error: /);
  });
});

describe('the cron explainer', () => {
  const explain = (expr: string) => {
    $<HTMLInputElement>('cron-input').value = expr;
    $('cron-parse').click();
    return $('cron-output').textContent!.replace(/\s+/g, ' ');
  };

  it('asks for an expression', () => {
    explain('');
    expect(toasts).toContainEqual(['Enter a cron expression', 'err']);
  });

  it('explains a normal expression field by field and lists the next fire times', () => {
    const out = explain('*/15 9-17 * * 1-5');
    expect(out).toContain('Minute: 0, 15, 30, 45');
    expect(out).toContain('Hour: 9, 10, 11, 12, 13, 14, 15, 16, 17');
    expect(out).toContain('Day: every day');
    expect(out).toContain('Weekday: Mon, Tue, Wed, Thu, Fri');
    expect(out).toContain('Next 5 fire times');
  });

  it('expands names like @daily, and treats @reboot as a one-off', () => {
    expect(explain('@daily')).toContain('→');
    expect(explain('@reboot')).toContain('@reboot — runs once when system starts');
  });

  it('collapses a field that matches everything, and summarises a long list', () => {
    expect(explain('0-59 0-23 1-31 1-12 0-7')).toContain('Minute: every minute');
    expect(explain('1,2,3,4,5,6,7,8,9,10,11,12,13,14 * * * *')).toContain('14 values (1 … 14)');
  });

  it('treats 7 as Sunday, steps from a start value, and names the months', () => {
    expect(explain('5/20 * * 1,12 7')).toContain('Minute: 5, 25, 45');
    expect(explain('0 0 1 1,12 7')).toContain('Month: Jan, Dec');
    expect(explain('0 0 1 1 7')).toContain('Weekday: Sun');
  });

  it('says which field is wrong, and refuses the wrong number of fields', () => {
    expect(explain('61 * * * *')).toContain('Minute: invalid (61)');
    expect(explain('1,,2 * * * *')).toContain('invalid');
    expect(explain('a * * * *')).toContain('invalid (a)');
    expect(explain('1-x * * * *')).toContain('invalid');
    expect(explain('*/0 * * * *')).toContain('invalid');
    expect(explain('5-2 * * * *')).toContain('invalid');
    expect(explain('* * * *')).toContain('Invalid: expected 5 fields (min hr dom mon dow)');
  });

  it('lists nothing to fire for an expression that can never match, and for one with a bad field', () => {
    expect(explain('0 0 31 2 *')).not.toContain('Next');
    expect(explain('61 0 * * *')).not.toContain('Next');
  });

  it('Enter in the box runs it', () => {
    $<HTMLInputElement>('cron-input').value = '0 0 * * *';
    press('Enter', $('cron-input'));
    expect($('cron-output').textContent).toContain('Minute');
    press('a', $('cron-input'));
  });

  it('a day-of-month and a weekday together mean either', () => {
    const out = explain('0 0 13 * 5');
    expect(out).toContain('Next 5 fire times');
  });
});

describe('the CIDR calculator', () => {
  it('Enter in the box runs it', () => {
    $<HTMLInputElement>('cidr-input').value = '10.0.0.0/24';
    press('Enter', $('cidr-input'));
    expect($('cidr-output').textContent).toContain('10.0.0.255');
    press('a', $('cidr-input'));
  });
});

describe('the JSON and YAML formatter', () => {
  const mode = (m: string) =>
    document.querySelector<HTMLButtonElement>(`.tool-fmt-btn[data-fmt="${m}"]`)!.click();
  const run = (id: string, input: string) => {
    $<HTMLTextAreaElement>('fmt-input').value = input;
    $(id).click();
    return $('fmt-status').textContent!;
  };

  it('reports bad input to each of format, validate and minify, in both modes', () => {
    mode('json');
    for (const id of ['fmt-format', 'fmt-validate', 'fmt-minify']) {
      const s = run(id, '{not json');
      expect(s, id).toMatch(/JSON/);
      expect($('fmt-status').className).toContain('err');
    }
    mode('yaml');
    for (const id of ['fmt-format', 'fmt-validate', 'fmt-minify']) {
      const s = run(id, 'a: [unclosed');
      expect(s, id).toMatch(/YAML/);
    }
  });

  it('formats, validates and minifies good input', () => {
    mode('json');
    expect(run('fmt-validate', '{"a":1}')).toBe('Valid JSON ✓');
    expect(run('fmt-minify', '{ "a": 1 }')).toBe('Minified ✓');
    expect($<HTMLTextAreaElement>('fmt-output').value).toBe('{"a":1}');
    mode('yaml');
    expect(run('fmt-validate', 'a: 1')).toBe('Valid YAML ✓');
    expect(run('fmt-minify', 'a: [1, 2]')).toBe('YAML minified (flow style) ✓');
    $('fmt-copy').click();
  });

  it('asks for text to minify', () => {
    $<HTMLTextAreaElement>('fmt-input').value = '  ';
    $('fmt-minify').click();
    expect(toasts).toContainEqual(['Enter text to minify', 'err']);
  });
});

describe('templates', () => {
  it('clicking a template opens the Add form with its defaults after a moment', async () => {
    vi.useFakeTimers();
    try {
      const btn = document.querySelector<HTMLElement>('#template-grid [data-tpl-id]')!;
      expect(btn).not.toBeNull();
      btn.click();
      await vi.advanceTimersByTimeAsync(150);
      expect(document.getElementById('modal-overlay')!.classList.contains('open')).toBe(true);
    } finally {
      vi.useRealTimers();
    }
  });

  it('a click in the grid that is not on a template, or on an unknown one, does nothing', () => {
    document.getElementById('modal-overlay')!.classList.remove('open');
    const grid =
      document.getElementById('template-grid') ??
      document.querySelector('[data-tpl-id]')!.parentElement!;
    grid.click();
    const odd = document.createElement('button');
    odd.dataset.tplId = 'no-such-template';
    grid.appendChild(odd);
    odd.click();
    expect(document.getElementById('modal-overlay')!.classList.contains('open')).toBe(false);
  });
});

describe('switching tools', () => {
  const nav = (tool: string) =>
    document.querySelector<HTMLButtonElement>(`.tool-nav-btn[data-tool="${tool}"]`)!;

  it('every tool button shows its own pane and remembers the choice', () => {
    for (const tool of ['audit', 'pools', 'timeline', 'diff', 'expiry-calendar']) {
      nav(tool).click();
      expect(Settings.get('activeTool')).toBe(tool);
    }
  });

  it('the audit tool refreshes its log when opened', () => {
    let refreshed = 0;
    $('audit-refresh').addEventListener('click', () => void refreshed++);
    nav('audit').click();
    expect(refreshed).toBe(1);
  });

  it.each(['timeline', 'pools', 'expiry-calendar'])(
    'starting with %s as the last tool paints it straight away',
    async (tool) => {
      vi.resetModules();
      loadRealIndexHtml();
      localStorage.setItem('unenverse-settings', JSON.stringify({ activeTool: tool }));
      const s = await import('../src/ts/state');
      await s.Settings.init();
      (await import('../src/ts/tools-markup')).mountToolsPanes();
      (await import('../src/ts/tools')).initTools();
      await flush(120);
      expect(s.Settings.get('activeTool')).toBe(tool);
      const painted: Record<string, string> = {
        timeline: 'tl-rows',
        pools: 'pools-body',
        'expiry-calendar': 'cal-grid',
      };
      expect(document.getElementById(painted[tool])).not.toBeNull();
    },
  );

  it('a stored users panel is not restored at start-up, because it would open on nothing', async () => {
    vi.resetModules();
    loadRealIndexHtml();
    localStorage.setItem('unenverse-settings', JSON.stringify({ activePanel: 'users' }));
    const s = await import('../src/ts/state');
    await s.Settings.init();
    (await import('../src/ts/tools-markup')).mountToolsPanes();
    (await import('../src/ts/tools')).initTools();
    expect(document.getElementById('secrets-panel')!.style.display).toBe('');
  });
});
