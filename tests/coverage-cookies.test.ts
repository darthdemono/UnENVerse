/** Branch coverage for cookies.ts: every parser fallback, exporter and the vault round trip. */
import { describe, it, expect } from 'vitest';
import {
  parseCookieHeader,
  parseCookiesTxt,
  parseCookieJson,
  parseAnyCookies,
  toCookiesTxt,
  toCookieJson,
  toPlaywrightStorageState,
  bareCookieName,
  cookiesOf,
  cookiesToExtraVars,
} from '../src/ts/cookies';

const entry = (o: object) => o as any;

describe('cookie parsers', () => {
  it('header: null, prefix, junk pieces', () => {
    expect(parseCookieHeader(null as any)).toEqual([]);
    expect(parseCookieHeader('Cookie: a=1; ;=2; junk; b=x=y')).toEqual([
      { name: 'a', value: '1' },
      { name: 'b', value: 'x=y' },
    ]);
  });

  it('cookies.txt: null, comments, httponly, short rows, expiry, tabs in values', () => {
    expect(parseCookiesTxt(null as any)).toEqual([]);
    const txt = [
      '# c',
      '',
      '#HttpOnly_.h.com\tTRUE\t/\tTRUE\t99\tn\tv\tw',
      'short\trow',
      'h\tFALSE\t/p\tfalse\tx\tm\tz',
    ].join('\n');
    const c = parseCookiesTxt(txt);
    expect(c[0]).toMatchObject({
      domain: '.h.com',
      secure: true,
      expires: 99,
      http_only: true,
      value: 'v\tw',
    });
    expect(c[1]).toMatchObject({ secure: false, expires: 0, http_only: undefined });
    expect(c).toHaveLength(2);
  });

  it('json: array, wrapped, garbage, odd rows, flag spellings', () => {
    expect(parseCookieJson('nope')).toEqual([]);
    expect(parseCookieJson('{"x":1}')).toEqual([]);
    expect(parseCookieJson('null')).toEqual([]);
    const rows = parseCookieJson(
      JSON.stringify({
        cookies: [
          { name: 'a' },
          { Name: 'b', Value: 'v', Secure: true, HttpOnly: true, expiry: 5.7 },
          { name: 1 },
          { name: '' },
        ],
      }),
    );
    expect(rows[0]).toMatchObject({ name: 'a', value: '' });
    expect(rows[1]).toMatchObject({
      name: 'b',
      value: 'v',
      secure: true,
      http_only: true,
      expires: 5,
    });
    expect(rows).toHaveLength(2);
    expect(
      parseCookieJson(
        '[{"name":"z","value":"1","domain":"d","path":"/p","secure":true,"httpOnly":true,"expirationDate":9}]',
      )[0],
    ).toMatchObject({ domain: 'd', expires: 9 });
  });

  it('any: empty, json, txt, header, and the fallbacks when the first guess yields nothing', () => {
    expect(parseAnyCookies(null as any)).toEqual([]);
    expect(parseAnyCookies('   ')).toEqual([]);
    expect(parseAnyCookies('[{"name":"a"}]')).toHaveLength(1);
    expect(parseAnyCookies('[nothing\ta=1')).toEqual([{ name: '[nothing\ta', value: '1' }]);
    expect(parseAnyCookies('h\tFALSE\t/\tFALSE\t0\tn\tv')).toHaveLength(1);
    expect(parseAnyCookies('a\tb=1')).toEqual([{ name: 'a\tb', value: '1' }]);
    expect(parseAnyCookies('a=1; b=2')).toHaveLength(2);
  });
});

describe('cookie writers', () => {
  it('cookies.txt refuses a bare jar, then writes httponly, dotted and session rows', () => {
    expect(() => toCookiesTxt([{ name: 'a', value: '1' }])).toThrow(/domain and path/);
    expect(() => toCookiesTxt([{ name: 'a', value: '1', domain: 'd' }])).toThrow(/needs path/);
    const out = toCookiesTxt([
      { name: 'a', value: '1', domain: '.d', path: '/', secure: true, http_only: true, expires: 7 },
      { name: 'b', value: '2', domain: 'e', path: '/' },
    ]);
    expect(out).toContain('#HttpOnly_.d\tTRUE\t/\tTRUE\t7\ta\t1');
    expect(out).toContain('e\tFALSE\t/\tFALSE\t0\tb\t2');
  });

  it('json export defaults absent attributes', () => {
    const rows = JSON.parse(
      toCookieJson([
        { name: 'a', value: '1' },
        { name: 'b', value: '2', domain: 'd', path: '/x', expires: 3 },
      ]),
    );
    expect(rows[0]).toMatchObject({ domain: '', path: '/', expirationDate: 0 });
    expect(rows[1]).toMatchObject({ domain: 'd', path: '/x', expirationDate: 3 });
  });
});

describe('playwright storage state', () => {
  const jar = [
    { key: 'a', value: '1' },
    {
      key: 'b',
      value: '2',
      attrs: { domain: 'c.test', path: '/p', expires: 10, secure: true, http_only: true },
    },
  ];

  it('uses the session url host for cookies without a domain', () => {
    const out = JSON.parse(
      toPlaywrightStorageState(
        entry({
          api_url: 'https://site.test/x',
          extra_vars: jar,
          storage_tokens: [
            { storage: 'local', origin: 'https://o.test/p', key: 'k', value: 'v' },
            { storage: 'local', origin: 'https://o.test', key: 'k2', value: 'v2' },
          ],
        }),
      ),
    );
    expect(out.cookies[0]).toMatchObject({
      domain: 'site.test',
      path: '/',
      expires: -1,
      httpOnly: false,
      secure: false,
    });
    expect(out.cookies[1]).toMatchObject({
      domain: 'c.test',
      path: '/p',
      expires: 10,
      httpOnly: true,
      secure: true,
    });
    expect(out.origins).toEqual([
      {
        origin: 'https://o.test',
        localStorage: [
          { name: 'k', value: 'v' },
          { name: 'k2', value: 'v2' },
        ],
      },
    ]);
  });

  it('fails loudly instead of losing data', () => {
    expect(() =>
      toPlaywrightStorageState(
        entry({
          storage_tokens: [{ storage: 'session', origin: 'https://a', key: 'k', value: 'v' }],
        }),
      ),
    ).toThrow(/sessionStorage/);
    expect(() =>
      toPlaywrightStorageState(entry({ api_url: 'not a url', extra_vars: jar })),
    ).toThrow(/needs a session URL/);
    expect(() =>
      toPlaywrightStorageState(
        entry({ api_url: 'ftp://x', extra_vars: [{ key: 'a', value: '1' }] }),
      ),
    ).toThrow(/needs a session URL/);
    expect(() => toPlaywrightStorageState(entry({ extra_vars: [{ key: 'a' }] }))).toThrow(
      /needs a session URL/,
    );
    expect(() =>
      toPlaywrightStorageState(
        entry({ storage_tokens: [{ storage: 'local', origin: 'ftp://x', key: 'k', value: 'v' }] }),
      ),
    ).toThrow(/Invalid storage-token origin/);
    expect(() =>
      toPlaywrightStorageState(
        entry({ storage_tokens: [{ storage: 'local', origin: 'junk', key: 'k', value: 'v' }] }),
      ),
    ).toThrow(/Invalid storage-token origin: junk/);
    expect(JSON.parse(toPlaywrightStorageState(entry({})))).toEqual({ cookies: [], origins: [] });
  });
});

describe('vault entry mapping', () => {
  it('strips prefixes, tolerates null', () => {
    expect(bareCookieName('__Host-sid')).toBe('sid');
    expect(bareCookieName(null as any)).toBe('');
  });

  it('prefers split cookies, falls back to the header, and drops keyless rows', () => {
    expect(
      cookiesOf(
        entry({
          extra_vars: [
            { key: 'a', value: '1', attrs: { domain: 'd' } },
            { key: '', value: 'x' },
            { key: 'n' },
          ],
        }),
      ),
    ).toEqual([
      {
        name: 'a',
        value: '1',
        domain: 'd',
        path: undefined,
        secure: undefined,
        http_only: undefined,
        expires: undefined,
      },
      {
        name: 'n',
        value: '',
        domain: undefined,
        path: undefined,
        secure: undefined,
        http_only: undefined,
        expires: undefined,
      },
    ]);
    expect(cookiesOf(entry({ api_key: 'a=1' }))).toEqual([{ name: 'a', value: '1' }]);
    expect(cookiesOf(entry({}))).toEqual([]);
  });

  it('turns cookies into extra vars with only the attributes present', () => {
    const rows = cookiesToExtraVars([
      { name: 'a', value: '1' },
      { name: 'b', value: '2', domain: 'd', path: '/', secure: true, http_only: true, expires: 4 },
    ]);
    expect(rows[0]).toEqual({ key: 'a', value: '1', secret: true });
    expect(rows[1].attrs).toEqual({
      domain: 'd',
      path: '/',
      secure: true,
      http_only: true,
      expires: 4,
    });
  });
});
