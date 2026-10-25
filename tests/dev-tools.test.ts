// SPDX-License-Identifier: PolyForm-Strict-1.0.0
// Copyright Jubair Hasan.
/**
 * The developer tools: generators, converters, the JWT decoder, the CIDR
 * calculator and the cron explainer.
 *
 * Most of these are answers a person will paste somewhere without checking, so
 * the tests use known results: the SHA-256 of "abc", the /26 a network engineer
 * can do in their head, a cron line whose meaning is not in doubt.
 */
import { describe, it, expect, beforeAll, beforeEach, afterEach, vi } from 'vitest';
import { loadRealIndexHtml } from './helpers';

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
const val = (id: string) => $<HTMLInputElement>(id).value;
const setv = (id: string, v: string) => {
  $<HTMLInputElement>(id).value = v;
};
const click = (id: string) => $(id).click();
const flush = (ms = 20) => new Promise((r) => setTimeout(r, ms));
const pick = (cls: string, attr: string, v: string) =>
  document.querySelector<HTMLElement>(`.${cls}[data-${attr}="${v}"]`)!.click();

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
  toasts.length = 0;
  copied.length = 0;
});

describe('secret generator', () => {
  it('makes the chosen number of random bytes in the chosen format', () => {
    pick('tool-byte-btn', 'bytes', '16');
    setv('sg-format', 'hex');
    click('sg-generate');
    expect(val('sg-output')).toMatch(/^[0-9a-f]{32}$/);
    pick('tool-byte-btn', 'bytes', '64');
    setv('sg-format', 'base64url');
    click('sg-generate');
    expect(val('sg-output')).toMatch(/^[A-Za-z0-9_-]{86}$/);
    setv('sg-format', 'base64');
    click('sg-generate');
    expect(val('sg-output')).toMatch(/^[A-Za-z0-9+/]{86}==$/);
    const first = val('sg-output');
    click('sg-generate');
    expect(val('sg-output')).not.toBe(first);
  });

  it('Copy says when there is nothing to copy, and copies what is shown', async () => {
    setv('sg-output', '');
    click('sg-copy');
    expect(toasts).toContainEqual(['Nothing to copy yet', 'err']);
    setv('sg-output', 'abc123');
    click('sg-copy');
    await flush();
    expect(copied).toEqual(['abc123']);
  });

  it('Inject asks you to open the form first, and fills the open form’s key field', () => {
    setv('sg-output', 'abc123');
    click('sg-inject');
    expect(toasts).toContainEqual(['Open the Add/Edit form first, then Inject', 'err']);
    $('modal-overlay').classList.add('open');
    click('sg-inject');
    expect(val('f-key')).toBe('abc123');
    expect(toasts).toContainEqual(['Injected into form', 'ok']);
    $('modal-overlay').classList.remove('open');
  });
});

describe('password generator', () => {
  const sets = (u: boolean, l: boolean, d: boolean, s: boolean, a = false) => {
    $<HTMLInputElement>('pg-upper').checked = u;
    $<HTMLInputElement>('pg-lower').checked = l;
    $<HTMLInputElement>('pg-digits').checked = d;
    $<HTMLInputElement>('pg-symbols').checked = s;
    $<HTMLInputElement>('pg-noambig').checked = a;
  };

  it('honours length and the chosen character sets', () => {
    setv('pg-length', '40');
    sets(false, false, true, false);
    click('pg-generate');
    expect(val('pg-output')).toMatch(/^\d{40}$/);
    sets(true, true, false, false, true);
    click('pg-generate');
    expect(val('pg-output')).toMatch(/^[A-HJ-NP-Za-hj-km-z]{40}$/);
  });

  it('shows the chosen length and a strength bar that grows with entropy', () => {
    const len = $<HTMLInputElement>('pg-length');
    len.value = '24';
    len.dispatchEvent(new Event('input'));
    expect($('pg-len-display').textContent).toBe('24');
    sets(false, false, true, false);
    setv('pg-length', '8');
    click('pg-generate');
    const weak = parseFloat($('pg-strength-fill').style.width);
    sets(true, true, true, true);
    setv('pg-length', '64');
    click('pg-generate');
    const strong = parseFloat($('pg-strength-fill').style.width);
    expect(strong).toBeGreaterThan(weak);
    expect(strong).toBeLessThanOrEqual(100);
  });

  it('refuses to generate from no character set at all', () => {
    sets(false, false, false, false);
    setv('pg-output', 'keep');
    click('pg-generate');
    expect(toasts).toContainEqual(['Select at least one character set', 'err']);
    expect(val('pg-output')).toBe('keep');
  });
});

describe('identifiers and key patterns', () => {
  it('makes the requested number of UUIDs or ULIDs, one per line', () => {
    setv('uu-count', '3');
    click('uu-generate');
    const uuids = val('uu-output').split('\n');
    expect(uuids).toHaveLength(3);
    uuids.forEach((u) =>
      expect(u).toMatch(/^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/),
    );
    pick('tool-id-type-btn', 'type', 'ulid');
    click('uu-generate');
    val('uu-output')
      .split('\n')
      .forEach((u) => expect(u).toMatch(/^[0-9A-HJKMNP-TV-Z]{26}$/));
    setv('uu-count', '');
    click('uu-generate');
    expect(val('uu-output').split('\n')).toHaveLength(1);
    pick('tool-id-type-btn', 'type', 'uuid');
  });

  it('API key patterns look like the keys they imitate', () => {
    for (const [pattern, re] of [
      ['hex-32', /^[0-9a-f]{64}$/],
      ['hex-16', /^[0-9a-f]{32}$/],
      ['sk-prefix', /^sk-[A-Za-z0-9_-]+$/],
      ['bearer', /^[A-Za-z0-9_-]+$/],
    ] as const) {
      setv('ak-pattern', pattern);
      click('ak-generate');
      expect(val('ak-output'), pattern).toMatch(re);
    }
  });
});

describe('hash generator', () => {
  it('hashes text with the chosen algorithm and format (SHA-256 of "abc" is a known value)', async () => {
    setv('hg-input', 'abc');
    click('hg-hash');
    await vi.waitFor(() =>
      expect(val('hg-output')).toBe(
        'ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad',
      ),
    );
    pick('tool-hash-fmt-btn', 'fmt', 'base64');
    click('hg-hash');
    await vi.waitFor(() =>
      expect(val('hg-output')).toBe('ungWv48Bz+pBQUDeXa4iI7ADYaOWF3qctBD/YfIAFa0='),
    );
    pick('tool-hash-fmt-btn', 'fmt', 'hex');
    pick('tool-hash-algo-btn', 'algo', 'SHA-512');
    click('hg-hash');
    await vi.waitFor(() => expect(val('hg-output')).toHaveLength(128));
    pick('tool-hash-algo-btn', 'algo', 'SHA-256');
  });
});

describe('JWT decoder', () => {
  const b64u = (o: object) =>
    btoa(JSON.stringify(o)).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
  const jwt = (payload: object) => `${b64u({ alg: 'HS256', typ: 'JWT' })}.${b64u(payload)}.sig`;

  it('shows header and payload and says whether the token is still valid', () => {
    const future = Math.floor(Date.now() / 1000) + 3600;
    setv('tv-input', jwt({ sub: 'me', exp: future }));
    click('tv-decode');
    expect(JSON.parse($('tv-header').textContent!)).toEqual({ alg: 'HS256', typ: 'JWT' });
    expect(JSON.parse($('tv-payload').textContent!).sub).toBe('me');
    expect($('tv-status').className).toContain('ok');
    expect($('tv-status').textContent).toMatch(/^Valid until: /);
    setv('tv-input', jwt({ exp: Math.floor(Date.now() / 1000) - 60 }));
    click('tv-decode');
    expect($('tv-status').className).toContain('err');
    expect($('tv-status').textContent).toMatch(/^Expired: /);
    setv('tv-input', jwt({ sub: 'x' }));
    click('tv-decode');
    expect($('tv-status').className).toContain('warn');
    expect($('tv-status').textContent).toContain('No expiry claim');
  });

  it('rejects anything that is not three dot-separated JSON parts, and clears the previous result', () => {
    setv('tv-input', jwt({ a: 1 }));
    click('tv-decode');
    setv('tv-input', 'only.two');
    click('tv-decode');
    expect($('tv-status').textContent).toBe('Invalid JWT: expected 3 parts');
    expect($('tv-header').textContent).toBe('');
    setv('tv-input', 'a.b.c');
    click('tv-decode');
    expect($('tv-status').textContent).toBe('Failed to decode JWT');
  });

  it('copies header and payload, or asks for a token first', async () => {
    $('tv-header').textContent = '';
    click('tv-copy-header');
    click('tv-copy-payload');
    expect(toasts.filter(([m]) => m === 'Decode a JWT first')).toHaveLength(2);
    setv('tv-input', jwt({ a: 1 }));
    click('tv-decode');
    click('tv-copy-header');
    click('tv-copy-payload');
    await flush();
    expect(copied).toHaveLength(2);
    expect(toasts).toContainEqual(['Header copied ✓', 'ok']);
  });
});

describe('string tools', () => {
  const run = (op: string, input: string) => {
    setv('st-op', op);
    setv('st-input', input);
    click('st-convert');
    return val('st-output');
  };

  it('encodes and decodes URLs', () => {
    expect(run('url-encode', 'a b&c=é/')).toBe('a%20b%26c%3D%C3%A9%2F');
    expect(run('url-decode', 'a%20b%26c')).toBe('a b&c');
  });

  it('quotes for a .env only when needed, and for a shell always, even with a quote inside', () => {
    expect(run('dotenv-escape', 'plain')).toBe('plain');
    expect(run('dotenv-escape', 'has space')).toBe('"has space"');
    expect(run('dotenv-escape', 'say "hi"\nbye')).toBe('"say \\"hi\\"\\nbye"');
    expect(run('shell-quote', "it's $HOME")).toBe("'it'\\''s $HOME'");
  });

  it('escapes and unescapes JSON string contents', () => {
    expect(run('json-escape', 'a"b\n\\')).toBe('a\\"b\\n\\\\');
    expect(run('json-unescape', 'a\\"b\\n')).toBe('a"b\n');
  });

  it('reports a conversion that cannot be done and leaves the output alone', () => {
    setv('st-output', 'previous');
    run('url-decode', '%E0%A4%A');
    expect(toasts.some(([m, k]) => m.startsWith('Conversion error') && k === 'err')).toBe(true);
    expect(val('st-output')).toBe('previous');
    run('json-unescape', '"bad');
    expect(toasts.filter(([m]) => m.startsWith('Conversion error')).length).toBe(2);
  });
});

describe('base64 tool', () => {
  const convert = (op: 'encode' | 'decode', variant: 'std' | 'url', input: string) => {
    pick('tool-b64-op-btn', 'op', op);
    pick('tool-b64-var-btn', 'var', variant);
    setv('b64-input', input);
    click('b64-convert');
    return val('b64-output');
  };

  it('round-trips text including non-Latin characters', () => {
    const text = 'héllo wörld ✓ 日本語';
    const enc = convert('encode', 'std', text);
    expect(enc).toBe(btoa(unescape(encodeURIComponent(text))));
    expect(convert('decode', 'std', enc)).toBe(text);
  });

  it('URL-safe output drops padding and uses - and _, and decodes back', () => {
    const enc = convert('encode', 'url', '??>>~~');
    expect(enc).toBe('Pz8-Pn5-');
    expect(convert('encode', 'url', 'a')).toBe('YQ');
    expect(convert('decode', 'url', 'Pz8-Pn5-')).toBe('??>>~~');
    expect(convert('decode', 'url', 'YQ')).toBe('a');
  });

  it('says so when the input cannot be decoded', () => {
    setv('b64-output', 'kept');
    convert('decode', 'std', '!!!not base64!!!');
    expect(toasts.some(([m, k]) => m.startsWith('Base64 error') && k === 'err')).toBe(true);
    expect(val('b64-output')).toBe('kept');
    pick('tool-b64-op-btn', 'op', 'encode');
    pick('tool-b64-var-btn', 'var', 'std');
  });
});

describe('CIDR calculator', () => {
  const calc = (input: string) => {
    setv('cidr-input', input);
    click('cidr-calc');
    return Object.fromEntries(
      [...$('cidr-output').querySelectorAll('tr')].map((r) => [
        r.children[0].textContent,
        r.children[1].textContent,
      ]),
    );
  };

  it('works out the range of a /26', () => {
    expect(calc('192.168.1.77/26')).toEqual({
      Network: '192.168.1.64/26',
      Broadcast: '192.168.1.127',
      'First host': '192.168.1.65',
      'Last host': '192.168.1.126',
      'Subnet mask': '255.255.255.192',
      'Usable hosts': '62',
    });
  });

  it('handles the edges: a /24, a point-to-point /31, a single host /32 and the whole internet /0', () => {
    expect(calc('10.0.0.5/24')['Usable hosts']).toBe('254');
    const p2p = calc('10.0.0.4/31');
    expect(p2p['First host']).toBe('10.0.0.4');
    expect(p2p['Last host']).toBe('10.0.0.5');
    expect(p2p['Usable hosts']).toBe('2');
    const one = calc('10.0.0.9/32');
    expect(one['Usable hosts']).toBe('1');
    expect(one['First host']).toBe('10.0.0.9');
    const all = calc('1.2.3.4/0');
    expect(all['Subnet mask']).toBe('0.0.0.0');
    expect(all.Broadcast).toBe('255.255.255.255');
    expect(all['Usable hosts']).toBe('4,294,967,294');
  });

  it('rejects anything that is not an IPv4 address with a valid prefix, and hides the old result', () => {
    calc('10.0.0.0/8');
    for (const bad of ['10.0.0.0', '10.0.0/8', '300.1.1.1/8', '10.0.0.0/33', 'abc', '']) {
      toasts.length = 0;
      calc(bad);
      expect(toasts, bad).toContainEqual(['Invalid CIDR — use format 192.168.1.0/24', 'err']);
      expect($('cidr-output').style.display, bad).toBe('none');
    }
  });

  it('Enter in the box calculates', () => {
    setv('cidr-input', '192.168.0.0/16');
    $('cidr-input').dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter' }));
    expect($('cidr-output').textContent).toContain('255.255.0.0');
  });
});

describe('cron explainer', () => {
  const explain = (expr: string) => {
    setv('cron-input', expr);
    click('cron-parse');
    return $('cron-output').textContent!.replace(/[ \t]+/g, ' ');
  };

  it('describes each field in words', () => {
    const t = explain('*/15 9-17 * 1,6 1-5');
    expect(t).toContain('Minute: 0, 15, 30, 45');
    expect(t).toContain('Hour: 9, 10, 11, 12, 13, 14, 15, 16, 17');
    expect(t).toContain('Day: every day');
    expect(t).toContain('Month: Jan, Jun');
    expect(t).toContain('Weekday: Mon, Tue, Wed, Thu, Fri');
  });

  it('expands the @shortcuts and shows what they mean', () => {
    const t = explain('@daily');
    expect(t).toContain('@daily → 0 0 * * *');
    expect(t).toContain('Minute: 0');
    expect(explain('@hourly')).toContain('0 * * * *');
    expect(explain('@reboot')).toContain('runs once when system starts');
  });

  it('treats 7 as Sunday, and long lists as a count', () => {
    expect(explain('0 0 * * 0,7')).toContain('Weekday: Sun');
    expect(explain('* * * * *')).toContain('Minute: every minute');
    expect(explain('1-40 * * * *')).toContain('40 values (1 … 40)');
  });

  it('flags a bad field without throwing away the others', () => {
    const t = explain('61 * * * *');
    expect(t).toContain('invalid (61)');
    expect(t).toContain('Hour: every hour');
    expect(explain('0 0 * *')).toContain('Invalid: expected 5 fields');
    expect(explain('*/0 * * * *')).toContain('invalid (*/0)');
    expect(explain('a-b * * * *')).toContain('invalid (a-b)');
    expect(explain('5-2 * * * *')).toContain('invalid (5-2)');
  });

  it('asks for an expression when the box is empty; Enter explains', () => {
    explain('');
    expect(toasts).toContainEqual(['Enter a cron expression', 'err']);
    setv('cron-input', '0 0 * * *');
    $('cron-input').dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter' }));
    expect($('cron-output').textContent).toContain('Minute');
  });

  describe('next fire times', () => {
    beforeEach(() => {
      vi.useFakeTimers();
      vi.setSystemTime(new Date(2026, 0, 31, 23, 58, 30));
    });
    afterEach(() => vi.useRealTimers());
    const fires = () =>
      [...$('cron-output').querySelectorAll('div[style*="font-size:11px"]')].map(
        (d) => new Date(d.textContent!),
      );

    it('lists the next five, in order, each matching the schedule', () => {
      explain('*/5 * * * *');
      const times = fires();
      expect(times).toHaveLength(5);
      times.forEach((t) => expect(t.getMinutes() % 5).toBe(0));
      for (let i = 1; i < times.length; i++)
        expect(times[i].getTime()).toBeGreaterThan(times[i - 1].getTime());
      expect(times[0].getTime()).toBeGreaterThan(Date.now());
    });

    it('a monthly job on the 31st skips the months that have no 31st', () => {
      explain('0 12 31 * *');
      const times = fires();
      expect(times.length).toBe(5);
      times.forEach((t) => expect(t.getDate()).toBe(31));
      expect(times.map((t) => t.getMonth() + 1)).toEqual([3, 5, 7, 8, 10]);
    });

    it('a leap-day job is found years out', () => {
      explain('0 0 29 2 *');
      const times = fires();
      expect(times.length).toBeGreaterThan(0);
      expect(times[0].getFullYear()).toBe(2028);
    });

    it('with both a day of the month and a weekday, either one matches (not both)', () => {
      explain('0 0 1 * 1');
      const times = fires();
      expect(times.length).toBe(5);
      times.forEach((t) => expect(t.getDate() === 1 || t.getDay() === 1).toBe(true));
      expect(times.some((t) => t.getDay() === 1 && t.getDate() !== 1)).toBe(true);
    });

    it('an impossible date yields no times rather than looping', () => {
      explain('0 0 31 2 *');
      expect(fires()).toHaveLength(0);
    });
  });
});

describe('JSON / YAML formatter', () => {
  const run = (btn: string, input: string) => {
    setv('fmt-input', input);
    click(btn);
    return {
      out: val('fmt-output'),
      status: $('fmt-status').textContent!,
      ok: $('fmt-status').className,
    };
  };

  it('pretty-prints and minifies JSON and counts lines', () => {
    pick('tool-fmt-btn', 'fmt', 'json');
    const f = run('fmt-format', '{"a":[1,2],"b":{"c":null}}');
    expect(f.out).toBe('{\n  "a": [\n    1,\n    2\n  ],\n  "b": {\n    "c": null\n  }\n}');
    expect(f.status).toBe('Valid JSON — 9 lines');
    expect(run('fmt-minify', '{ "a" : [ 1 , 2 ] }').out).toBe('{"a":[1,2]}');
    expect($('fmt-copy-row').style.display).toBe('');
  });

  it('names a JSON error and hides stale output', () => {
    run('fmt-format', '{"a":1}');
    const bad = run('fmt-format', '{"a":');
    expect(bad.ok).toContain('err');
    expect(bad.status).toContain('JSON parse error');
    expect($('fmt-output').style.display).toBe('none');
    expect($('fmt-copy-row').style.display).toBe('none');
    expect(run('fmt-validate', '[1,2]').status).toBe('Valid JSON ✓');
    expect(run('fmt-validate', '[1,').status).toContain('Invalid JSON');
  });

  it('formats, minifies and validates YAML', () => {
    pick('tool-fmt-btn', 'fmt', 'yaml');
    expect(run('fmt-format', 'a:   1\nb:\n    - x\n    - y').out).toBe('a: 1\nb:\n  - x\n  - y\n');
    expect(run('fmt-minify', 'a: 1\nb: [x, y]').out).toBe('{ a: 1, b: [ x, y ] }');
    expect(run('fmt-validate', 'a: 1').status).toBe('Valid YAML ✓');
    expect(run('fmt-validate', 'a: [unclosed').status).toContain('Invalid YAML');
    expect(run('fmt-format', 'a: [unclosed').status).toContain('YAML parse error');
    expect(run('fmt-minify', 'a: [unclosed').status).toContain('YAML parse error');
    pick('tool-fmt-btn', 'fmt', 'json');
  });

  it('asks for text first, for each action', () => {
    for (const [btn, word] of [
      ['fmt-format', 'format'],
      ['fmt-validate', 'validate'],
      ['fmt-minify', 'minify'],
    ] as const) {
      toasts.length = 0;
      run(btn, '   ');
      expect(toasts).toContainEqual([`Enter text to ${word}`, 'err']);
    }
  });

  it('copies the result', async () => {
    run('fmt-format', '{"a":1}');
    click('fmt-copy');
    await flush();
    expect(copied[0]).toContain('"a": 1');
  });
});
