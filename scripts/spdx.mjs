// SPDX-License-Identifier: PolyForm-Strict-1.0.0
// Copyright Jubair Hasan.
//
// Every source file carries a licence and copyright line (OpenSSF gold:
// license_per_file, copyright_per_file).
//   node scripts/spdx.mjs          list files that lack the header and exit 1
//   node scripts/spdx.mjs --write  add the header where it is missing
import { execFileSync } from 'node:child_process';
import { readFileSync, writeFileSync } from 'node:fs';

const SLASH = new Set(['rs', 'ts', 'js', 'mjs']);
const HASH = new Set(['py', 'sh']);
const SKIP = /^(tests\/fixtures|node_modules|dist|target|docs)\//;
const ID = 'SPDX-License-Identifier: PolyForm-Strict-1.0.0';
const HOLDER = 'Copyright Jubair Hasan.';

const files = execFileSync('git', ['ls-files', '-z'], { encoding: 'utf8' })
  .split('\0')
  .filter((f) => f && !SKIP.test(f) && /\.[a-z]+$/.test(f))
  .filter((f) => SLASH.has(f.split('.').pop()) || HASH.has(f.split('.').pop()));

const write = process.argv.includes('--write');
const missing = [];
for (const f of files) {
  const text = readFileSync(f, 'utf8');
  if (text.split('\n', 6).some((l) => l.includes('SPDX-License-Identifier'))) continue;
  missing.push(f);
  if (!write) continue;
  const c = HASH.has(f.split('.').pop()) ? '#' : '//';
  const head = `${c} ${ID}\n${c} ${HOLDER}\n`;
  // Keep a shebang (and a Python coding line) on top.
  const lines = text.split('\n');
  let at = 0;
  if (lines[0]?.startsWith('#!')) at = 1;
  if (at === 1 && /coding[:=]/.test(lines[1] ?? '')) at = 2;
  lines.splice(at, 0, ...head.trimEnd().split('\n'));
  writeFileSync(f, lines.join('\n'));
}
if (!write && missing.length) {
  console.error(`Missing SPDX header (run: node scripts/spdx.mjs --write):\n${missing.join('\n')}`);
  process.exit(1);
}
console.log(
  write
    ? `Added header to ${missing.length} files`
    : `All ${files.length} source files have a header`,
);
