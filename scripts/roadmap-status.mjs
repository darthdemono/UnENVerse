#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
/**
 * One source for the roadmap phase states: roadmap-status.json.
 *
 *   node scripts/roadmap-status.mjs          # rewrite the generated blocks
 *   node scripts/roadmap-status.mjs --check  # exit 1 when a block is stale
 *
 * The README, the book's roadmap page and the website roadmap each carry a block between
 * `roadmap-status:start` and `roadmap-status:end` markers. The block names the released version
 * (the first dated heading in CHANGELOG.md) and lists the phases by state, so the three pages
 * cannot disagree with each other or fall behind the code.
 */
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const read = (p) => fs.readFileSync(path.join(ROOT, p), 'utf8');

export function releasedVersion(changelog) {
  const m = /^## (\d+\.\d+\.\d+) \(\d{4}-\d{2}-\d{2}\)/m.exec(changelog);
  if (!m) throw new Error('CHANGELOG.md has no dated release heading');
  return m[1];
}

const GROUPS = [
  ['done', 'Done'],
  ['partial', 'Partly shipped'],
  ['planned', 'Not started'],
];

/** The block as `{label, items}` rows plus the version, shared by the three renderers. */
export function model(status, version) {
  const rows = GROUPS.map(([state, label]) => ({
    label,
    items: status.phases.filter((p) => p.state === state).map((p) => `${p.id} ${p.title}`),
  })).filter((r) => r.items.length);
  return { version, rows };
}

const esc = (s) => s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');

export function renderMarkdown({ version, rows }) {
  const lines = rows.map((r) => `- **${r.label}:** ${r.items.join('; ')}.`);
  return `Status as of release ${version}:\n\n${lines.join('\n')}`;
}

export function renderHtml({ version, rows }) {
  const lis = rows
    .map((r) => `<li><strong>${r.label}:</strong> ${r.items.map(esc).join('; ')}.</li>`)
    .join('');
  return `<p>Status as of release ${version}:</p><ul>${lis}</ul>`;
}

const TARGETS = [
  ['README.md', renderMarkdown],
  ['book/src/roadmap.md', renderMarkdown],
  ['website/roadmap/index.html', renderHtml],
];

const START = /(<!-- roadmap-status:start -->)[\s\S]*?(<!-- roadmap-status:end -->)/;

export function apply(text, block) {
  if (!START.test(text)) throw new Error('missing roadmap-status markers');
  return text.replace(START, (_, a, b) => `${a}\n\n${block}\n\n${b}`);
}

export function generate() {
  const status = JSON.parse(read('roadmap-status.json'));
  const m = model(status, releasedVersion(read('CHANGELOG.md')));
  return TARGETS.map(([file, render]) => [file, apply(read(file), render(m))]);
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const stale = [];
  for (const [file, next] of generate()) {
    if (read(file) === next) continue;
    stale.push(file);
    if (!process.argv.includes('--check')) fs.writeFileSync(path.join(ROOT, file), next);
  }
  if (process.argv.includes('--check') && stale.length) {
    console.error(
      `stale roadmap status in: ${stale.join(', ')}\nrun: node scripts/roadmap-status.mjs`,
    );
    process.exit(1);
  }
}
