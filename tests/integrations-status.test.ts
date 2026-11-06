// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
// The page of exporter validators is generated from the CI workflow, so it cannot name a tool
// the CI does not run (R24).
import fs from 'node:fs';
import path from 'node:path';
import { describe, expect, it } from 'vitest';
// @ts-expect-error plain .mjs without types
import * as gen from '../scripts/integrations-status.mjs';

const { apply, generate, jobs, render } = gen;
const root = path.resolve(__dirname, '..');

describe('integrations page', () => {
  it('is up to date (run: node scripts/integrations-status.mjs)', () => {
    const page = fs.readFileSync(path.join(root, 'book/src/integrations.md'), 'utf8');
    expect(page === generate(), 'book/src/integrations.md is stale').toBe(true);
  });

  it('lists only jobs named "what we write → what checks it", with pinned images', () => {
    const wf = [
      'name: x',
      'jobs:',
      '  fixtures:',
      '    name: Regenerate golden fixtures',
      '  pg:',
      '    name: pg_connection → libpq',
      '    services:',
      '      db:',
      '        image: postgres:16',
      '  ssh:',
      '    name: ssh_config → OpenSSH',
    ].join('\n');
    expect(jobs(wf)).toEqual([
      { writes: 'pg_connection', checkedBy: 'libpq', images: ['postgres:16'] },
      { writes: 'ssh_config', checkedBy: 'OpenSSH', images: [] },
    ]);
    const out = render(jobs(wf));
    expect(out).toContain('| `pg_connection` | libpq | `postgres:16` |');
    expect(out).toContain('| `ssh_config` | OpenSSH | not pinned here |');
  });

  it('finds every real validator job in the workflow', () => {
    const real = fs.readFileSync(path.join(root, '.github/workflows/exporters.yml'), 'utf8');
    expect(jobs(real).length).toBeGreaterThanOrEqual(10);
  });

  it('refuses a page without markers', () => {
    expect(() => apply('no markers', 'x')).toThrow(/markers/);
  });
});
