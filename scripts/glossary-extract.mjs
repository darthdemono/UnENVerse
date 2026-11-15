#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
/**
 * Lists every term the product shows a person, with the text the product already says about it.
 *
 * This is slice 1 of the glossary (R44). It writes nothing the product reads: it reads the sources
 * (form labels, secret types, entry field comments, settings, the CLI's own contract, ADR titles) and
 * prints or writes a to-do list, so a definition can be drafted from what the product says and not from
 * what a word usually means, and so a test can fail when a term has no definition.
 *
 *   node scripts/glossary-extract.mjs                 # counts per source
 *   node scripts/glossary-extract.mjs --json          # the full list on stdout
 *   node scripts/glossary-extract.mjs --out FILE      # the full list into FILE
 *   UNV_BIN=target/debug/unv node scripts/glossary-extract.mjs   # which `unv` describes the CLI
 *
 * It does not guess: a term with no source text is listed with an empty `text` and counted, because
 * "nothing to draft from" is a finding.
 */
import { readFileSync, readdirSync, writeFileSync, existsSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const read = (p) => readFileSync(join(root, p), 'utf8');

const slug = (s) =>
  s
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+|-+$/g, '');
const clean = (s) =>
  s
    .replace(/<[^>]*>/g, ' ')
    .replace(/&nbsp;|&amp;/g, (m) => (m === '&amp;' ? '&' : ' '))
    .replace(/\s+/g, ' ')
    .trim();

/** @type {Map<string, {id:string, term:string, category:string, aliases:Set<string>, sources:{kind:string,file:string,text:string}[]}>} */
const terms = new Map();

function add(id, term, category, kind, file, text, alias) {
  if (!id) return;
  let t = terms.get(id);
  if (!t) {
    t = { id, term, category, aliases: new Set(), sources: [] };
    terms.set(id, t);
  }
  if (alias && alias !== term) t.aliases.add(alias);
  if (text || kind) t.sources.push({ kind, file, text: text || '' });
}

// 1. Form labels: `<label ... data-field="x" title="...">Label <span class="opt">...</span></label>`.
{
  const html = read('index.html');
  const re = /<label\b([^>]*?)>([\s\S]*?)<\/label>/g;
  for (const m of html.matchAll(re)) {
    const attrs = m[1];
    const field = /data-field="([^"]+)"/.exec(attrs)?.[1];
    if (!field) continue;
    const title = /title="([^"]*)"/.exec(attrs)?.[1] ?? '';
    const body = m[2].replace(/<span class="opt"[\s\S]*?<\/span>/g, '');
    const label = clean(body);
    add(slug(field), label || field, 'field', 'form label', 'index.html', clean(title), field);
  }
}

// 2. Secret types.
{
  const reg = JSON.parse(read('secret-types.json'));
  for (const t of reg.types) {
    add(slug(t.id), t.label, 'secret type', 'secret-types.json', 'secret-types.json', '', t.id);
  }
}

// 3. Entry fields and settings: interface members with the comment above them.
function interfaceMembers(file, name, category) {
  const lines = read(file).split('\n');
  const start = lines.findIndex((l) => l.startsWith(`export interface ${name}`));
  if (start < 0) return;
  let comment = [];
  let inComment = false;
  for (let i = start + 1; i < lines.length; i++) {
    const l = lines[i];
    if (l === '}') break;
    const t = l.trim();
    if (t.startsWith('/**')) {
      inComment = true;
      comment = [];
    }
    if (inComment) {
      comment.push(t.replace(/^\/\*\*\s?|\*\/$|^\*\s?/g, '').trim());
      if (t.endsWith('*/')) inComment = false;
      continue;
    }
    const m = /^([a-zA-Z_][a-zA-Z0-9_]*)\??:/.exec(t);
    if (m && l.startsWith('  ') && !l.startsWith('   ')) {
      const text = comment.join(' ').replace(/\s+/g, ' ').trim();
      add(slug(m[1]), m[1].replace(/_/g, ' '), category, `${name} comment`, file, text, m[1]);
    }
    if (!t.startsWith('*') && !t.startsWith('//')) comment = [];
  }
}
interfaceMembers('src/ts/types.ts', 'VaultEntry', 'field');
interfaceMembers('src/ts/types.ts', 'AppSettings', 'setting');

// 4. The CLI's own contract.
{
  const bin = process.env.UNV_BIN || 'unv';
  try {
    const out = execFileSync(bin, ['describe'], {
      encoding: 'utf8',
      env: { ...process.env, UNV_SERVER_URL: '', UNV_PASSWORD: '' },
      maxBuffer: 64 * 1024 * 1024,
    });
    const d = JSON.parse(out);
    const walk = (c) => {
      const path = Array.isArray(c.path) ? c.path.join(' ') : String(c.path ?? c.name ?? '');
      if (path && path !== 'unv') {
        add(
          slug(`unv ${path}`),
          `unv ${path}`,
          'command',
          'unv describe',
          'unv describe',
          c.about ?? c.description ?? '',
          null,
        );
      }
      for (const a of c.args ?? []) {
        const long = a.long ?? a.name;
        if (!long) continue;
        add(
          slug(`flag ${long}`),
          `--${long}`,
          'flag',
          'unv describe',
          'unv describe',
          a.help ?? a.description ?? '',
          null,
        );
      }
      for (const s of c.subcommands ?? []) walk(s);
    };
    walk(d.command);
    const exitCodes = d.exit_codes ?? [];
    for (const e of Array.isArray(exitCodes) ? exitCodes : Object.entries(exitCodes)) {
      const code = Array.isArray(e) ? e[0] : (e.code ?? e.name);
      const text = Array.isArray(e) ? String(e[1]) : (e.meaning ?? e.description ?? '');
      add(
        slug(`exit code ${code}`),
        `exit code ${code}`,
        'exit code',
        'unv describe',
        'unv describe',
        text,
        null,
      );
    }
  } catch (e) {
    process.stderr.write(
      `note: could not run \`${bin} describe\` (${e.message.split('\n')[0]}); commands skipped\n`,
    );
  }
}

// 5. ADR titles.
{
  const dir = join(root, 'book/src/adr');
  if (existsSync(dir)) {
    for (const f of readdirSync(dir).filter((n) => /^\d+.*\.md$/.test(n))) {
      const first = read(`book/src/adr/${f}`)
        .split('\n')
        .find((l) => l.startsWith('# '));
      if (first) {
        const title = first.replace(/^#\s*/, '').trim();
        add(
          slug(`adr ${f.slice(0, 4)}`),
          title,
          'decision',
          'ADR title',
          `book/src/adr/${f}`,
          title,
          null,
        );
      }
    }
  }
}

const list = [...terms.values()]
  .map((t) => ({ ...t, aliases: [...t.aliases].sort() }))
  .sort((a, b) => a.category.localeCompare(b.category) || a.id.localeCompare(b.id));

const byCategory = {};
let noText = 0;
for (const t of list) {
  byCategory[t.category] = (byCategory[t.category] ?? 0) + 1;
  if (!t.sources.some((s) => s.text)) noText++;
}

const args = process.argv.slice(2);
const outIdx = args.indexOf('--out');
if (outIdx >= 0) {
  writeFileSync(
    args[outIdx + 1],
    JSON.stringify({ counts: byCategory, terms: list }, null, 2) + '\n',
  );
}
if (args.includes('--json')) {
  process.stdout.write(JSON.stringify({ counts: byCategory, terms: list }, null, 2) + '\n');
} else {
  console.log(`${list.length} terms`);
  for (const [k, v] of Object.entries(byCategory)) console.log(`  ${k.padEnd(12)} ${v}`);
  console.log(`  without source text: ${noText}`);
}
