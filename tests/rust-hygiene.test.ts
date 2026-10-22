// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
// Source-level rules for the Rust crates that a compiler does not enforce.
import fs from 'node:fs';
import path from 'node:path';
import { describe, expect, it } from 'vitest';

const root = path.resolve(__dirname, '..');

function rustFiles(dir: string): string[] {
  return fs.readdirSync(dir, { withFileTypes: true }).flatMap((e) => {
    const p = path.join(dir, e.name);
    if (e.isDirectory()) return e.name === 'tests' || e.name === 'target' ? [] : rustFiles(p);
    return p.endsWith('.rs') && !p.endsWith('_tests.rs') ? [p] : [];
  });
}

describe('temporary files in production code', () => {
  // A path in the shared temp directory is guessable and another user can plant something there
  // first. Production code that uses one must create it with `create_private_dir` or
  // `write_secret_file` (owner-only from the first byte, and refusing an existing path).
  const SAFE_HELPERS = /create_private_dir|write_secret_file/;

  it('only builds a temp path where the same file creates it privately', () => {
    const bad: string[] = [];
    for (const crate of ['vault-core', 'unv-app', 'unv-cli', 'unv-server', 'src-tauri']) {
      for (const f of rustFiles(path.join(root, crate, 'src'))) {
        const text = fs.readFileSync(f, 'utf8');
        const cut = text.search(/#\[cfg\((all\()?test\b/);
        const prod = cut < 0 ? text : text.slice(0, cut);
        if (prod.includes('temp_dir()') && !SAFE_HELPERS.test(prod))
          bad.push(path.relative(root, f));
      }
    }
    expect(bad).toEqual([]);
  });
});
