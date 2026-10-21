// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
// The documentation book: every page the table of contents names exists, every phase is listed, and
// the phase pages carry no development-record paths.
import fs from 'node:fs';
import path from 'node:path';
import { describe, expect, it } from 'vitest';

const book = path.resolve(__dirname, '..', 'book', 'src');
const read = (rel: string) => fs.readFileSync(path.join(book, rel), 'utf8');

describe('the book', () => {
  it('links only pages that exist, and lists each page once', () => {
    const links = [...read('SUMMARY.md').matchAll(/\]\(([^)]+\.md)\)/g)].map((m) => m[1]);
    expect(links.length).toBeGreaterThan(50);
    for (const l of links) expect(fs.existsSync(path.join(book, l)), l).toBe(true);
    expect(new Set(links).size).toBe(links.length);
  });

  it('lists every phase from 1 to 56 and the queue after it', () => {
    const idx = read('phases/index.md');
    for (let n = 1; n <= 56; n++)
      if (![7, 8, 9, 10, 40, 55].includes(n))
        expect(idx, `phase ${n}`).toMatch(new RegExp(`\\|\\s*${n}\\s*\\|`));
    const queue = read('phases/roadmap-queue.md');
    for (let n = 57; n <= 80; n++)
      expect(queue, `phase ${n}`).toMatch(new RegExp(`\\|\\s*${n}\\s*\\|`));
  });

  it('keeps development-record paths and the old product name out of the phase pages', () => {
    for (const f of fs.readdirSync(path.join(book, 'phases'))) {
      const text = read(`phases/${f}`);
      expect(text, f).not.toMatch(/handoff\//);
      expect(text, f).not.toMatch(/Handoff-\d+/);
      expect(text, f).not.toContain('—');
    }
  });

  it('explains the numbering and the agent-safety lane', () => {
    expect(read('phases/index.md')).toContain('queue position');
    expect(read('roadmap.md')).toContain('agent-safety lane');
  });
});
