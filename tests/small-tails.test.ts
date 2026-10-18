/**
 * The edges of the small helper modules: what each says for input at the limit
 * of what it accepts. Each is a sentence a person could be shown.
 */
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { markUsed, codeStatus, nextCode } from '../src/ts/recovery-codes';
import { parseRateLimit } from '../src/ts/ratelimit';
import { planRestore, canRestore } from '../src/ts/restore-chunks';
import { emittersFor, findSecretType } from '../src/ts/secret-types';
import { html } from '../src/ts/html';
import { toCookiesTxt, parseAnyCookies } from '../src/ts/cookies';

const codes = (value: string) => ({ extra_vars: [{ key: 'codes', value }] }) as any;

describe('recovery codes', () => {
  it('marking the next code when every code is used says none remain', () => {
    expect(() => markUsed(codes('a\tUSED 2026-01-01'), null, '2026-02-02')).toThrow(
      'no unused codes remain',
    );
    expect(() => markUsed(codes('a'), 'zzz', '2026-02-02')).toThrow(
      'that code is not in this entry',
    );
    expect(() => markUsed(codes('a\tUSED 2026-01-01'), 'a', '2026-02-02')).toThrow(
      'already marked used',
    );
  });

  it('marks the named code or the first unused one, and counts them', () => {
    const e = codes('a\nb\nc');
    e.extra_vars[0].value = markUsed(e, 'b', '2026-02-02');
    expect(codeStatus(e)).toEqual({ total: 3, remaining: 2 });
    expect(nextCode(e)).toBe('a');
    e.extra_vars[0].value = markUsed(e, null, '2026-02-03');
    expect(nextCode(e)).toBe('c');
  });

  it('an entry with no codes variable has none', () => {
    expect(codeStatus({} as any)).toEqual({ total: 0, remaining: 0 });
    expect(nextCode({} as any)).toBeNull();
  });
});

describe('rate limits', () => {
  it.each([
    ['99999999999999999999 per minute', null],
    ['abc', null],
    ['', null],
    [42, null],
    ['100/min', { count: 100 }],
    ['1,000 per hour', { count: 1000 }],
  ])('%j', (raw, expected) => {
    const got = parseRateLimit(raw);
    if (expected === null) expect(got).toBeNull();
    else expect(got).toMatchObject({ count: (expected as { count: number }).count });
  });
});

describe('restoring chunks from an exported file', () => {
  it('a project with no chunk list simply replaces nothing, and a format with no reader is refused', () => {
    const p = { id: 'p', name: 'P' } as any;
    expect(planRestore(p, 'no-such-format', 'x')).toBeNull();
    expect(canRestore('no-such-format')).toBe(false);
    const plan = planRestore(p, 'wireguard', '[Interface]\nAddress = 10.0.0.1/24\n')!;
    expect(plan.parsed).toHaveLength(1);
    expect(plan.replaced).toEqual([]);
    expect(plan.kept).toEqual([]);
  });
});

describe('the secret-type registry', () => {
  it('an unknown or missing type offers no emitters', () => {
    expect(emittersFor(undefined)).toEqual([]);
    expect(emittersFor('')).toEqual([]);
    expect(emittersFor('no-such-type')).toEqual([]);
    expect(findSecretType('no-such-type')).toBeUndefined();
  });
});

describe('html', () => {
  it('with no interpolation is the text itself; false and null print nothing; numbers print', () => {
    expect(html`plain`.toString()).toBe('plain');
    expect(html`a${false}b${null}c${0}d`.toString()).toBe('abc0d');
  });
});

describe('cookies', () => {
  it('a header with a blank name is skipped, and a jar with paths and domains writes them', () => {
    expect(parseAnyCookies('=x; a=1')).toEqual([{ name: 'a', value: '1' }]);
    const txt = toCookiesTxt([{ name: 'sid', value: 'v', domain: 'site.test', path: '/p' } as any]);
    expect(txt).toContain('site.test\tFALSE\t/p\tFALSE\t0\tsid\tv');
  });
});

beforeEach(() => vi.restoreAllMocks());
