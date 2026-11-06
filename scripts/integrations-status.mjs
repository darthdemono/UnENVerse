#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
/**
 * The page "Config we write, and the real tool that checks it" (R24), generated from the jobs in
 * .github/workflows/exporters.yml so it cannot claim a validator the CI does not run.
 *
 *   node scripts/integrations-status.mjs          # rewrite the block in book/src/integrations.md
 *   node scripts/integrations-status.mjs --check  # exit 1 when the block is stale
 *
 * A job counts when its `name:` has the form `<what we write> → <what checks it>`. A container
 * image pinned inside the job (`image: name:tag`) is listed; a tool with no pin has no version recorded here, and the page says so instead of inventing one.
 */
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const WORKFLOW = '.github/workflows/exporters.yml';
const PAGE = 'book/src/integrations.md';
const read = (p) => fs.readFileSync(path.join(ROOT, p), 'utf8');

/** `[{ writes, checkedBy, images }]` for every job named `a → b`, in file order. */
export function jobs(workflow) {
  const body = workflow.slice(workflow.indexOf('\njobs:'));
  const chunks = body.split(/^ {2}[A-Za-z0-9_-]+:[ \t]*$/m).slice(1);
  const out = [];
  for (const chunk of chunks) {
    const name = /^ {4}name:[ \t]*(.+?)[ \t]*$/m.exec(chunk);
    if (!name || !name[1].includes('→')) continue;
    const [writes, checkedBy] = name[1].split('→').map((s) => s.trim());
    const images = [...chunk.matchAll(/^\s*image:[ \t]*([\w./-]+:[\w.-]+)[ \t]*$/gm)].map(
      (m) => m[1],
    );
    out.push({ writes, checkedBy, images: [...new Set(images)] });
  }
  return out;
}

export function render(list) {
  const rows = list.map(
    (j) =>
      `| \`${j.writes}\` | ${j.checkedBy} | ${j.images.length ? j.images.map((i) => `\`${i}\``).join(', ') : 'not pinned here'} |`,
  );
  return ['| Config we write | Checked by | Pinned image |', '| --- | --- | --- |', ...rows].join(
    '\n',
  );
}

const BLOCK = /(<!-- integrations:start -->)[\s\S]*?(<!-- integrations:end -->)/;

export function apply(text, block) {
  if (!BLOCK.test(text)) throw new Error('missing integrations markers');
  return text.replace(BLOCK, (_, a, b) => `${a}\n\n${block}\n\n${b}`);
}

export function generate() {
  return apply(read(PAGE), render(jobs(read(WORKFLOW))));
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const next = generate();
  if (read(PAGE) !== next) {
    if (process.argv.includes('--check')) {
      console.error(`stale ${PAGE}\nrun: node scripts/integrations-status.mjs`);
      process.exit(1);
    }
    fs.writeFileSync(path.join(ROOT, PAGE), next);
  }
}
