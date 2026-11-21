#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
// Build the Tauri updater feed (latest.json) from the signed desktop bundles.
//   node scripts/updater-manifest.mjs <assets-dir> <version> <base-url> <notes-file> > latest.json
// Every platform must have its bundle and a non-empty signature; a feed with a missing
// platform is never written, so a release cannot ship a half-filled table.
import { readFileSync, readdirSync, existsSync } from 'node:fs';
import { pathToFileURL } from 'node:url';

// Order matters: the arm64 pattern must be tried before the generic x64 one.
export const PLATFORMS = [
  ['linux-aarch64', /_aarch64\.AppImage$/],
  ['linux-x86_64', /_(amd64|x86_64)\.AppImage$/],
  ['windows-aarch64', /_arm64-setup\.exe$/],
  ['windows-x86_64', /_x64-setup\.exe$/],
];

export function buildManifest({ files, read, version, baseUrl, notes, now }) {
  const platforms = {};
  const missing = [];
  for (const [key, re] of PLATFORMS) {
    const bundle = files.find((f) => re.test(f) && files.includes(f + '.sig'));
    const signature = bundle ? read(bundle + '.sig').trim() : '';
    if (!bundle || !signature) missing.push(key);
    else platforms[key] = { signature, url: baseUrl + '/' + encodeURIComponent(bundle) };
  }
  if (missing.length) throw new Error('no signed bundle for: ' + missing.join(', '));
  return { version, notes, pub_date: now, platforms };
}

if (import.meta.url === pathToFileURL(process.argv[1] ?? '').href) {
  const [dir, version, baseUrl, notesFile] = process.argv.slice(2);
  if (!dir || !version || !baseUrl || !existsSync(notesFile ?? '')) {
    console.error('usage: updater-manifest.mjs <assets-dir> <version> <base-url> <notes-file>');
    process.exit(2);
  }
  const m = buildManifest({
    files: readdirSync(dir),
    read: (f) => readFileSync(dir + '/' + f, 'utf8'),
    version,
    baseUrl,
    notes: readFileSync(notesFile, 'utf8').trim(),
    now: new Date().toISOString(),
  });
  process.stdout.write(JSON.stringify(m, null, 2) + '\n');
}
