/**
 * Exporting and importing a structured project: each project type's Export
 * button must offer to copy and to download the real rendered file, under the
 * name its tool expects; importing a pasted Apache or HAProxy config must ask
 * before replacing chunks, can append instead, and must say when the text held
 * nothing usable.
 */
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { st } from '../src/ts/state';
import { render } from '../src/ts/render';
import { loadRealIndexHtml, makeProject, makeVault, resetState } from './helpers';

const toasts: [string, string?][] = [];
const copied: string[] = [];
const saved: [string, string][] = [];
let confirmAnswer = true;
let largeAnswer: string | null = '';
vi.mock('../src/ts/utils', async (importOriginal) => {
  const real = await importOriginal<typeof import('../src/ts/utils')>();
  return {
    ...real,
    showToast: (m: string, k?: string) => toasts.push([m, k]),
    showConfirm: async () => confirmAnswer,
    showPromptLarge: async () => largeAnswer,
    clipboardWrite: async (t: string) => void copied.push(t),
    saveFile: async (c: string, n: string) => {
      saved.push([c, n]);
      return { ok: true as const, path: `/dl/${n}` };
    },
  };
});

const flush = (ms = 20) => new Promise((r) => setTimeout(r, ms));
const chunk = (chunk_type: string, name: string, fields: [string, string][]) => ({
  id: crypto.randomUUID(),
  name,
  chunk_type,
  fields: fields.map(([key, value]) => ({ key, value, field_type: 'var' })),
});

function open(project: any) {
  st.vault = makeVault({
    api_keys: [],
    projects: [makeProject({ id: 'Universal', name: 'Universal' }), project],
  });
  st.currentSelectedProjectIds = [project.id];
  st.store = { save: async () => {}, isRemote: false } as never;
  render();
}
const press = async (action: string) => {
  document.querySelector<HTMLElement>(`#card-grid [data-action="${action}"]`)!.click();
  await flush();
};
const menu = () =>
  [...document.querySelectorAll<HTMLElement>('#dropdown .dropdown-item')].map((i) => i);
const choose = async (label: string) => {
  menu()
    .find((i) => i.textContent!.trim() === label)!
    .click();
  await flush(40);
};

beforeEach(() => {
  loadRealIndexHtml();
  resetState(st);
  toasts.length = 0;
  copied.length = 0;
  saved.length = 0;
  confirmAnswer = true;
  largeAnswer = '';
});

const EXPORTS: [string, string, string, string, string, [string, string, [string, string][]]][] = [
  [
    'kubernetes',
    'export-k8s',
    'Copy YAML',
    'Download manifests.yaml',
    'P-manifests.yaml',
    [
      'k8s_deployment',
      'Deployment',
      [
        ['name', 'web'],
        ['image', 'nginx:1'],
      ],
    ],
  ],
  [
    'ssh_config',
    'export-ssh',
    'Copy config',
    'Download config',
    'config',
    [
      'ssh_host',
      'box',
      [
        ['Host', 'box'],
        ['HostName', '10.0.0.5'],
      ],
    ],
  ],
  [
    'traefik',
    'export-traefik',
    'Copy traefik.yaml',
    'Download traefik.yaml',
    'traefik.yaml',
    [
      'traefik_router',
      'r1',
      [
        ['name', 'r1'],
        ['rule', 'Host(`a.test`)'],
      ],
    ],
  ],
  [
    'apache',
    'export-apache',
    'Copy config',
    'Download config',
    'P.conf',
    ['apache_vhost', 'v1', [['ServerName', 'a.test']]],
  ],
  [
    'haproxy',
    'export-haproxy',
    'Copy haproxy.cfg',
    'Download haproxy.cfg',
    'haproxy.cfg',
    [
      'haproxy_frontend',
      'fe',
      [
        ['name', 'fe'],
        ['bind', '*:80'],
      ],
    ],
  ],
  [
    'ansible',
    'export-ansible',
    'Copy YAML',
    'Download playbook.yml',
    'playbook.yml',
    [
      'ansible_task',
      'install',
      [
        ['name', 'install'],
        ['module', 'apt'],
      ],
    ],
  ],
  [
    'postgres',
    'export-postgres',
    'Copy .pgpass',
    'Download .pgpass',
    '.pgpass',
    [
      'pg_connection',
      'db-1',
      [
        ['host', 'db.test'],
        ['port', '5432'],
        ['dbname', 'app'],
        ['user', 'u'],
        ['password', 'pw-value'],
      ],
    ],
  ],
];

describe('exporting each project type', { timeout: 30000 }, () => {
  it.each(EXPORTS)(
    '%s: %s copies and downloads the rendered file',
    async (type, action, copyLabel, dlLabel, filename, [kind, name, fields]) => {
      open(
        makeProject({
          id: 'p',
          name: 'P',
          project_type: type,
          chunks: [chunk(kind, name, fields)],
        } as any),
      );
      await press(action);
      await choose(copyLabel);
      expect(copied).toHaveLength(1);
      expect(copied[0].length).toBeGreaterThan(5);
      expect(toasts).toContainEqual(['Copied ✓', 'ok']);
      await press(action);
      await choose(dlLabel);
      expect(saved).toHaveLength(1);
      expect(saved[0][1]).toBe(filename);
      expect(saved[0][0]).toBe(copied[0]);
      expect(toasts).toContainEqual(['Downloaded', 'ok']);
    },
  );
});

describe('importing a pasted Apache or HAProxy config', { timeout: 30000 }, () => {
  const APACHE =
    '<VirtualHost *:80>\n  ServerName a.test\n  DocumentRoot /var/www\n</VirtualHost>\n';
  const HAPROXY =
    'frontend fe\n  bind *:80\n  default_backend be\nbackend be\n  server s1 10.0.0.1:80\n';

  it.each([
    ['apache', 'import-apache', 'Paste config…', APACHE, 'Append to existing'],
    ['haproxy', 'import-haproxy', 'Paste config…', HAPROXY, 'Append to existing'],
  ])(
    '%s: pasted text becomes chunks, replacing only after asking',
    async (type, action, paste, text) => {
      const existing = chunk(type === 'apache' ? 'apache_vhost' : 'haproxy_frontend', 'old', [
        ['x', '1'],
      ]);
      open(makeProject({ id: 'p', name: 'P', project_type: type, chunks: [existing] } as any));
      largeAnswer = text;
      confirmAnswer = false;
      await press(action);
      await choose(paste);
      expect(st.vault.projects[1].chunks).toEqual([existing]);
      confirmAnswer = true;
      await press(action);
      await choose(paste);
      const after = st.vault.projects[1].chunks!;
      expect(after.some((c) => c.id === existing.id)).toBe(false);
      expect(after.length).toBeGreaterThan(0);
      expect(toasts.some(([m]) => /^Imported \d+ chunks/.test(m))).toBe(true);
    },
  );

  it.each([
    ['apache', 'import-apache', APACHE, 'No VirtualHost/Directory blocks found'],
    ['haproxy', 'import-haproxy', HAPROXY, 'No sections found'],
  ])(
    '%s: text with nothing usable says so and leaves the chunks alone',
    async (type, action, _t, message) => {
      const existing = chunk(type === 'apache' ? 'apache_vhost' : 'haproxy_frontend', 'old', [
        ['x', '1'],
      ]);
      open(makeProject({ id: 'p', name: 'P', project_type: type, chunks: [existing] } as any));
      largeAnswer = 'just some prose, no directives at all';
      await press(action);
      await choose('Paste config…');
      expect(toasts).toContainEqual([message, 'err']);
      expect(st.vault.projects[1].chunks).toEqual([existing]);
    },
  );

  it('cancelling the paste dialog imports nothing', async () => {
    open(makeProject({ id: 'p', name: 'P', project_type: 'apache', chunks: [] } as any));
    largeAnswer = null;
    await press('import-apache');
    await choose('Paste config…');
    expect(st.vault.projects[1].chunks).toEqual([]);
    expect(toasts).toEqual([]);
  });

  it('with chunks already there, the menu also offers to append instead of replace', async () => {
    open(
      makeProject({
        id: 'p',
        name: 'P',
        project_type: 'haproxy',
        chunks: [chunk('haproxy_frontend', 'old', [['x', '1']])],
      } as any),
    );
    await press('import-haproxy');
    expect(menu().map((i) => i.textContent!.trim())).toEqual([
      'Import from file',
      'Paste config…',
      'Append to existing',
    ]);
  });
});
