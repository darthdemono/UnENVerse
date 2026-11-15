// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
// The glossary (R44): one file that says what every term in the app means. These tests pin its shape
// and its coverage. Coverage is a ratchet: `glossary/pending.json` lists the terms still to define and
// only ever shrinks, so a new field, setting or secret type without an entry fails the build.
import { execFileSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';

interface GlossaryTerm {
  id: string;
  term: string;
  category: string;
  aliases?: string[];
  definition: string;
  general?: string;
  example?: string;
  see_also?: string[];
  where?: string[];
  since?: string;
  status?: string;
}

const read = (p: string) => readFileSync(join(process.cwd(), p), 'utf8');
const file = JSON.parse(read('glossary/terms.json')) as { version: number; terms: GlossaryTerm[] };
const pending = new Set(
  (JSON.parse(read('glossary/pending.json')) as { pending: string[] }).pending,
);
const terms = file.terms;
const ids = new Set(terms.map((t) => t.id));

// Categories the product always shows and that must have a definition. Commands and flags join once
// the glossary is built from `unv describe` in a job that has the binary.
const REQUIRED = new Set(['field', 'setting', 'secret type']);
const STATUSES = new Set(['current', 'deprecated', 'renamed', 'planned']);
const CATEGORIES = new Set([
  'field',
  'setting',
  'secret type',
  'concept',
  'command',
  'flag',
  'security',
  'exit code',
  'renamed',
]);

function extracted(): { id: string; category: string; aliases: string[] }[] {
  const out = execFileSync(
    process.execPath,
    [join(process.cwd(), 'scripts/glossary-extract.mjs'), '--json'],
    { encoding: 'utf8', env: { ...process.env, UNV_BIN: '/nonexistent/unv' }, maxBuffer: 1 << 26 },
  );
  return (JSON.parse(out) as { terms: { id: string; category: string; aliases: string[] }[] })
    .terms;
}

describe('glossary file', () => {
  it('has a version and a list of terms', () => {
    expect(file.version).toBe(1);
    expect(Array.isArray(terms)).toBe(true);
  });

  it('gives every term the required parts and a known category and status', () => {
    for (const t of terms) {
      expect(t.id, 'id').toMatch(/^[a-z0-9]+(-[a-z0-9]+)*$/);
      expect(t.term.trim(), `${t.id} term`).not.toBe('');
      expect(CATEGORIES.has(t.category), `${t.id} category ${t.category}`).toBe(true);
      expect(t.definition.trim(), `${t.id} definition`).not.toBe('');
      if (t.status) expect(STATUSES.has(t.status), `${t.id} status`).toBe(true);
    }
  });

  it('has unique ids and no alias that two terms share', () => {
    expect(ids.size).toBe(terms.length);
    const seen = new Map<string, string>();
    for (const t of terms) {
      for (const a of [t.term.toLowerCase(), ...(t.aliases ?? []).map((x) => x.toLowerCase())]) {
        const owner = seen.get(a);
        if (owner && owner !== t.id) throw new Error(`"${a}" names both ${owner} and ${t.id}`);
        seen.set(a, t.id);
      }
    }
  });

  it('keeps definitions short, plain and finished', () => {
    for (const t of terms) {
      expect(t.definition.length, `${t.id} definition length`).toBeLessThanOrEqual(600);
      for (const text of [t.definition, t.general ?? '', t.example ?? '']) {
        expect(text, `${t.id} has an em dash`).not.toContain('—');
        expect(text, `${t.id} has unfinished text`).not.toMatch(/\bTODO\b|\bTBD\b|lorem ipsum/i);
      }
    }
  });

  it('only links to terms that exist', () => {
    for (const t of terms) {
      for (const s of t.see_also ?? []) expect(ids.has(s), `${t.id} see_also ${s}`).toBe(true);
    }
  });
});

describe('glossary coverage', () => {
  const found = extracted().filter((t) => REQUIRED.has(t.category));

  it('defines every field, setting and secret type, or lists it as pending', () => {
    const defined = new Set<string>();
    for (const t of terms) {
      defined.add(t.id);
      for (const a of t.aliases ?? []) defined.add(a.toLowerCase().replace(/[^a-z0-9]+/g, '-'));
    }
    const missing = found.filter((t) => !defined.has(t.id) && !pending.has(t.id)).map((t) => t.id);
    expect(missing, 'terms with no glossary entry (add one to glossary/terms.json)').toEqual([]);
  });

  it('keeps pending honest: nothing pending is already defined or no longer exists', () => {
    const foundIds = new Set(found.map((t) => t.id));
    const stale = [...pending].filter((id) => ids.has(id) || !foundIds.has(id));
    expect(stale, 'remove these from glossary/pending.json').toEqual([]);
  });
});
