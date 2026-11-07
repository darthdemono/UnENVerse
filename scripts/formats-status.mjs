#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
/**
 * The list of sources `unv` can import from (R24), written from the command definitions so the page
 * cannot name a source the CLI does not accept.
 *
 *   node scripts/formats-status.mjs          # rewrite the block in book/src/formats-read.md
 *   node scripts/formats-status.mjs --check  # exit 1 when the block is stale
 *
 * It reads the `value_parser = [...]` list that follows `ImportVault {` and `ImportService {` in
 * unv-cli/src/main.rs.
 */
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const SOURCE = 'unv-cli/src/main.rs';
const PAGE = 'book/src/formats-read.md';
const read = (p) => fs.readFileSync(path.join(ROOT, p), 'utf8');

/** The quoted values of the first `value_parser = [...]` after `marker`, or `[]`. */
export function listAfter(text, marker) {
  const at = text.indexOf(marker);
  if (at < 0) return [];
  const m = /value_parser\s*=\s*\[([^\]]*)\]/.exec(text.slice(at));
  if (!m) return [];
  return [...m[1].matchAll(/"([^"]+)"/g)].map((x) => x[1]);
}

export function render(vendors, services) {
  const list = (xs) => xs.map((x) => `\`${x}\``).join(', ');
  return [
    `- \`unv import-vault <source> <file>\` reads: ${list(vendors)}.`,
    `- \`unv import-service <service>\` reads: ${list(services)}.`,
  ].join('\n');
}

const BLOCK = /(<!-- formats:start -->)[\s\S]*?(<!-- formats:end -->)/;

export function apply(text, block) {
  if (!BLOCK.test(text)) throw new Error('missing formats markers');
  return text.replace(BLOCK, (_, a, b) => `${a}\n\n${block}\n\n${b}`);
}

export function generate() {
  const src = read(SOURCE);
  const vendors = listAfter(src, 'ImportVault {');
  const services = listAfter(src, 'ImportService {');
  if (!vendors.length || !services.length)
    throw new Error('could not find the import source lists');
  return apply(read(PAGE), render(vendors, services));
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const next = generate();
  if (read(PAGE) !== next) {
    if (process.argv.includes('--check')) {
      console.error(`stale ${PAGE}\nrun: node scripts/formats-status.mjs`);
      process.exit(1);
    }
    fs.writeFileSync(path.join(ROOT, PAGE), next);
  }
}
