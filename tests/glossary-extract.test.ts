// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
// The glossary extractor (R44, slice 1): it lists every term the product shows, with the text the
// product already says about it. These tests pin what it finds in the sources that need no build.
import { execFileSync } from 'node:child_process';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';

interface Term {
  id: string;
  term: string;
  category: string;
  aliases: string[];
  sources: { kind: string; file: string; text: string }[];
}

function run(): { counts: Record<string, number>; terms: Term[] } {
  const out = execFileSync(
    process.execPath,
    [join(process.cwd(), 'scripts/glossary-extract.mjs'), '--json'],
    // A binary that does not exist keeps the CLI part out of the run: the test needs no build.
    { encoding: 'utf8', env: { ...process.env, UNV_BIN: '/nonexistent/unv' }, maxBuffer: 1 << 26 },
  );
  return JSON.parse(out);
}

describe('glossary extractor', () => {
  const { counts, terms } = run();
  const byId = new Map(terms.map((t) => [t.id, t]));

  it('finds form fields, entry fields, settings, secret types and decisions', () => {
    for (const k of ['field', 'setting', 'secret type', 'decision']) {
      expect(counts[k], k).toBeGreaterThan(0);
    }
  });

  it('merges a form label and the entry field it edits into one term with both sources', () => {
    const username = byId.get('username');
    expect(username).toBeDefined();
    expect(username?.aliases).toContain('username');
    const kinds = username?.sources.map((s) => s.kind) ?? [];
    expect(kinds).toContain('form label');
    expect(kinds).toContain('VaultEntry comment');
  });

  it('lists every secret type by its registry id', () => {
    const ids = terms.filter((t) => t.sources.some((s) => s.kind === 'secret-types.json'));
    expect(ids.length).toBeGreaterThanOrEqual(26);
    expect(byId.get('recovery-codes')?.term).toBe('Recovery Codes');
  });

  it('has unique ids and a category for every term', () => {
    expect(new Set(terms.map((t) => t.id)).size).toBe(terms.length);
    for (const t of terms) expect(t.category, t.id).toBeTruthy();
  });
});
