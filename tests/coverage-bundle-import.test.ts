/** Branch coverage for bundle-import.ts: quoting, escapes, f-strings, duplicates and the public-value rules. */
import { describe, it, expect } from 'vitest';
import { importPythonConfig } from '../src/ts/bundle-import';

const one = (src: string) => importPythonConfig(src);
const first = (src: string) => one(src).vars[0];
const isPublic = (src: string) => first(src)?.public === true;

describe('quoting and escapes', () => {
  it('imports an unterminated or trailing-backslash string as source text with a warning', () => {
    for (const src of ['A = "abc', 'A = "abc\\', 'A = \'abc"', 'A = "abc" junk']) {
      const r = one(src);
      expect(r.vars[0].kind).toBe('string');
      expect(r.warnings.join('\n')).toMatch(/not a supported literal/);
    }
  });

  it('reads raw strings untouched, known escapes decoded, unknown ones refused', () => {
    expect(first('A = r"a\\qb"').value).toBe('a\\qb');
    expect(first('A = "a\\n\\t\\r\\\\\\"\\\'z"').value).toBe('a\n\t\r\\"\'z');
    const bad = one('A = "a\\qb"');
    expect(bad.warnings[0]).toMatch(/unsupported escape in A/);
    expect(bad.vars[0].value).toBe('"a\\qb"');
  });

  it('ignores lines that are not assignments and strips trailing comments', () => {
    const r = one('if x == 1:\n    pass\nA = 5  # note\nB = "x" # note');
    expect(r.vars.map((v) => v.key)).toEqual(['A', 'B']);
  });
});

describe('literals', () => {
  it('classifies ints, hex, float, bool, None, and unsupported source', () => {
    const r = one(
      [
        'H = 0xff',
        'I = -12',
        'F = 1.5e3',
        'T = True',
        'U = False',
        'N = None',
        'X = foo(1)',
        'S = "txt"',
      ].join('\n'),
    );
    const by = Object.fromEntries(r.vars.map((v) => [v.key, v]));
    expect(by.H.kind).toBe('hex_int');
    expect(by.I.kind).toBe('int');
    expect(by.F.kind).toBe('float');
    expect(by.T).toMatchObject({ kind: 'bool', value: 'true' });
    expect(by.U).toMatchObject({ kind: 'bool', value: 'false' });
    expect(by.N).toMatchObject({ kind: 'string', value: '' });
    expect(by.X.kind).toBe('string');
    expect(r.warnings.join('\n')).toMatch(/N is None/);
    expect(by.H.public && by.T.public && by.F.public).toBe(true);
    expect(by.I.public).toBe(true);
    expect(isPublic('A = 12345678')).toBe(false);
    expect(isPublic('A = -123456')).toBe(true);
  });

  it('tells ids, markdown links and urls from plain strings', () => {
    expect(first('A = "123456789012345"').kind).toBe('large_id');
    expect(first('user_id = "1234567890"').kind).toBe('large_id');
    expect(first('A = "[t](https://x.io/p)"').kind).toBe('markdown_link');
    expect(first('A = "https://x.io/p"').kind).toBe('url');
  });
});

describe('f-strings', () => {
  it('becomes a template over known names, keeps literal braces, plain f-string is a string', () => {
    const r = one('host = "h"\nU = f"https://{host}/{{lit}}"\nP = f"plain"\nB = f"a}}b{{c"');
    expect(r.vars[1]).toMatchObject({ kind: 'template', value: 'https://{host}/{{lit}}' });
    expect(r.vars[2].kind).toBe('string');
    expect(r.vars[3].kind).toBe('template');
  });

  it('refuses holes a template cannot hold', () => {
    for (const src of [
      'A = f"{x"',
      'A = f"{a b}"',
      'A = f"{missing}"',
      'A = f"stray } brace"',
      'A = f"{x!r}"',
    ]) {
      const r = one('x = 1\n' + src);
      expect(r.vars[1].kind).toBe('string');
      expect(r.warnings.join('\n')).toMatch(/f-string with an expression/);
    }
  });

  it('rewrites later references to a reassigned name and reports the count', () => {
    const r = one('k = "a"\nk = "b"\nU1 = f"{k}"\nU2 = f"{k}"\nV = f"{k}"');
    expect(r.vars.map((v) => v.key)).toEqual(['k', 'k_2', 'U1', 'U2', 'V']);
    expect(r.warnings.join('\n')).toMatch(/3 later references to k now use k_2/);
    const one1 = one('k = "a"\nk = "b"\nU = f"{k}"');
    expect(one1.warnings.join('\n')).toMatch(/1 later reference to k now uses k_2/);
    const triple = one('k = 1\nk = 2\nk = 3');
    expect(triple.vars.map((v) => v.key)).toEqual(['k', 'k_2', 'k_3']);
    const clash = one('k_2 = 1\nk = 1\nk = 2');
    expect(clash.vars.map((v) => v.key)).toEqual(['k_2', 'k', 'k_3']);
  });
});

describe('what is safe to print', () => {
  it('secret-looking names are never public', () => {
    expect(isPublic('api_token = 5')).toBe(false);
    expect(isPublic('title = "hi"')).toBe(true);
  });

  it('urls: credentials, hooks, token segments and secret-ish query keys make them private', () => {
    expect(isPublic('u = "http://host"')).toBe(true);
    expect(isPublic('u = "https://host/page?a=1&b"')).toBe(true);
    expect(isPublic('u = "https://user:pw@host/x"')).toBe(false);
    expect(isPublic('u = "https://host/hook/abc"')).toBe(false);
    expect(isPublic('u = "https://host/abcdefghijklmnopqrstuvwxyz0123456789"')).toBe(false);
    expect(isPublic('u = "https://host/x?token=1"')).toBe(false);
    expect(isPublic('u = "https://host/x?a=1;sig=2"')).toBe(false);
  });

  it('markdown links are judged by their target', () => {
    expect(isPublic('m = "[t](https://x.io/p)"')).toBe(true);
    expect(isPublic('m = "[t](https://x.io/p?token=1)"')).toBe(false);
  });

  it('templates are public only when every input and url is', () => {
    const r = one(
      'a = 5\nb = f"{a} hello {{x}}"\nc = f"{b} see https://h/p"\nd = f"https://h/?token={a}"\ne = f"{a}://x.io/hook/zz"',
    );
    expect(r.vars[1].public).toBe(true);
    expect(r.vars[2].public).toBe(true);
    expect(r.vars[3].public).toBeUndefined();
    expect(r.vars[4].public).toBeUndefined();
    const secretInput = one('tok = "abc"\nm = f"{tok}"');
    expect(secretInput.vars[1].public).toBeUndefined();
  });

  it('strings: short, bracketed url, prose, and long mixed tokens', () => {
    expect(isPublic('s = "ab"')).toBe(true);
    expect(isPublic('s = "<https://x.io/p>"')).toBe(true);
    expect(isPublic('s = "<https://x.io/hook/a>"')).toBe(false);
    expect(isPublic('s = "hello there friend"')).toBe(true);
    expect(isPublic('s = "hello a=b friend"')).toBe(false);
    expect(isPublic('s = "hello abcdefghij0123456789 friend"')).toBe(false);
    expect(isPublic('s = "singleword"')).toBe(false);
  });

  it('an unrecognised kind (large id) is never public', () => {
    expect(isPublic('i = "123456789012345678"')).toBe(false);
  });
});
