#!/usr/bin/env node
// SPDX-License-Identifier: PolyForm-Strict-1.0.0
// Copyright Jubair Hasan.
/**
 * Regenerates the data-model diagram on the architecture page from src/ts/types.ts with
 * TsUML2 (https://github.com/rimdev/tsuml2, a TypeScript to UML tool that runs offline).
 *
 *   node scripts/gen-uml.mjs
 *
 * It copies the entity interfaces (comments stripped) into a temporary file, because the
 * whole of types.ts also holds settings and view state that are not part of the vault, and
 * hands that file to `npx tsuml2`. Output: website/assets/architecture/data-model.svg.
 */
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const WANT = [
  'VaultData',
  'Project',
  'SecretChunk',
  'ChunkField',
  'VaultEntry',
  'EntryAuthority',
  'EntryProvenance',
  'EntryAttachment',
  'UserInfo',
  'UserClass',
  'PermissionEntry',
  'TokenInfo',
  'AuditRow',
];

const src = fs.readFileSync(path.join(root, 'src/ts/types.ts'), 'utf8');
const blocks = [];
for (const name of WANT) {
  const m = new RegExp(`export interface ${name}\\b[^{]*\\{`, 's').exec(src);
  if (!m) throw new Error(`interface ${name} not found in src/ts/types.ts`);
  let i = m.index + m[0].length;
  for (let depth = 1; depth; i++) depth += src[i] === '{' ? 1 : src[i] === '}' ? -1 : 0;
  blocks.push(
    src
      .slice(m.index, i)
      .replace(/\/\*\*[\s\S]*?\*\/\s*/g, '')
      .replace(/\/\/.*/g, ''),
  );
}
const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'unv-uml-'));
const model = path.join(tmp, 'model.ts');
fs.writeFileSync(model, blocks.join('\n') + '\n');
const out = path.join(root, 'website/assets/architecture/data-model.svg');
fs.mkdirSync(path.dirname(out), { recursive: true });
execFileSync(
  'npx',
  [
    '--yes',
    'tsuml2',
    '-g',
    model,
    '-o',
    out,
    '--typeLinks',
    'false',
    '--propertyTypes',
    'false',
    '--modifiers',
    'false',
    '--nomnoml',
    '#direction: down',
    '#spacing: 36',
    '#fontSize: 11',
  ],
  { stdio: 'inherit' },
);
fs.rmSync(tmp, { recursive: true, force: true });
console.log(`wrote ${path.relative(root, out)}`);
