// SPDX-License-Identifier: PolyForm-Strict-1.0.0
// Copyright Jubair Hasan.
/**
 * Edge-case half of the exporter parity check. `cli-parity.test.ts` pins one
 * ordinary project per format; this feeds the same exporters the awkward inputs
 * (empty and unnamed fields, disabled and unknown chunks, defaults, duplicate list
 * items, unresolved references, quoting) from `tests/fixtures/parity/vault-edge.json`.
 *
 * The app writes `edge-exports.json`; the CLI's exporters are held to the same
 * bytes by `unv-app/tests/parity_edge.rs`. Regenerate deliberately with
 * `PARITY_UPDATE=1 npx vitest run tests/cli-parity-edge.test.ts`, then read the diff:
 * an intended change moves both sides together.
 */
import { describe, it, expect, beforeEach } from 'vitest';
import { readFileSync, writeFileSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { st } from '../src/ts/state';
import {
  exportWireGuard,
  exportDockerCompose,
  exportNginx,
  exportK8s,
  exportSshConfig,
  exportTraefik,
  exportApache,
  exportHaproxy,
  exportAnsible,
  exportPostgres,
  chunkToString,
} from '../src/ts/chunk-ops';
import { parseAllDocuments } from 'yaml';
import { loadRealIndexHtml, resetState } from './helpers';

const FIXTURES = join(dirname(fileURLToPath(import.meta.url)), 'fixtures', 'parity');
const vaultFixture = JSON.parse(readFileSync(join(FIXTURES, 'vault-edge.json'), 'utf8'));
const GOLDEN = join(FIXTURES, 'edge-exports.json');

const project = (id: string) => st.vault.projects.find((x: any) => x.id === id) as any;

function render(): Record<string, string> {
  const out: Record<string, string> = {};
  out['wireguard'] = exportWireGuard(project('vpn'));
  const compose = exportDockerCompose(project('stack'));
  out['compose.yaml'] = compose.yaml;
  out['compose.env'] = compose.envFile;
  out['nginx'] = exportNginx(project('edge'));
  out['k8s'] = exportK8s(project('k8s'));
  out['ssh'] = exportSshConfig(project('ssh'));
  out['traefik'] = exportTraefik(project('traefik'));
  out['traefik-empty'] = exportTraefik(project('traefik-none'));
  out['apache'] = exportApache(project('apache'));
  out['haproxy'] = exportHaproxy(project('haproxy'));
  out['ansible-tasks'] = exportAnsible(project('ansible-tasks'));
  out['ansible-vars'] = exportAnsible(project('ansible-vars'));
  out['ansible-none'] = exportAnsible(project('ansible-none'));
  out['pg'] = exportPostgres(project('pg'));
  for (const p of st.vault.projects as any[]) {
    for (const c of p.chunks ?? []) out[`chunk:${p.id}:${c.id}`] = chunkToString(c);
  }
  return out;
}

beforeEach(() => {
  loadRealIndexHtml();
  resetState(st);
  st.vault = JSON.parse(JSON.stringify(vaultFixture));
});

describe('exporter parity on edge-case projects', () => {
  it('matches the recorded output the CLI is held to', () => {
    const actual = render();
    if (process.env.PARITY_UPDATE) {
      writeFileSync(GOLDEN, JSON.stringify(actual, null, 2) + '\n');
    }
    expect(actual).toEqual(JSON.parse(readFileSync(GOLDEN, 'utf8')));
  });

  // Properties asserted directly, so a regenerated golden cannot bless a regression.
  it('never leaves a ${reference} to a vault entry in a generated file', () => {
    for (const [name, text] of Object.entries(render())) {
      if (name.startsWith('chunk:')) continue;
      expect(text, name).not.toContain('${Edge/');
    }
  });

  it('never exports a disabled chunk', () => {
    // A disabled chunk is greyed out in the app; a whole-project export leaves it out.
    const files = Object.entries(render())
      .filter(([name]) => !name.startsWith('chunk:'))
      .map(([, text]) => text)
      .join('\n');
    for (const needle of ['NEVER', 'image: never', 'must', 'gone.example.com', 'old.internal']) {
      expect(files).not.toContain(needle);
    }
  });
});

// Bug: a Deployment's `env:` block sat one level too shallow (8 spaces, beside the
// container's `- name:` instead of inside it), so any Deployment with an env_var
// field produced YAML no parser accepts. The ordinary fixture had no env fields.
describe('kubernetes output is valid YAML with env inside the container', () => {
  it('parses, and the container carries its env and envFrom', () => {
    const docs = parseAllDocuments(exportK8s(project('k8s')));
    expect(docs.every((d) => d.errors.length === 0)).toBe(true);
    const deployment = docs[0].toJS();
    const container = deployment.spec.template.spec.containers[0];
    expect(container.env).toEqual([
      { name: 'E1', value: 'v1' },
      { name: 'E2', value: 'has "quote"' },
    ]);
    expect(container.envFrom).toHaveLength(2);
  });
});

// Bug: Traefik middleware values were written unquoted, so a resolved secret such as
// `p@ss #1 "q"` loaded back as `p@ss` (YAML reads ` #` as a comment). Routers and
// services already quoted every value.
describe('traefik middleware values survive a YAML round trip', () => {
  it('keeps a value that contains # and quotes', () => {
    const parsed = parseAllDocuments(exportTraefik(project('traefik')))[0].toJS();
    expect(parsed.http.middlewares.auth.basicAuth.users).toBe('p@ss #1 "q"');
  });
});

// Bug: a compose reference that resolved to nothing was written into the .env as an
// empty string (the app) or as an escaped literal (the CLI), so the container started
// with a blank secret and nothing said why. It is now written through raw, which
// compose refuses loudly.
describe('an unresolvable compose reference is not turned into a blank secret', () => {
  it('writes the placeholder through to the .env', () => {
    const { envFile } = exportDockerCompose(project('stack'));
    expect(envFile).toContain('MISSING=${Nope/api_key}');
    expect(envFile).not.toMatch(/^MISSING=""?$/m);
  });
});
