// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
/**
 * The glossary (R44): what every term in the app means, from the one file the website page and
 * `unv glossary` are built from (`glossary/terms.json`). Loaded on demand, so the main bundle does
 * not carry it.
 */
import file from '../../glossary/terms.json';

export interface GlossaryTerm {
  id: string;
  term: string;
  category: string;
  aliases?: string[];
  definition: string;
  general?: string;
  example?: string;
  see_also?: string[];
}

const all = (file as { terms: GlossaryTerm[] }).terms;

/** Letters and digits only, lower case: `api_key`, `API Key` and `api-key` are one name. */
const norm = (s: string): string => s.toLowerCase().replace(/[^a-z0-9]+/g, '');

const byName = new Map<string, GlossaryTerm>();
for (const t of all) {
  for (const n of [t.id, t.term, ...(t.aliases ?? [])]) {
    const k = norm(n);
    if (k && !byName.has(k)) byName.set(k, t);
  }
}

export function glossaryTerms(): GlossaryTerm[] {
  return all;
}

export function categories(): string[] {
  return [...new Set(all.map((t) => t.category))];
}

/** The term a name means: its id, its title, or a name it goes by. */
export function findTerm(name: string): GlossaryTerm | undefined {
  return byName.get(norm(name));
}

/** The definition for a form field name such as `api_secret`, if the glossary has one. */
export function definitionFor(field: string): string | undefined {
  return findTerm(field)?.definition;
}

/**
 * Terms matching a search: every word must appear in the title, a name or the definition. Title
 * hits come first, then name hits, then definition hits.
 */
export function searchTerms(query: string, category = '', limit = 100): GlossaryTerm[] {
  const words = query
    .toLowerCase()
    .split(/[^a-z0-9]+/)
    .filter(Boolean);
  const scored: { t: GlossaryTerm; score: number }[] = [];
  for (const t of all) {
    if (category && t.category !== category) continue;
    const title = t.term.toLowerCase();
    const names = [t.id, ...(t.aliases ?? [])].join(' ').toLowerCase();
    const def = t.definition.toLowerCase();
    let score = 0;
    let ok = true;
    for (const w of words) {
      if (title.includes(w)) score += 6;
      else if (names.includes(w)) score += 3;
      else if (def.includes(w)) score += 1;
      else {
        ok = false;
        break;
      }
    }
    if (ok) scored.push({ t, score });
  }
  scored.sort((a, b) => b.score - a.score || a.t.term.localeCompare(b.t.term));
  return scored.slice(0, limit).map((x) => x.t);
}

/** Puts the glossary's definition on every form label that names a field it knows. */
export function applyGlossaryTooltips(root: ParentNode = document): void {
  root.querySelectorAll<HTMLElement>('.form-label[data-field]').forEach((el) => {
    const d = definitionFor(el.dataset.field ?? '');
    if (d) el.title = d;
  });
}
