// SPDX-License-Identifier: PolyForm-Strict-1.0.0
// Copyright Jubair Hasan.
/**
 * The TypeScript half of the starter-chunk parity pin: the app builds a new typed
 * project's chunks here, the CLI builds them in `unv-app/src/starters.rs`, and
 * `unv-cli/tests/contract_starters.rs` holds the CLI to the same golden file.
 * Regenerate deliberately with `PARITY_UPDATE=1 npx vitest run tests/starters-parity.test.ts`.
 */
import { describe, it, expect } from 'vitest';
import { readFileSync, writeFileSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import {
  makeWgStarterChunks,
  makeDockerStarterChunks,
  makeNginxStarterChunks,
  makeK8sStarterChunks,
  makeSshStarterChunks,
  makeTraefikStarterChunks,
  makeApacheStarterChunks,
  makeHaproxyStarterChunks,
  makeAnsibleStarterChunks,
  makePostgresStarterChunks,
} from '../src/ts/chunks/starters';

const FILE = join(dirname(fileURLToPath(import.meta.url)), 'fixtures', 'parity', 'starters.json');

const factories: Record<string, () => { id: string }[]> = {
  wireguard: makeWgStarterChunks,
  docker: makeDockerStarterChunks,
  nginx: makeNginxStarterChunks,
  kubernetes: makeK8sStarterChunks,
  ssh_config: makeSshStarterChunks,
  traefik: makeTraefikStarterChunks,
  apache: makeApacheStarterChunks,
  haproxy: makeHaproxyStarterChunks,
  ansible: makeAnsibleStarterChunks,
  postgres: makePostgresStarterChunks,
};

const bare = (chunks: { id: string }[]) => chunks.map(({ id: _id, ...rest }) => rest);

describe('starter chunks match the golden file the CLI is held to', () => {
  const golden: Record<string, unknown> = {};
  if (process.env.PARITY_UPDATE) {
    for (const [type, make] of Object.entries(factories)) golden[type] = bare(make());
    writeFileSync(FILE, JSON.stringify(golden, null, 2) + '\n');
  }
  const want = JSON.parse(readFileSync(FILE, 'utf8')) as Record<string, unknown>;

  for (const [type, make] of Object.entries(factories)) {
    it(`${type} starts with the recorded chunks and fresh unique ids`, () => {
      const chunks = make();
      expect(bare(chunks)).toEqual(want[type]);
      expect(new Set(chunks.map((c) => c.id)).size).toBe(chunks.length);
    });
  }
});
