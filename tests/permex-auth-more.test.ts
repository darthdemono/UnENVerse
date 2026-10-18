/**
 * Permission expressions and request authentication, the parts a person meets
 * when something is mistyped or left out: the exact words of each parse error,
 * how a rule reads an entry that lacks the field it names, and every way an
 * entry's credential becomes a header, a query string or a curl command.
 */
import { describe, it, expect } from 'vitest';
import { parse, validate, evaluateSrc, globMatches } from '../src/ts/permex';
import {
  authSchemeOf,
  authParamOf,
  authHeaderFor,
  authHeaderValue,
  authQueryFor,
  authUrlFor,
  curlFor,
  shellQuote,
} from '../src/ts/auth-request';

const entry = (o: object = {}) => ({ provider: 'P', api_key: 'k', ...o }) as any;

describe('permission expressions: what a mistake says', () => {
  it.each([
    ['tag:a &', 'unexpected end of expression'],
    ['(tag:a', "missing closing ')'"],
    ['tag:a)', 'trailing input after the end of the expression'],
    [')', "unexpected ')'"],
    ['AND tag:a', 'expression begins with an operator'],
    ['bogus', "expected AND, OR, NOT or a `field:value` term, found 'bogus'"],
    [':x', "unexpected character ':'"],
  ])('%s → %s', (src, message) => {
    expect(validate(src)).toBe(message);
  });

  it('a blank rule is valid: it clears the restriction', () => {
    expect(validate('   ')).toBeNull();
    expect(validate('tag:a')).toBeNull();
  });

  it('accepts & and | as well as && and ||, and the words in any case', () => {
    const e = entry({ tags: ['a'], categories: ['c'] });
    expect(evaluateSrc('tag:a & category:c', e, [])).toBe(true);
    expect(evaluateSrc('tag:a && category:c', e, [])).toBe(true);
    expect(evaluateSrc('tag:x | category:c', e, [])).toBe(true);
    expect(evaluateSrc('tag:x || category:x', e, [])).toBe(false);
    expect(evaluateSrc('tag:x or category:c', e, [])).toBe(true);
    expect(evaluateSrc('NOT tag:x and tag:a', e, [])).toBe(true);
  });
});

describe('permission expressions: entries missing the field', () => {
  it('a rule on a field the entry lacks matches nothing, except a wildcard', () => {
    const bare = entry();
    expect(evaluateSrc('category:infra', bare, [])).toBe(false);
    expect(evaluateSrc('tag:prod', bare, [])).toBe(false);
    expect(evaluateSrc('env:production', bare, [])).toBe(false);
    expect(evaluateSrc('project:web', bare, [])).toBe(false);
    expect(evaluateSrc('category:*', bare, [])).toBe(true);
    expect(evaluateSrc('vault:anything', bare, [])).toBe(true);
  });

  it('type defaults to api_key', () => {
    expect(evaluateSrc('type:api_key', entry(), [])).toBe(true);
    expect(evaluateSrc('type:password', entry(), [])).toBe(false);
  });

  it('a project matches by id or name, never by the default project, and unknown ids match as themselves', () => {
    const projects = [{ id: 'p1', name: 'Web' }] as any;
    expect(evaluateSrc('project:Web', entry({ projectIds: ['p1'] }), projects)).toBe(true);
    expect(evaluateSrc('project:p1', entry({ projectIds: ['p1'] }), projects)).toBe(true);
    expect(evaluateSrc('project:Universal', entry({ projectIds: ['Universal'] }), projects)).toBe(
      false,
    );
    expect(evaluateSrc('project:ghost', entry({ projectIds: ['ghost'] }), projects)).toBe(true);
  });

  it('globs use * as a wildcard and match the whole value', () => {
    expect(globMatches('Inf*', 'Infra')).toBe(true);
    expect(globMatches('Inf', 'Infra')).toBe(false);
    expect(globMatches('*fra', 'Infra')).toBe(true);
  });
});

describe('how an entry authenticates a request', () => {
  it('unknown schemes read as bearer', () => {
    expect(authSchemeOf(entry({ auth_scheme: 'weird' }))).toBe('bearer');
    expect(authSchemeOf(entry())).toBe('bearer');
    expect(authSchemeOf(entry({ auth_scheme: 'HEADER' }))).toBe('header');
  });

  it('the default parameter name depends on the scheme', () => {
    expect(authParamOf(entry({ auth_scheme: 'header' }))).toBe('X-Api-Key');
    expect(authParamOf(entry({ auth_scheme: 'query' }))).toBe('api_key');
    expect(authParamOf(entry({ auth_scheme: 'cookie' }))).toBe('Cookie');
    expect(authParamOf(entry({ auth_scheme: 'bearer' }))).toBe('');
    expect(authParamOf(entry({ auth_scheme: 'header', auth_param: 'X-Mine' }))).toBe('X-Mine');
  });

  it('builds the header for each scheme, and none for query or an empty key', () => {
    expect(authHeaderFor(entry())).toEqual({ name: 'Authorization', value: 'Bearer k' });
    expect(authHeaderFor(entry({ auth_scheme: 'cookie' }))).toEqual({ name: 'Cookie', value: 'k' });
    expect(authHeaderFor(entry({ auth_scheme: 'query' }))).toBeNull();
    expect(authHeaderFor(entry({ api_key: '' }))).toBeNull();
    expect(authHeaderFor(entry({ api_key: undefined }))).toBeNull();
    expect(
      authHeaderFor(entry({ auth_scheme: 'basic', username: 'bob', api_key: 'pw' }))!.value,
    ).toBe(`Basic ${btoa('bob:pw')}`);
    expect(authHeaderFor(entry({ auth_scheme: 'basic', api_key: 'pw' }))!.value).toBe(
      `Basic ${btoa(':pw')}`,
    );
  });

  it('a header template is used only when it holds {key} and is safe', () => {
    const e = (t: string) => entry({ auth_scheme: 'header', auth_template: t });
    expect(authHeaderFor(e('Token {key}'))!.value).toBe('Token k');
    for (const bad of [
      'no placeholder',
      'line\nbreak {key}',
      'nul\0 {key}',
      'x'.repeat(130) + '{key}',
      '',
    ])
      expect(authHeaderValue(e(bad), 'k')).toBe('k');
  });

  it('the query form encodes name and value, and joins an existing query with &', () => {
    const e = entry({ auth_scheme: 'query', auth_param: 'a b', api_key: 'x&y' });
    expect(authQueryFor(e)).toBe('a%20b=x%26y');
    expect(authUrlFor(e, 'https://h.test/p')).toBe('https://h.test/p?a%20b=x%26y');
    expect(authUrlFor(e, 'https://h.test/p?z=1')).toBe('https://h.test/p?z=1&a%20b=x%26y');
    expect(authQueryFor(entry())).toBeNull();
    expect(authQueryFor(entry({ auth_scheme: 'query', api_key: '' }))).toBeNull();
    expect(authUrlFor(entry(), 'https://h.test')).toBe('https://h.test');
  });

  it('curl quotes every part, adds the user agent, and falls back to a harmless address', () => {
    expect(curlFor(entry({ api_key: "it's" }))).toBe(
      "curl -H 'Authorization: Bearer it'\\''s' 'https://example.invalid/'",
    );
    expect(
      curlFor(entry({ user_agent: 'UA/1', api_url: 'https://a.test' }), 'https://b.test'),
    ).toBe("curl -H 'Authorization: Bearer k' -H 'User-Agent: UA/1' 'https://b.test'");
    expect(shellQuote(undefined as unknown as string)).toBe("''");
  });

  it('a very long credential is encoded without blowing the stack', () => {
    const e = entry({ auth_scheme: 'basic', username: 'u', api_key: 'p'.repeat(200_000) });
    expect(authHeaderFor(e)!.value.length).toBeGreaterThan(200_000);
  });
});

describe('parse returns the structure it evaluates', () => {
  it('nests not, and, or by precedence', () => {
    const e = parse('tag:a OR tag:b AND NOT tag:c');
    expect(e.kind).toBe('or');
  });
});
