// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
import { describe, it, expect } from 'vitest';
// @ts-expect-error plain .mjs without types
import { buildManifest } from '../scripts/updater-manifest.mjs';

const names = [
  'UnENVerse_1.0.0_amd64.AppImage',
  'UnENVerse_1.0.0_aarch64.AppImage',
  'UnENVerse_1.0.0_x64-setup.exe',
  'UnENVerse_1.0.0_arm64-setup.exe',
];
const withSigs = (n: string[]) => [...n, ...n.map((f) => f + '.sig')];
const build = (files: string[], sig = (f: string) => 'sig-of-' + f) =>
  buildManifest({
    files,
    read: sig,
    version: '1.0.0',
    baseUrl: 'https://example.test/v1.0.0',
    notes: 'n',
    now: '2026-01-01T00:00:00.000Z',
  });

describe('updater manifest', () => {
  it('names every platform with its own bundle and signature', () => {
    const m = build(withSigs(names));
    expect(Object.keys(m.platforms).sort()).toEqual([
      'linux-aarch64',
      'linux-x86_64',
      'windows-aarch64',
      'windows-x86_64',
    ]);
    expect(m.platforms['linux-aarch64'].url).toBe(
      'https://example.test/v1.0.0/UnENVerse_1.0.0_aarch64.AppImage',
    );
    expect(m.platforms['windows-x86_64'].signature).toBe(
      'sig-of-UnENVerse_1.0.0_x64-setup.exe.sig',
    );
  });

  it('refuses a feed with a missing platform (control: this must fail)', () => {
    expect(() => build(withSigs(names.slice(1)))).toThrow('linux-x86_64');
  });

  it('refuses a bundle without a signature and an empty signature', () => {
    expect(() => build([...names, ...names.slice(1).map((f) => f + '.sig')])).toThrow(
      'linux-x86_64',
    );
    expect(() => build(withSigs(names), () => '  \n')).toThrow('no signed bundle');
  });
});
