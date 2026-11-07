// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
// The page of import sources is generated from the command definitions (R24).
import fs from 'node:fs';
import path from 'node:path';
import { describe, expect, it } from 'vitest';
// @ts-expect-error plain .mjs without types
import * as gen from '../scripts/formats-status.mjs';

const { apply, generate, listAfter, render } = gen;
const root = path.resolve(__dirname, '..');

describe('formats page', () => {
  it('is up to date (run: node scripts/formats-status.mjs)', () => {
    const page = fs.readFileSync(path.join(root, 'book/src/formats-read.md'), 'utf8');
    expect(page === generate(), 'book/src/formats-read.md is stale').toBe(true);
  });

  it('reads the list that follows a marker and nothing else', () => {
    const src = 'A {\n #[arg(value_parser = ["x", "y"])]\n}\nB {\n #[arg(value_parser = ["z"])]\n}';
    expect(listAfter(src, 'A {')).toEqual(['x', 'y']);
    expect(listAfter(src, 'B {')).toEqual(['z']);
    expect(listAfter(src, 'Missing {')).toEqual([]);
    expect(listAfter('A { no list }', 'A {')).toEqual([]);
  });

  it('names the sources in the real command definitions', () => {
    const page = fs.readFileSync(path.join(root, 'book/src/formats-read.md'), 'utf8');
    for (const s of ['bitwarden', 'onepassword', 'keepass', 'openbao', 'aws-sm']) {
      expect(page).toContain(`\`${s}\``);
    }
  });

  it('renders one line per command and refuses a page without markers', () => {
    expect(render(['a', 'b'], ['c'])).toBe(
      '- `unv import-vault <source> <file>` reads: `a`, `b`.\n- `unv import-service <service>` reads: `c`.',
    );
    expect(() => apply('no markers', 'x')).toThrow(/markers/);
  });
});
