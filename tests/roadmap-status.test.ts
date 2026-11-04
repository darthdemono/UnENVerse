// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
// The roadmap status is generated from roadmap-status.json into three pages. Before the
// generator the README, the book and the website each carried their own hand-written copy and
// disagreed (one still listed release 0.56.0 as upcoming at 0.62).
import fs from 'node:fs';
import path from 'node:path';
import { describe, expect, it } from 'vitest';
// @ts-expect-error plain ES module without declarations
import {
  apply,
  generate,
  model,
  releasedVersion,
  renderMarkdown,
} from '../scripts/roadmap-status.mjs';

const root = path.resolve(__dirname, '..');
const read = (p: string) => fs.readFileSync(path.join(root, p), 'utf8');

describe('roadmap status', () => {
  it('has every generated block up to date (run: node scripts/roadmap-status.mjs)', () => {
    for (const [file, next] of generate() as [string, string][])
      expect(read(file) === next, `${file} is stale`).toBe(true);
  });

  it('names the released version from the changelog', () => {
    const v = releasedVersion(read('CHANGELOG.md'));
    expect(v).toMatch(/^\d+\.\d+\.\d+$/);
    expect(read('README.md')).toContain(`Status as of release ${v}:`);
  });

  it('groups phases by state and drops empty groups', () => {
    const m = model(
      {
        phases: [
          { id: 'R1', title: 'a', state: 'done' },
          { id: 'R2', title: 'b', state: 'planned' },
        ],
      },
      '1.2.3',
    );
    expect(renderMarkdown(m)).toBe(
      'Status as of release 1.2.3:\n\n- **Done:** R1 a.\n- **Not started:** R2 b.',
    );
  });

  it('refuses a page without markers', () => {
    expect(() => apply('no markers', 'x')).toThrow(/markers/);
  });
});
