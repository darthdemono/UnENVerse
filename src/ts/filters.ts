// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
import type { VaultEntry, Project } from './types';
import { st } from './state';

export function getDescendantProjectIds(projectId: string): string[] {
  let projectName: string;
  if (projectId.startsWith('virtual:')) {
    projectName = projectId.slice(8);
  } else {
    const project = st.vault.projects.find((p) => p.id === projectId);
    if (!project) return [projectId];
    projectName = project.name;
  }
  const prefix = projectName + '/';
  return st.vault.projects
    .filter((p) => p.name === projectName || p.name.startsWith(prefix))
    .map((p) => p.id);
}

export type ProjectTreeNode = Project & { children: ProjectTreeNode[]; virtual: boolean };

export function buildProjectTree(projects: Project[]): ProjectTreeNode[] {
  const byName = new Map<string, ProjectTreeNode>();
  for (const p of projects) byName.set(p.name, { ...p, children: [], virtual: false });
  for (const p of projects) {
    const parts = p.name.split('/');
    for (let i = 1; i < parts.length; i++) {
      const anc = parts.slice(0, i).join('/');
      if (!byName.has(anc))
        byName.set(anc, {
          id: 'virtual:' + anc,
          name: anc,
          description: '',
          children: [],
          virtual: true,
        });
    }
  }
  const tree: ProjectTreeNode[] = [];
  for (const [name, node] of byName) {
    const parts = name.split('/');
    if (parts.length === 1) tree.push(node);
    else byName.get(parts.slice(0, -1).join('/'))!.children.push(node);
  }
  for (const [, node] of byName) node.children.sort((a, b) => a.name.localeCompare(b.name));
  tree.sort((a, b) => a.name.localeCompare(b.name));
  return tree;
}

/**
 * Search prefixes that mean "filter by this field". Everything else containing a
 * colon is ordinary text.
 *
 * Without the whitelist, any token with a colon became a filter: searching
 * `postgres://user` produced the filter `postgres="//user"`, which no branch in
 * `getFiltered` reads, and left the text query empty — so looking for a
 * connection string silently returned the *entire vault* instead of the one
 * entry, and looked like everything matched.
 */
const SEARCH_FILTER_KEYS = ['price', 'cat', 'env'];

export function parseSearch(q: string): { filters: Record<string, string>; text: string } {
  const filters: Record<string, string> = {};
  const words: string[] = [];
  q.split(/\s+/).forEach((t) => {
    const colon = t.indexOf(':');
    const k = colon > 0 ? t.slice(0, colon).toLowerCase() : '';
    const v = colon > 0 ? t.slice(colon + 1) : '';
    if (v && SEARCH_FILTER_KEYS.includes(k)) filters[k] = v.toLowerCase();
    else if (t) words.push(t.toLowerCase());
  });
  return { filters, text: words.join(' ') };
}

export function getFiltered(): VaultEntry[] {
  const { filters, text } = parseSearch(st.searchQ);
  // Fall back to the catch-all: an empty selection array would otherwise pass
  // undefined into getDescendantProjectIds and throw.
  const selectedProject = st.currentSelectedProjectIds[0] ?? 'Universal';
  const projectFilterIds =
    selectedProject === 'Universal' ? null : getDescendantProjectIds(selectedProject);
  return st.vault.api_keys.filter((k) => {
    if (projectFilterIds && !projectFilterIds.some((pid) => (k.projectIds || []).includes(pid)))
      return false;
    if (filters.price && k.price_type !== filters.price) return false;
    if (filters.cat && !(k.categories || []).some((c) => c.includes(filters.cat))) return false;
    if (filters.env && k.environment !== filters.env) return false;
    // Sidebar environment filter (separate from st.filter so it stacks with other filters)
    if (st.currentEnvFilter && k.environment !== st.currentEnvFilter) return false;
    // Tag sidebar filter
    if (st.activeTagFilter && !(k.tags ?? []).includes(st.activeTagFilter)) return false;
    // Env-prefix sidebar filter
    if (st.activePrefixFilter && !(k.env_prefixes ?? []).includes(st.activePrefixFilter))
      return false;
    // Key-pool sidebar filter. Compared against the trimmed name, matching
    // `poolsOf()` — a pool written as "openai " and one written as "openai" are
    // the same pool everywhere else, so they must be here too.
    if (
      st.activePoolFilter &&
      (typeof k.pool !== 'string' || k.pool.trim() !== st.activePoolFilter)
    )
      return false;
    if (text) {
      const hay = [
        k.provider,
        k.account_name,
        k.key_id,
        k.api_description,
        k.description,
        k.details,
        ...(k.categories || []),
        ...(k.tags ?? []),
      ]
        .join(' ')
        .toLowerCase();
      // Regex search (item 10): /pattern/flags syntax detected
      const reMatch = /^\/(.+)\/([gimsuy]*)$/.exec(text);
      if (reMatch) {
        try {
          const re = new RegExp(reMatch[1], reMatch[2] || 'i');
          if (!re.test(hay)) return false;
          // Malformed regex: fall back to a literal substring search on the
          // pattern itself. Matching against `text` instead would include the
          // wrapping slashes, which are never in the haystack, so the fallback
          // could never match anything.
        } catch {
          if (!hay.includes(reMatch[1])) return false;
        }
      } else {
        if (!hay.includes(text)) return false;
      }
    }
    // These two used to `return` their own verdict, which skipped the type chips below:
    // pick a category, then a type chip, and every card of the category stayed. They are
    // plain conditions now, so every filter dimension narrows the same result.
    if (st.filter.type === 'price' && k.price_type !== st.filter.value) return false;
    if (st.filter.type === 'category') {
      const pfx = st.filter.value + '/';
      if (!(k.categories || []).some((c) => c === st.filter.value || c.startsWith(pfx)))
        return false;
    }
    // Phase 24.2's type chip bar replaced the old single-select sidebar
    // sub-list (`st.filter.type === 'secret_type'`) — multi-toggle,
    // OR-combined within itself, ANDed with everything else here like every
    // other filter dimension. `__totp` is the one virtual chip: a seed is a
    // field on any entry type rather than a type of its own. There is
    // deliberately no `__pool` chip — key-pool filtering already lives in the
    // sidebar's Key Pools section (`activePoolFilter`, checked above), and a
    // second control for the same thing is exactly the redundancy this bar
    // exists to remove, not add.
    if (st.activeTypeChips.size) {
      const matches = [...st.activeTypeChips].some((chip) => {
        if (chip === '__totp') return !!k.totp_secret && k.totp_secret.trim() !== '';
        return (k.secretType || 'api_key') === chip;
      });
      if (!matches) return false;
    }
    return true;
  });
}

/** Compare two optional dates; a missing or unparsable one sorts after every real one. */
function dateCompare(
  a: string | null | undefined,
  b: string | null | undefined,
  dir: 1 | -1,
): number {
  const ta = a ? new Date(a).getTime() : NaN;
  const tb = b ? new Date(b).getTime() : NaN;
  if (Number.isNaN(ta) && Number.isNaN(tb)) return 0;
  if (Number.isNaN(ta)) return 1;
  if (Number.isNaN(tb)) return -1;
  return (ta - tb) * dir;
}

export function sorted(arr: VaultEntry[]): VaultEntry[] {
  const by = st.currentSortBy;
  const priceOrder: Record<string, number> = { free: 0, local: 1, conditional: 2, paid: 3 };
  return [...arr].sort((a, b) => {
    // Pinned entries always float to top
    if (a.pinned && !b.pinned) return -1;
    if (!a.pinned && b.pinned) return 1;
    // `??`, not `||`: `free` maps to 0, which `||` treats as missing and
    // replaces with 9 — sorting the free tier last instead of first.
    if (by === 'price')
      return (
        (priceOrder[a.price_type] ?? 9) - (priceOrder[b.price_type] ?? 9) ||
        a.provider.localeCompare(b.provider)
      );
    if (by === 'category')
      return (
        (a.categories?.[0] || 'zzz').localeCompare(b.categories?.[0] || 'zzz') ||
        a.provider.localeCompare(b.provider)
      );
    // Dates: an entry without the date is listed last in either direction, so reversing
    // the order never pushes the undated ones to the top.
    if (by === 'expiry' || by === 'expiry-asc' || by === 'expiry-desc') {
      const dir = by === 'expiry-desc' ? -1 : 1;
      return dateCompare(a.expires_at, b.expires_at, dir) || a.provider.localeCompare(b.provider);
    }
    if (by === 'added-desc' || by === 'added-asc') {
      const dir = by === 'added-asc' ? 1 : -1;
      return dateCompare(a.created_at, b.created_at, dir) || a.provider.localeCompare(b.provider);
    }
    return a.provider.localeCompare(b.provider);
  });
}
