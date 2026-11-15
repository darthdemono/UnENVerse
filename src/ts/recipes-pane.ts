// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
/**
 * Tools, Password recipes (R41): the full settings of the password generator in one place.
 *
 * A recipe is a template such as `{prefix}-{list:food}-{random:12:base62}` plus options. The parser,
 * the generator and the strength figures live in Rust (`vault_core::recipe`) and are reached over
 * IPC; this pane has no twin of them. It only edits what the vault stores (recipes, lists, facts and
 * rules) and shows what Rust answers. Samples stay masked until the eye box is ticked.
 */
import { html, setHtml } from './html';
import { persist, st } from './state';
import { invokeTauri, isTauri } from './tauri';
import { errorMessage, showConfirm, showToast } from './utils';

export interface SavedRecipe {
  name: string;
  template: string;
  options?: Record<string, unknown>;
}
interface RecipeList {
  name: string;
  words: string[];
  private?: boolean;
}
interface RecipeFact {
  name: string;
  value: string;
  private?: boolean;
}
interface RecipeRule {
  kind: string;
  value: string;
  recipe: string;
}
interface Doc {
  recipes?: SavedRecipe[];
  recipe_lists?: RecipeList[];
  recipe_vars?: RecipeFact[];
  recipe_rules?: RecipeRule[];
  recipe_default?: string;
  projects?: unknown[];
  api_keys?: Record<string, unknown>[];
}
interface Token {
  id: string;
  group: string;
  kind: string;
  summary: string;
  args: { name: string; default?: string }[];
}
interface Preview {
  samples: string[];
  bits: number;
  private_bits: number;
  length: number;
  parts: { token: string; kind: string; bits: number; private: boolean }[];
  dropped: string[];
  warnings: string[];
  rebuildable_from_memory: boolean;
}

/** Starting points. Choosing one copies it into a recipe you own and edit. */
export const PRESETS: { name: string; template: string }[] = [
  { name: 'Random only', template: '{random:20:safe}' },
  { name: 'Random + prefix', template: '{prefix}{divider}{random:16:base62}' },
  { name: 'Random + prefix + list', template: '{prefix}_{list:food}_{random:10:base36}' },
  {
    name: 'Provider, date and random',
    template: '{provider}_{created:D_M_YYYY}_{random:22:base36}_{digits:3}',
  },
  {
    name: 'Personal fact and a droppable tail',
    template: '{provider|lower}.{var:pet|title}.{random:12:base62}{suffix?1}',
  },
  {
    name: 'Short random lengthened per host',
    template: '{provider}_{random:8:base36|expand:22:base36:host}_{suffix}',
  },
  { name: 'Mask', template: '{mask:"CvcvCvcv"}-{number:100..999}-{symbols:1}' },
];

const RULE_KINDS = ['provider', 'family', 'host', 'type', 'project', 'tag'];
let registry: { tokens: Token[]; lists: { name: string; words: number }[] } | null = null;
let selected = '';

const el = <T extends HTMLElement = HTMLElement>(id: string) =>
  document.getElementById(id) as T | null;
const val = (id: string) => el<HTMLInputElement>(id)?.value ?? '';
const checked = (id: string) => el<HTMLInputElement>(id)?.checked ?? false;
const doc = () => st.vault as unknown as Doc;
const providerOf = (e: Record<string, unknown>) =>
  (typeof e.provider === 'string' ? e.provider : '').toLowerCase();

/** The part of the vault a recipe may read, sent to Rust for previews and rule tests. */
export function recipeView(): Doc {
  const d = doc();
  return {
    recipes: d.recipes ?? [],
    recipe_lists: d.recipe_lists ?? [],
    recipe_vars: d.recipe_vars ?? [],
    recipe_rules: d.recipe_rules ?? [],
    recipe_default: d.recipe_default,
    projects: d.projects ?? [],
  };
}

function say(msg: string, kind: 'ok' | 'err' = 'ok'): void {
  const s = el('rcp-status');
  if (!s) return;
  s.textContent = msg;
  s.className = `tool-status ${kind}`;
}

// ── Options form ↔ the options object ────────────────────────────────────────────────────────

const NUMBERS: [string, string][] = [
  ['rcp-min', 'min_len'],
  ['rcp-max', 'max_len'],
  ['rcp-req-upper', 'require_upper'],
  ['rcp-req-lower', 'require_lower'],
  ['rcp-req-digit', 'require_digit'],
  ['rcp-req-symbol', 'require_symbol'],
  ['rcp-maxrun', 'max_run'],
  ['rcp-chunk', 'chunk'],
  ['rcp-floor', 'floor_bits'],
];
const FLAGS: [string, string][] = [
  ['rcp-ambig', 'avoid_ambiguous'],
  ['rcp-noseq', 'no_sequences'],
  ['rcp-startletter', 'start_letter'],
  ['rcp-noedge', 'no_edge_symbol'],
];
const TEXTS: [string, string][] = [
  ['rcp-forbid', 'forbid'],
  ['rcp-allow', 'allow'],
  ['rcp-chunk-sep', 'chunk_sep'],
];

/** Reads the options form into the stored shape: only what differs from the defaults. */
export function readOptions(): Record<string, unknown> {
  const o: Record<string, unknown> = {};
  for (const [id, key] of NUMBERS) {
    const v = val(id).trim();
    if (v !== '' && Number.isFinite(Number(v))) o[key] = Number(v);
  }
  for (const [id, key] of FLAGS) if (checked(id)) o[key] = true;
  for (const [id, key] of TEXTS) if (val(id)) o[key] = val(id);
  if (val('rcp-policy') === 'refuse') o.policy = 'refuse';
  return o;
}

export function writeOptions(o: Record<string, unknown> = {}): void {
  for (const [id, key] of NUMBERS) {
    const input = el<HTMLInputElement>(id);
    const n = o[key];
    if (input) input.value = typeof n === 'number' && n !== 0 ? String(n) : '';
  }
  for (const [id, key] of FLAGS) {
    const input = el<HTMLInputElement>(id);
    if (input) input.checked = o[key] === true;
  }
  for (const [id, key] of TEXTS) {
    const input = el<HTMLInputElement>(id);
    if (input) input.value = typeof o[key] === 'string' ? (o[key] as string) : '';
  }
  const pol = el<HTMLSelectElement>('rcp-policy');
  if (pol) pol.value = o.policy === 'refuse' ? 'refuse' : 'fit';
}

// ── Painting ─────────────────────────────────────────────────────────────────────────────────

function paintRecipes(): void {
  const d = doc();
  const sel = el<HTMLSelectElement>('rcp-select');
  if (sel) {
    const saved = (d.recipes ?? []).map((r) => r.name);
    setHtml(
      sel,
      html`<option value="">(new recipe)</option>${saved.map(
        (n) =>
          html`<option value="${n}" ${n === selected ? 'selected' : ''}>
            ${n}${n === d.recipe_default ? ' (default)' : ''}
          </option>`,
      )}`,
    );
    sel.value = selected;
  }
  const names = (d.recipes ?? []).map((r) => r.name);
  for (const id of ['rcp-default', 'rcp-rule-recipe']) {
    const s = el<HTMLSelectElement>(id);
    if (!s) continue;
    const keep = s.value;
    const first = id === 'rcp-default' ? '(none)' : 'none (plain random)';
    setHtml(
      s,
      html`<option value="${id === 'rcp-default' ? '' : 'none'}">${first}</option>${names.map(
        (n) => html`<option value="${n}">${n}</option>`,
      )}`,
    );
    s.value = id === 'rcp-default' ? (d.recipe_default ?? '') : keep || (names[0] ?? 'none');
  }
}

function paintLists(): void {
  const box = el('rcp-lists');
  if (!box) return;
  const mine = doc().recipe_lists ?? [];
  const builtin = registry?.lists ?? [];
  setHtml(
    box,
    html`${builtin.map(
      (l) =>
        html`<div class="rcp-row">${l.name} <span class="rcp-dim">${l.words} words, built in</span></div>`,
    )}${mine.map(
      (l) =>
        html`<div class="rcp-row">
          ${l.name}
          <span class="rcp-dim"
            >${l.words.length} words${l.private ? ', private' : ''},
            ${Math.log2(Math.max(l.words.length, 1)).toFixed(1)} bits a pick${
              l.words.length < 256 ? ', a small list' : ''
            }</span
          >
          <button class="btn btn-ghost btn-sm" data-action="rcp-list-rm" data-name="${l.name}" aria-label="Remove list ${l.name}">Remove</button>
        </div>`,
    )}`,
  );
}

function paintFacts(): void {
  const box = el('rcp-facts');
  if (!box) return;
  setHtml(
    box,
    html`${(doc().recipe_vars ?? []).map(
      (f) =>
        html`<div class="rcp-row">
          ${f.name} <span class="rcp-dim">${f.private === false ? f.value : 'private'}</span>
          <button class="btn btn-ghost btn-sm" data-action="rcp-fact-rm" data-name="${f.name}" aria-label="Remove fact ${f.name}">Remove</button>
        </div>`,
    )}`,
  );
}

function paintRules(): void {
  const box = el('rcp-rules');
  if (!box) return;
  const rules = doc().recipe_rules ?? [];
  setHtml(
    box,
    rules.length
      ? html`${rules.map(
          (r, i) =>
            html`<div class="rcp-row">
              ${i + 1}. ${r.kind} <strong>${r.value}</strong> uses ${r.recipe}
              <button class="btn btn-ghost btn-sm" data-action="rcp-rule-rm" data-n="${i}" aria-label="Remove rule ${i + 1}">Remove</button>
            </div>`,
        )}`
      : html`<div class="rcp-dim">No rules. With none, a recipe applies only where an entry or the default picks it.</div>`,
  );
}

function paintPalette(): void {
  const sel = el<HTMLSelectElement>('rcp-token');
  if (!sel || !registry) return;
  const groups = [...new Set(registry.tokens.map((t) => t.group))];
  setHtml(
    sel,
    html`${groups.map(
      (g) =>
        html`<optgroup label="${g}">
          ${registry!.tokens
            .filter((t) => t.group === g)
            .map((t) => html`<option value="${t.id}">${t.id}: ${t.summary.slice(0, 70)}</option>`)}
        </optgroup>`,
    )}`,
  );
}

function paintPresets(): void {
  const sel = el<HTMLSelectElement>('rcp-preset');
  if (sel?.options.length === 0) {
    for (const p of PRESETS) sel.add(new Option(p.name, p.template));
  }
}

function paintPreview(p: Preview | null, error = ''): void {
  const box = el('rcp-preview');
  if (!box) return;
  if (error) {
    setHtml(box, html`<div class="rcp-error" role="alert">${error}</div>`);
    return;
  }
  if (!p) {
    setHtml(box, '');
    return;
  }
  const show = checked('rcp-reveal');
  setHtml(
    box,
    html`${p.samples.map(
      (s) =>
        html`<div class="mono rcp-sample">${show ? s : '•'.repeat(Math.min(s.length, 32))}</div>`,
    )}
    <div class="rcp-dim">
      ${p.length} characters. If the scheme is known: ${p.bits} bits (from the random parts only).
      ${p.private_bits ? `If your private lists and facts stay private: about ${p.private_bits} bits more are hidden.` : ''}
      Rebuildable from memory: ${p.rebuildable_from_memory ? 'yes' : 'no'}.
    </div>
    <table class="rcp-table">
      <caption class="rcp-dim">What each part adds</caption>
      <tr><th scope="col">Token</th><th scope="col">Kind</th><th scope="col">Bits</th><th scope="col">Public or private</th></tr>
      ${p.parts.map(
        (t) =>
          html`<tr><td>${t.token}</td><td>${t.kind}</td><td>${Math.round(t.bits)}</td><td>${t.private ? 'private' : 'public'}</td></tr>`,
      )}
    </table>
    ${p.dropped.length ? html`<div class="rcp-dim">Dropped to fit: ${p.dropped.join(', ')}</div>` : ''}
    ${p.warnings.map((w) => html`<div class="rcp-warn">${w}</div>`)}`,
  );
}

/** Repaints everything from the vault. */
export function renderRecipes(): void {
  paintRecipes();
  paintLists();
  paintFacts();
  paintRules();
}

// ── Actions ──────────────────────────────────────────────────────────────────────────────────

function entryForPreview(): Record<string, unknown> | null {
  const name = val('rcp-entry').trim().toLowerCase();
  if (!name) return null;
  return (doc().api_keys ?? []).find((e) => providerOf(e) === name) ?? null;
}

async function preview(count = 5): Promise<void> {
  if (!isTauri()) {
    paintPreview(null, 'Previews need the desktop app: the generator runs in Rust.');
    return;
  }
  try {
    const p = await invokeTauri<Preview>('recipe_preview', {
      template: val('rcp-template'),
      options: readOptions(),
      vault: recipeView(),
      entry: entryForPreview(),
      count,
    });
    paintPreview(p);
    say('');
  } catch (e) {
    paintPreview(null, errorMessage(e));
  }
}

async function save(): Promise<void> {
  const name = val('rcp-name').trim();
  if (!name) return say('Give the recipe a name.', 'err');
  const template = val('rcp-template');
  if (isTauri()) {
    try {
      // Saving checks the template parses; a weak result is a warning, not a reason to refuse.
      await invokeTauri('recipe_preview', {
        template,
        options: { ...readOptions(), floor_bits: 0 },
        vault: recipeView(),
        entry: entryForPreview(),
        count: 1,
      });
    } catch (e) {
      return say(errorMessage(e), 'err');
    }
  }
  const d = doc();
  d.recipes ??= [];
  const row: SavedRecipe = { name, template, options: readOptions() };
  const at = d.recipes.findIndex((r) => r.name === name);
  if (at >= 0) d.recipes[at] = row;
  else d.recipes.push(row);
  selected = name;
  await persist();
  renderRecipes();
  say(`Saved ${name}.`);
}

async function remove(): Promise<void> {
  const d = doc();
  if (!selected || !(d.recipes ?? []).some((r) => r.name === selected)) return;
  if (!(await showConfirm(`Delete the recipe ${selected} and the rules that use it?`))) return;
  d.recipes = (d.recipes ?? []).filter((r) => r.name !== selected);
  d.recipe_rules = (d.recipe_rules ?? []).filter((r) => r.recipe !== selected);
  if (d.recipe_default === selected) delete d.recipe_default;
  selected = '';
  await persist();
  load('');
  renderRecipes();
}

/** Puts a saved recipe (or a blank one) into the editor. */
export function load(name: string): void {
  selected = name;
  const r = (doc().recipes ?? []).find((x) => x.name === name);
  (el<HTMLInputElement>('rcp-name') as HTMLInputElement).value = r?.name ?? '';
  (el<HTMLTextAreaElement>('rcp-template') as HTMLTextAreaElement).value = r?.template ?? '';
  writeOptions(r?.options ?? {});
  paintPreview(null);
}

function insertToken(): void {
  const id = val('rcp-token');
  const t = registry?.tokens.find((x) => x.id === id);
  const box = el<HTMLTextAreaElement>('rcp-template');
  if (!t || !box) return;
  const args = t.args
    .map((a) => a.default ?? '')
    .join(':')
    .replace(/:+$/, '');
  const call = args ? `${id}:${args}` : id;
  const at = box.selectionStart ?? box.value.length;
  const end = box.selectionEnd ?? at;
  if (t.kind === 'transform' || t.kind === 'derive') {
    // A transform goes inside the braces of the token before the cursor: {random:8|upper}.
    if (box.value[at - 1] !== '}') {
      showToast('Put the cursor right after a token to add a transform to it', 'err');
      return;
    }
    box.value = `${box.value.slice(0, at - 1)}|${call}}${box.value.slice(end)}`;
  } else {
    box.value = `${box.value.slice(0, at)}{${call}}${box.value.slice(end)}`;
  }
  box.focus();
}

async function addList(): Promise<void> {
  const name = val('rcp-list-name').trim();
  const words = [
    ...new Set(
      val('rcp-list-words')
        .split(/[\n,]+/)
        .map((w) => w.trim())
        .filter(Boolean),
    ),
  ];
  if (!/^[A-Za-z0-9_-]{1,32}$/.test(name))
    return say('A list name is 1 to 32 letters, digits, - or _.', 'err');
  if (words.length < 2) return say('A list needs at least 2 different words.', 'err');
  const d = doc();
  d.recipe_lists ??= [];
  const row = { name, words, private: checked('rcp-list-private') };
  const at = d.recipe_lists.findIndex((l) => l.name === name);
  if (at >= 0) d.recipe_lists[at] = row;
  else d.recipe_lists.push(row);
  await persist();
  paintLists();
  say(`List ${name}: ${words.length} words, ${Math.log2(words.length).toFixed(1)} bits a pick.`);
}

async function addFact(): Promise<void> {
  const name = val('rcp-fact-name').trim();
  const value = val('rcp-fact-value');
  if (!/^[a-z0-9_]{1,32}$/.test(name))
    return say('A fact name is 1 to 32 lower-case letters, digits or _.', 'err');
  if (!value) return say('A fact needs a value.', 'err');
  const d = doc();
  d.recipe_vars ??= [];
  const row = { name, value, private: !checked('rcp-fact-public') };
  const at = d.recipe_vars.findIndex((f) => f.name === name);
  if (at >= 0) d.recipe_vars[at] = row;
  else d.recipe_vars.push(row);
  await persist();
  paintFacts();
  say(`Saved fact ${name}.`);
}

async function addRule(): Promise<void> {
  const value = val('rcp-rule-value').trim();
  if (!value) return say('A rule needs a value to match.', 'err');
  const d = doc();
  d.recipe_rules ??= [];
  d.recipe_rules.push({
    kind: val('rcp-rule-kind'),
    value,
    recipe: val('rcp-rule-recipe') || 'none',
  });
  await persist();
  paintRules();
}

async function testRule(): Promise<void> {
  const name = val('rcp-rule-test-q').trim().toLowerCase();
  const out = el('rcp-rule-test-out');
  if (!out) return;
  const entry = (doc().api_keys ?? []).find((e) => providerOf(e) === name);
  if (!entry) {
    out.textContent = 'No entry has that provider name.';
    return;
  }
  if (!isTauri()) {
    out.textContent = 'Needs the desktop app.';
    return;
  }
  const hit = await invokeTauri<{ recipe: string; why: string } | null>('recipe_resolve', {
    vault: recipeView(),
    entry,
  });
  out.textContent = hit ? `${hit.recipe} (${hit.why})` : 'No recipe applies: plain random.';
}

async function setDefault(name: string): Promise<void> {
  const d = doc();
  if (name) d.recipe_default = name;
  else delete d.recipe_default;
  await persist();
  paintRecipes();
}

/** The Remove buttons of the lists, facts and rules tables. */
async function rowAction(action: string, name: string, n: number): Promise<void> {
  const d = doc();
  if (action === 'rcp-list-rm')
    d.recipe_lists = (d.recipe_lists ?? []).filter((l) => l.name !== name);
  else if (action === 'rcp-fact-rm')
    d.recipe_vars = (d.recipe_vars ?? []).filter((f) => f.name !== name);
  else if (action === 'rcp-rule-rm') (d.recipe_rules ?? []).splice(n, 1);
  else return;
  await persist();
  renderRecipes();
  showToast('Removed', 'ok');
}

/** Wires the pane once; every later call only repaints. Handlers are assigned, not added. */
export function initRecipesPane(): void {
  if (!el('rcp-template')) return;
  const go = async () => {
    registry = null;
    if (isTauri()) {
      try {
        registry = await invokeTauri('recipe_registry');
      } catch {
        registry = null;
      }
    }
    paintPalette();
    paintPresets();
    renderRecipes();
    if (!el<HTMLInputElement>('rcp-name')?.value) load(selected);
  };
  void go();
  const rule = el<HTMLSelectElement>('rcp-rule-kind');
  if (rule?.options.length === 0) for (const k of RULE_KINDS) rule.add(new Option(k, k));
  el<HTMLSelectElement>('rcp-select')!.onchange = (e) => {
    load((e.target as HTMLSelectElement).value);
  };
  el('rcp-save')!.onclick = () => void save();
  el('rcp-delete')!.onclick = () => void remove();
  el('rcp-preview-btn')!.onclick = () => void preview();
  el('rcp-reveal')!.onchange = () => void preview();
  el('rcp-insert')!.onclick = insertToken;
  el('rcp-use-preset')!.onclick = () => {
    (el<HTMLTextAreaElement>('rcp-template') as HTMLTextAreaElement).value = val('rcp-preset');
  };
  el('rcp-list-add')!.onclick = () => void addList();
  el('rcp-fact-add')!.onclick = () => void addFact();
  el('rcp-rule-add')!.onclick = () => void addRule();
  el('rcp-rule-test')!.onclick = () => void testRule();
  el<HTMLSelectElement>('rcp-default')!.onchange = (e) => {
    void setDefault((e.target as HTMLSelectElement).value);
  };
  for (const id of ['rcp-lists', 'rcp-facts', 'rcp-rules']) {
    el(id)!.onclick = (e) => {
      const t = (e.target as HTMLElement).closest<HTMLElement>('[data-action]');
      if (t) void rowAction(t.dataset.action ?? '', t.dataset.name ?? '', Number(t.dataset.n));
    };
  }
}
