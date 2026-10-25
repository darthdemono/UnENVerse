// SPDX-License-Identifier: PolyForm-Strict-1.0.0
// Copyright Jubair Hasan.
/**
 * Branch coverage for small pure modules: file-cred, composite.
 * Each case names the shape of input that reaches the branch.
 */
import { describe, it, expect } from 'vitest';
import { fileContentsOf, fileEnvLine, isFileShaped } from '../src/ts/file-cred';
import { renderComposite, renderErrorMessage, compositePlaceholders } from '../src/ts/composite';

const e = (o: Record<string, unknown>) => o as any;

describe('file-cred', () => {
  it('picks blob, certificate, key in that order, else nothing', () => {
    expect(fileContentsOf(e({ blob_data: '{"a":1}' }))).toEqual({ text: '{"a":1}', ext: 'json' });
    expect(fileContentsOf(e({ blob_data: '[1]' }))?.ext).toBe('json');
    expect(fileContentsOf(e({ blob_data: '-----BEGIN X' }))?.ext).toBe('pem');
    expect(fileContentsOf(e({ blob_data: 'kind: Config' }))?.ext).toBe('yaml');
    expect(fileContentsOf(e({ blob_data: 'plain' }))?.ext).toBe('txt');
    expect(fileContentsOf(e({ certificate_data: 'c' }))).toEqual({ text: 'c', ext: 'pem' });
    expect(fileContentsOf(e({ cert_key_data: 'k' }))).toEqual({ text: 'k', ext: 'key' });
    expect(fileContentsOf(e({}))).toBeNull();
  });

  it('emits the path, never the bytes, and nothing without a mount path', () => {
    expect(fileEnvLine(e({ mount_path: '/run/x' }), 'CERT')).toBe('CERT=/run/x');
    expect(fileEnvLine(e({ blob_data: 'secret' }), 'CERT')).toBeNull();
    expect(isFileShaped(e({ mount_path: '/x' }))).toBe(true);
    expect(isFileShaped(e({}))).toBe(false);
  });
});

describe('composite', () => {
  const parts = [
    { key: 'user', value: 'a b' },
    { key: 'pw', value: 'p@ss/w?rd' },
    { key: 'host', value: 'h' },
    { key: 'q', value: 'x y' },
    { key: 'extra', value: 'z' },
  ];

  it('encodes by URL zone for url-shaped kinds', () => {
    const r = renderComposite(
      'db://{user}:{pw}@{host}/p{host}?{q}={q}&flag&k={q}#{q}',
      parts,
      'connection',
    );
    expect(r.ok).toBe(true);
    if (r.ok) {
      expect(r.result.text).toContain('a%20b:p%40ss%2Fw%3Frd@h/ph?x%20y=x%20y&flag&k=x%20y#x%20y');
      expect(r.result.unused).toEqual(['extra']);
      expect(r.result.used).toContain('user');
    }
  });

  it('classifies a placeholder with no scheme, an authority without userinfo, and a bare host', () => {
    const noScheme = renderComposite('{user}', parts, 'link');
    expect(noScheme.ok && noScheme.result.text).toBe('a b');
    const noUser = renderComposite('https://{host}', parts, 'link');
    expect(noUser.ok && noUser.result.text).toBe('https://h');
    const hostAfterAt = renderComposite('https://u@{user}/x', parts, 'link');
    expect(hostAfterAt.ok && hostAfterAt.result.text).toBe('https://u@a%20b/x');
  });

  it('renders raw for custom, and doubled braces are literal', () => {
    const r = renderComposite('{{x}} {user}', parts, 'custom');
    expect(r.ok && r.result.text).toBe('{x} a b');
    const unknown = renderComposite('{user}', parts, 'weird');
    expect(unknown.ok && unknown.result.text).toBe('a b');
  });

  it('reports each failure with a message', () => {
    const unfilled = renderComposite('{nope}', parts, 'custom');
    expect(!unfilled.ok && renderErrorMessage(unfilled.error)).toMatch(/no part named "nope"/);
    const open = renderComposite('a{user', parts, 'custom');
    expect(!open.ok && renderErrorMessage(open.error)).toMatch(/unbalanced '\{' at position 1/);
    const empty = renderComposite('a{}', parts, 'custom');
    expect(empty.ok).toBe(false);
    const ctl = renderComposite('x://{c}', [{ key: 'c', value: 'a\nb' }], 'link');
    expect(!ctl.ok && renderErrorMessage(ctl.error)).toMatch(/control character/);
    // raw kinds accept control characters
    expect(renderComposite('{c}', [{ key: 'c', value: 'a\nb' }], 'custom').ok).toBe(true);
  });

  it('lists placeholders once each and none for a broken template', () => {
    expect(compositePlaceholders('{a}{b}{a}')).toEqual(['a', 'b']);
    expect(compositePlaceholders('{a')).toEqual([]);
  });

  it('placeholders before the scheme end and a lone query name are unstructured / query_name', () => {
    const early = renderComposite('{pw}://h', parts, 'link');
    expect(early.ok && early.result.text).toBe('p@ss/w?rd://h');
    const lone = renderComposite('https://h/?{pw}', parts, 'link');
    expect(lone.ok && lone.result.text).toBe('https://h/?p%40ss%2Fw%3Frd');
  });

  it('counts a repeated placeholder as used once', () => {
    const r = renderComposite('{user}{user}', parts, 'custom');
    expect(r.ok && r.result.used).toEqual(['user']);
  });
});
