// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
/**
 * The buttons on a config view and on its chunks: reordering within a kind,
 * duplicating, deleting, adding docker networks and volumes, every import path
 * (file or paste, replacing or appending) and what each says when the text has
 * nothing usable, the per-chunk .env export, and the nginx key-file import.
 */
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { st } from '../src/ts/state';
import { render } from '../src/ts/render';
import { loadRealIndexHtml, makeEntry, makeProject, makeVault, resetState } from './helpers';

const toasts: [string, string?][] = [];
const copied: string[] = [];
const saved: [string, string][] = [];
let confirmAnswer = true;
let promptAnswer: string | null = 'thing';
let largeAnswer: string | null = '';
let saveResult: { ok: boolean; path?: string; error?: string } = { ok: true, path: '/dl/x' };
vi.mock('../src/ts/utils', async (importOriginal) => {
  const real = await importOriginal<typeof import('../src/ts/utils')>();
  return {
    ...real,
    showToast: (m: string, k?: string) => toasts.push([m, k]),
    showConfirm: async () => confirmAnswer,
    showPrompt: async () => promptAnswer,
    showPromptLarge: async () => largeAnswer,
    clipboardWrite: async (t: string) => void copied.push(t),
    saveFile: async (c: string, n: string) => {
      saved.push([c, n]);
      return saveResult;
    },
  };
});

const flush = (ms = 30) => new Promise((r) => setTimeout(r, ms));
const ck = (chunk_type: string, name: string, fields: [string, string][] = [], id = name) => ({
  id,
  name,
  chunk_type,
  fields: fields.map(([key, value]) => ({ key, value, field_type: 'var' })),
});
const pj = (project_type: string, chunks: object[] | undefined, name = 'P') =>
  makeProject({ id: 'p', name, project_type, chunks } as any);

function open(project: any, entries: any[] = []) {
  st.vault = makeVault({
    api_keys: entries,
    projects: [makeProject({ id: 'Universal', name: 'Universal' }), project],
  });
  st.currentSelectedProjectIds = [project.id];
  st.store = { save: async () => {}, isRemote: false } as never;
  render();
}
const chunks = () => st.vault.projects[1].chunks!;
const names = () => chunks().map((c) => c.name);
const press = async (action: string, sel = '') => {
  document.querySelector<HTMLElement>(`#card-grid [data-action="${action}"]${sel}`)!.click();
  await flush();
};
const choose = async (label: string) => {
  [...document.querySelectorAll<HTMLElement>('#dropdown .dropdown-item')]
    .find((i) => i.textContent!.trim() === label)!
    .click();
  await flush(60);
};
const menu = () =>
  [...document.querySelectorAll('#dropdown .dropdown-item')].map((i) => i.textContent!.trim());
const synth = async (action: string, data: Record<string, string>) => {
  const b = document.createElement('button');
  b.dataset.action = action;
  for (const [k, v] of Object.entries(data)) b.dataset[k] = v;
  document.querySelector('.config-view')!.appendChild(b);
  b.click();
  await flush();
};
const pickFile = async (name: string, text: string) => {
  const inp = document.querySelector<HTMLInputElement>('input[type=file]')!;
  Object.defineProperty(inp, 'files', { value: [new File([text], name)], configurable: true });
  inp.dispatchEvent(new Event('change'));
  await flush(80);
};

beforeEach(() => {
  loadRealIndexHtml();
  resetState(st);
  toasts.length = 0;
  copied.length = 0;
  saved.length = 0;
  confirmAnswer = true;
  promptAnswer = 'thing';
  largeAnswer = '';
  saveResult = { ok: true, path: '/dl/x' };
});

describe('reordering, duplicating and deleting chunks', { timeout: 30000 }, () => {
  const three = () =>
    pj('wireguard', [
      ck('wg_interface', 'wg0'),
      ck('wg_peer', 'a'),
      ck('wg_peer', 'b'),
      ck('wg_peer', 'c'),
    ]);

  it('moves a chunk past its neighbours of the same kind only', async () => {
    open(three());
    await synth('chunk-down', { projectId: 'p', chunkId: 'a' });
    expect(names()).toEqual(['wg0', 'b', 'a', 'c']);
    await synth('chunk-up', { projectId: 'p', chunkId: 'c' });
    expect(names()).toEqual(['wg0', 'b', 'c', 'a']);
  });

  it('says so at the ends instead of moving across kinds', async () => {
    open(three());
    await synth('chunk-up', { projectId: 'p', chunkId: 'a' });
    expect(toasts[toasts.length - 1][0]).toBe('Already first of its kind');
    await synth('chunk-down', { projectId: 'p', chunkId: 'c' });
    expect(toasts[toasts.length - 1][0]).toBe('Already last of its kind');
    expect(names()).toEqual(['wg0', 'a', 'b', 'c']);
  });

  it('a move for a chunk that is gone changes nothing', async () => {
    open(three());
    await synth('chunk-up', { projectId: 'p', chunkId: 'ghost' });
    expect(names()).toEqual(['wg0', 'a', 'b', 'c']);
  });

  it('duplicating puts a renamed copy right after the original, with its own id and fields', async () => {
    open(three());
    await synth('dup-chunk', { projectId: 'p', chunkId: 'a' });
    expect(names()).toEqual(['wg0', 'a', 'a (copy)', 'b', 'c']);
    expect(new Set(chunks().map((c) => c.id)).size).toBe(5);
    expect(toasts).toContainEqual(['Duplicated "a"', 'ok']);
    await synth('dup-chunk', { projectId: 'p', chunkId: 'ghost' });
    expect(chunks()).toHaveLength(5);
  });

  it('deleting asks first, and a no leaves the chunk', async () => {
    open(three());
    confirmAnswer = false;
    await synth('delete-chunk', { projectId: 'p', chunkId: 'a' });
    expect(names()).toContain('a');
    confirmAnswer = true;
    await synth('delete-chunk', { projectId: 'p', chunkId: 'a' });
    expect(names()).not.toContain('a');
  });

  it('actions naming a chunk or project that is gone do nothing', async () => {
    open(three());
    const before = JSON.stringify(st.vault);
    for (const action of [
      'edit-chunk',
      'copy-chunk-full',
      'link-env-chunk',
      'export-env-chunk',
      'import-nginx-key-file',
    ])
      await synth(action, { projectId: 'p', chunkId: 'ghost' });
    for (const action of ['edit-chunk', 'add-wg-peer', 'export-wg'])
      await synth(action, { projectId: 'ghost', chunkId: 'a' });
    expect(JSON.stringify(st.vault)).toBe(before);
    expect(copied).toEqual([]);
  });

  it('copying a chunk puts all of it on the clipboard', async () => {
    open(three());
    await synth('copy-chunk-full', { projectId: 'p', chunkId: 'a' });
    expect(copied).toHaveLength(1);
    expect(toasts).toContainEqual(['Copied ✓', 'ok']);
  });

  it('a project with no chunk list yet gets one when something is added', async () => {
    open(pj('wireguard', undefined));
    await press('add-wg-peer');
    expect(names()).toEqual(['Peer 1']);
    await press('add-wg-peer');
    expect(names()).toEqual(['Peer 1', 'Peer 2']);
  });
});

describe('docker pieces', { timeout: 30000 }, () => {
  it('adds numbered services', async () => {
    open(pj('docker', []));
    await press('add-docker-service');
    await press('add-docker-service');
    expect(names()).toEqual(['service-1', 'service-2']);
  });

  it('adds networks and volumes by name: the first creates the chunk, later ones join it', async () => {
    open(pj('docker', undefined));
    promptAnswer = 'backend';
    await press('add-docker-network');
    promptAnswer = 'frontend';
    await press('add-docker-network');
    promptAnswer = 'data';
    await press('add-docker-volume');
    promptAnswer = 'logs';
    await press('add-docker-volume');
    expect(names()).toEqual(['networks', 'volumes']);
    expect(chunks()[0].fields.map((f) => f.key)).toEqual(['backend', 'frontend']);
    expect(chunks()[1].fields.map((f) => f.key)).toEqual(['data', 'logs']);
  });

  it('cancelling the name adds nothing', async () => {
    open(pj('docker', []));
    promptAnswer = null;
    await press('add-docker-network');
    await press('add-docker-volume');
    expect(chunks()).toEqual([]);
  });

  it('copying the services section says when there is nothing to copy', async () => {
    open(pj('docker', []));
    await press('export-docker-services');
    expect(toasts).toContainEqual(['No services to copy', '']);
  });

  it('copies the compose file and the .env, or downloads either, reporting each outcome', async () => {
    open(
      pj('docker', [
        ck('docker_service', 'web', [['image', 'nginx:1']]),
        ck('env_file', 'app', [['A', '1']]),
      ]),
    );
    await press('export-docker');
    expect(menu()).toEqual(['Copy YAML', 'Copy .env', 'Download YAML', 'Download .env']);
    await choose('Copy YAML');
    await press('export-docker');
    await choose('Copy .env');
    expect(copied).toHaveLength(2);
    expect(toasts).toContainEqual(['Copied YAML ✓', 'ok']);
    expect(toasts).toContainEqual(['Copied .env ✓', 'ok']);
    await press('export-docker');
    await choose('Download YAML');
    await press('export-docker');
    await choose('Download .env');
    expect(saved.map((s) => s[1])).toEqual(['docker-compose.yml', 'P.env']);
    expect(toasts).toContainEqual(['Downloaded to /dl/x', 'ok']);
    saveResult = { ok: true };
    await press('export-docker');
    await choose('Download YAML');
    expect(toasts).toContainEqual(['Downloaded', 'ok']);
    saveResult = { ok: false, error: 'disk full' };
    await press('export-docker');
    await choose('Download YAML');
    expect(toasts).toContainEqual(['Failed: disk full', 'error']);
  });

  it('copies the services section when there are services', async () => {
    open(pj('docker', [ck('docker_service', 'web', [['image', 'nginx:1']])]));
    await press('export-docker-services');
    expect(copied[0]).toContain('nginx:1');
    expect(toasts).toContainEqual(['Services YAML copied ✓', 'ok']);
  });
});

describe('wireguard export and import', { timeout: 30000 }, () => {
  const WG =
    '[Interface]\nPrivateKey = aaaaaaaa\nAddress = 10.0.0.1/24\n\n[Peer]\nPublicKey = bbbbbbbb\nAllowedIPs = 10.0.0.2/32\n';

  it('exports wg0.conf to the clipboard or to a file named after the project', async () => {
    open(pj('wireguard', [ck('wg_interface', 'wg0', [['Address', '10.0.0.1/24']])]));
    await press('export-wg');
    await choose('Copy wg0.conf');
    expect(copied[0]).toContain('[Interface]');
    await press('export-wg');
    await choose('Download wg0.conf');
    expect(saved[0][1]).toBe('P.conf');
    saveResult = { ok: false, error: 'nope' };
    await press('export-wg');
    await choose('Download wg0.conf');
    expect(toasts).toContainEqual(['Failed: nope', 'error']);
  });

  it('a pasted config replaces the chunks only after asking', async () => {
    open(pj('wireguard', [ck('wg_peer', 'old')]));
    largeAnswer = WG;
    confirmAnswer = false;
    await press('import-wg');
    await choose('Paste wg0.conf text…');
    expect(names()).toEqual(['old']);
    confirmAnswer = true;
    await press('import-wg');
    await choose('Paste wg0.conf text…');
    expect(names()).not.toContain('old');
    expect(toasts.some(([m]) => /^Imported \d+ sections/.test(m))).toBe(true);
  });

  it('an empty project imports without asking, and text with no sections says so', async () => {
    open(pj('wireguard', []));
    largeAnswer = WG;
    await press('import-wg');
    await choose('Paste wg0.conf text…');
    expect(chunks().length).toBeGreaterThan(0);
    largeAnswer = 'nothing here';
    await press('import-wg');
    await choose('Paste wg0.conf text…');
    expect(toasts).toContainEqual(['No WireGuard sections found', 'err']);
  });

  it('importing from a file reads it', async () => {
    open(pj('wireguard', []));
    await press('import-wg');
    await choose('Import from file');
    await pickFile('wg0.conf', WG);
    expect(chunks().length).toBeGreaterThan(0);
  });

  it('cancelling the paste dialog imports nothing', async () => {
    open(pj('wireguard', []));
    largeAnswer = null;
    await press('import-wg');
    await choose('Paste wg0.conf text…');
    expect(chunks()).toEqual([]);
  });
});

describe('docker, ssh and nginx imports', { timeout: 30000 }, () => {
  const COMPOSE = 'services:\n  web:\n    image: nginx:1\n';
  const SSH = 'Host box\n  HostName 10.0.0.5\n  User me\n';
  const NGINX =
    'upstream app { server 127.0.0.1:3000; }\nserver {\n  listen 443 ssl;\n  server_name site.example.test;\n  ssl_certificate /etc/letsencrypt/live/site.example.test/fullchain.pem;\n  location / { proxy_pass http://app; }\n}\n';

  it('docker: pasted compose replaces after asking; no services says so; file import works', async () => {
    open(pj('docker', [ck('docker_service', 'old')]));
    largeAnswer = COMPOSE;
    confirmAnswer = false;
    await press('import-docker');
    await choose('Paste YAML…');
    expect(names()).toEqual(['old']);
    confirmAnswer = true;
    await press('import-docker');
    await choose('Paste YAML…');
    expect(names()).not.toContain('old');
    largeAnswer = 'not: compose';
    await press('import-docker');
    await choose('Paste YAML…');
    expect(toasts).toContainEqual(['No services/networks/volumes found', 'err']);
    largeAnswer = null;
    await press('import-docker');
    await choose('Paste YAML…');
    await press('import-docker');
    await choose('Import from file');
    await pickFile('docker-compose.yml', COMPOSE);
    expect(toasts.some(([m]) => /^Imported \d+ chunks/.test(m))).toBe(true);
  });

  it('ssh: a file with hosts replaces after asking; one without says so', async () => {
    open(pj('ssh_config', [ck('ssh_host', 'old')]));
    await press('import-ssh');
    await pickFile('config', 'nothing useful');
    expect(toasts).toContainEqual(['No Host blocks found in file', 'err']);
    confirmAnswer = false;
    await press('import-ssh');
    await pickFile('config', SSH);
    expect(names()).toEqual(['old']);
    confirmAnswer = true;
    await press('import-ssh');
    await pickFile('config', SSH);
    expect(names()).toEqual(['box']);
    expect(toasts).toContainEqual(['Imported 1 host blocks ✓', 'ok']);
    open(pj('ssh_config', []));
    await press('import-ssh');
    await pickFile('config', SSH);
    expect(names()).toEqual(['box']);
  });

  it('nginx: replace after asking, append to what is there, and name the certificate domains found', async () => {
    open(pj('nginx', [ck('nginx_server', 'old')]));
    largeAnswer = NGINX;
    confirmAnswer = false;
    await press('import-nginx');
    expect(menu()).toEqual(['Import from file', 'Paste nginx config…', 'Append to existing']);
    await choose('Paste nginx config…');
    expect(names()).toEqual(['old']);
    confirmAnswer = true;
    await press('import-nginx');
    await choose('Paste nginx config…');
    expect(names()).not.toContain('old');
    expect(toasts[toasts.length - 1][0]).toContain('SSL cert domains: site.example.test');
    const before = chunks().length;
    await press('import-nginx');
    await choose('Append to existing');
    await pickFile('site.conf', NGINX);
    expect(chunks().length).toBe(before * 2);
  });

  it('nginx: text without blocks says so, an empty project has no append, and cancelling imports nothing', async () => {
    open(pj('nginx', undefined));
    await press('import-nginx');
    expect(menu()).toEqual(['Import from file', 'Paste nginx config…']);
    largeAnswer = 'plain words';
    await choose('Paste nginx config…');
    expect(toasts).toContainEqual(['No server/upstream blocks found', 'err']);
    largeAnswer = null;
    await press('import-nginx');
    await choose('Paste nginx config…');
    largeAnswer = 'upstream a { server 127.0.0.1:1; }';
    await press('import-nginx');
    await choose('Paste nginx config…');
    expect(toasts[toasts.length - 1][0]).toMatch(/^Imported 1 chunks ✓$/);
  });
});

describe('env chunks', { timeout: 30000 }, () => {
  const env = () =>
    pj('apache', [
      ck(
        'env_file',
        'prod',
        [
          ['DB_PASSWORD', 'pw'],
          ['NAME', 'x y'],
        ],
        'e1',
      ),
    ]);

  it('exports a chunk as a .env with values quoted, copying or downloading', async () => {
    open(env());
    await press('export-env-chunk');
    await choose('Copy .env');
    expect(copied[0]).toBe('DB_PASSWORD=pw\nNAME="x y"');
    await press('export-env-chunk');
    await choose('Download .env');
    expect(saved[0]).toEqual(['DB_PASSWORD=pw\nNAME="x y"', 'prod.env']);
    saveResult = { ok: true };
    await press('export-env-chunk');
    await choose('Download .env');
    expect(toasts).toContainEqual(['Downloaded', 'ok']);
    saveResult = { ok: false, error: 'ro' };
    await press('export-env-chunk');
    await choose('Download .env');
    expect(toasts).toContainEqual(['Failed: ro', 'error']);
  });

  it('keeps an unresolved reference as written when exporting', async () => {
    open(pj('apache', [ck('env_file', 'prod', [['X', '${Nobody/key}']], 'e1')]));
    await press('export-env-chunk');
    await choose('Copy .env');
    expect(copied[0]).toBe('X=${Nobody/key}');
  });

  it('link-to-vault opens the dialog for that chunk', async () => {
    open(env());
    await press('link-env-chunk');
    expect(document.getElementById('env-link-overlay')!.classList.contains('open')).toBe(true);
  });

  it('importing pasted .env text makes a chunk, marking likely secrets and references', async () => {
    open(pj('apache', []));
    largeAnswer = 'API_TOKEN=abc123\nHOST=db.test\nREF=${Other/key}\nEMPTY_KEY=\n';
    await press('import-env-chunk');
    await choose('Paste .env text…');
    const c = chunks()[0];
    expect(c.name).toBe('env-file');
    const byKey = Object.fromEntries(c.fields.map((f) => [f.key, f]));
    expect(byKey.API_TOKEN.field_type).toBe('secret');
    expect(byKey.HOST.field_type).toBe('var');
    expect(byKey.REF.field_type).toBe('env_var');
    expect(byKey.EMPTY_KEY.field_type).toBe('var');
    expect(toasts).toContainEqual(['Imported 4 vars into "env-file" ✓', 'ok']);
  });

  it('importing a file named like an existing chunk refreshes that chunk instead of adding another', async () => {
    open(env());
    await press('import-env-chunk');
    await choose('Import from file');
    await pickFile('prod.env', 'NEW=1\n');
    expect(chunks()).toHaveLength(1);
    expect(chunks()[0].fields.map((f) => f.key)).toEqual(['NEW']);
  });

  it('a file with no variables says so; a project without a chunk list gets one', async () => {
    open(pj('apache', undefined));
    await press('import-env-chunk');
    await choose('Import from file');
    await pickFile('empty.env', '# nothing\n');
    expect(toasts).toContainEqual(['No variables found in .env', 'err']);
    await press('import-env-chunk');
    await choose('Import from file');
    await pickFile('.env', 'A=1\n');
    expect(names()).toEqual(['env-file']);
    largeAnswer = null;
    await press('import-env-chunk');
    await choose('Paste .env text…');
    expect(chunks()).toHaveLength(1);
  });
});

describe('nginx key files', { timeout: 30000 }, () => {
  const server = ck('nginx_server', 'site', [['server_name', 'only.example.test']]);

  it('imports a pem into the one domain’s certificate entry when there is exactly one domain', async () => {
    open(pj('nginx', [server], 'internal'));
    await press('add-nginx-key');
    expect(menu()).toEqual(['Import fullchain.pem', 'Import privkey.pem', 'Blank key file']);
    await choose('Import fullchain.pem');
    await pickFile(
      'fullchain.pem',
      '-----BEGIN CERTIFICATE-----\nCHAIN\n-----END CERTIFICATE-----\n',
    );
    const cert = st.vault.api_keys.find((e) => e.secretType === 'certificate')!;
    expect(cert.provider).toBe('only.example.test');
    expect(cert.certificate_data).toContain('CHAIN');
    expect(
      toasts.some(([m]) => m.startsWith('Imported fullchain.pem into only.example.test')),
    ).toBe(true);
    await press('add-nginx-key');
    await choose('Import privkey.pem');
    await pickFile('privkey.pem', '-----BEGIN PRIVATE KEY-----\nKEY\n-----END PRIVATE KEY-----\n');
    expect(cert.cert_key_data).toContain('KEY');
    expect(chunks().some((c) => c.chunk_type === 'nginx_key')).toBe(false);
  });

  it('with no single domain it keeps the pem as a key-file chunk, and a blank one can be added', async () => {
    open(pj('nginx', [], 'internal'));
    await press('add-nginx-key');
    await choose('Import privkey.pem');
    await pickFile('my.key.pem', '-----BEGIN PRIVATE KEY-----\nKEY\n-----END PRIVATE KEY-----\n');
    expect(chunks()).toHaveLength(1);
    expect(chunks()[0].chunk_type).toBe('nginx_key');
    expect(chunks()[0].name).toBe('my.key');
    await press('add-nginx-key');
    await choose('Blank key file');
    expect(chunks()).toHaveLength(2);
    expect(chunks()[1].name).toBe('key-2');
  });

  it('re-importing a key file onto a chunk guesses privkey or fullchain from the file name', async () => {
    open(
      pj('nginx', [
        server,
        ck('nginx_key', 'k', [['path', 'k.pem']], 'k1'),
        ck(
          'nginx_key',
          'k2',
          [
            ['key_type', 'fullchain'],
            ['content', 'old'],
          ],
          'k2',
        ),
      ]),
    );
    await synth('import-nginx-key-file', { projectId: 'p', chunkId: 'k1' });
    await pickFile('server-privkey.pem', 'NEWKEY');
    const k1 = chunks().find((c) => c.id === 'k1')!;
    expect(k1.fields.find((f) => f.key === 'key_type')!.value).toBe('privkey');
    expect(k1.fields.find((f) => f.key === 'content')!.value).toBe('NEWKEY');
    await synth('import-nginx-key-file', { projectId: 'p', chunkId: 'k2' });
    await pickFile('cert-chain.pem', 'NEWCHAIN');
    const k2 = chunks().find((c) => c.id === 'k2')!;
    expect(k2.fields.find((f) => f.key === 'key_type')!.value).toBe('fullchain');
    expect(k2.fields.find((f) => f.key === 'content')!.value).toBe('NEWCHAIN');
  });

  it('exports nginx.conf to the clipboard or a file, reporting each outcome', async () => {
    open(pj('nginx', [server]));
    await press('export-nginx');
    await choose('Copy nginx.conf');
    expect(copied[0]).toContain('server');
    await press('export-nginx');
    await choose('Download nginx.conf');
    expect(saved[0][1]).toBe('nginx.conf');
    expect(toasts).toContainEqual(['Downloaded', 'ok']);
    saveResult = { ok: true };
    await press('export-nginx');
    await choose('Download nginx.conf');
    saveResult = { ok: false, error: 'bad' };
    await press('export-nginx');
    await choose('Download nginx.conf');
    expect(toasts).toContainEqual(['Failed: bad', 'error']);
  });
});

describe('stack projects', { timeout: 30000 }, () => {
  it('a section that is only allowed once is refused the second time', async () => {
    open(pj('prometheus', []));
    const btns = [...document.querySelectorAll<HTMLElement>('[data-action="add-stack-chunk"]')];
    expect(btns.length).toBeGreaterThan(0);
    for (const b of btns) {
      b.click();
      await flush();
    }
    const before = chunks().length;
    for (const b of document.querySelectorAll<HTMLElement>('[data-action="add-stack-chunk"]')) {
      b.click();
      await flush();
    }
    const refused = toasts.filter(([m, k]) => k === 'err' && m.includes('already has its'));
    expect(chunks().length - before + refused.length).toBe(btns.length);
  });

  it('an unknown chunk type adds nothing', async () => {
    open(pj('prometheus', []));
    await synth('add-stack-chunk', { projectId: 'p', chunkType: 'nope' });
    expect(chunks()).toEqual([]);
  });

  it('exports the project’s file under its tool’s name, to the clipboard or a download', async () => {
    open(pj('prometheus', []));
    await press('export-stack');
    const labels = menu();
    expect(labels[0]).toMatch(/^Copy /);
    await choose(labels[0]);
    expect(copied).toHaveLength(1);
    await press('export-stack');
    // Regression: Download threw "Cannot access 'dlText' before initialization", because the
    // helper was declared further down the handler than the stack branch that used it.
    await choose(labels[1]);
    expect(saved[0][1]).toBe(labels[1].replace('Download ', ''));
  });
});

describe('apache and haproxy imports from a file, appending', { timeout: 30000 }, () => {
  it('appends parsed chunks to the ones already there instead of replacing them', async () => {
    open(pj('apache', [ck('apache_vhost', 'old')]));
    await press('import-apache');
    await choose('Append to existing');
    await pickFile('site.conf', '<VirtualHost *:80>\n  ServerName a.test\n</VirtualHost>\n');
    expect(names()[0]).toBe('old');
    expect(chunks().length).toBeGreaterThan(1);
    open(pj('haproxy', [ck('haproxy_frontend', 'old')]));
    await press('import-haproxy');
    await choose('Append to existing');
    await pickFile('haproxy.cfg', 'frontend fe\n  bind *:80\n');
    expect(names()[0]).toBe('old');
    expect(chunks().length).toBeGreaterThan(1);
  });

  it('a project with no chunk list yet takes appended ones', async () => {
    open(pj('apache', undefined));
    largeAnswer = '<VirtualHost *:80>\n  ServerName a.test\n</VirtualHost>\n';
    await press('import-apache');
    await choose('Paste config…');
    expect(chunks().length).toBeGreaterThan(0);
  });
});

describe('projects with no chunk list yet', { timeout: 30000 }, () => {
  it('docker services, networks and volumes start the list', async () => {
    open(pj('docker', undefined));
    await press('add-docker-service');
    expect(names()).toEqual(['service-1']);
    open(pj('docker', undefined));
    promptAnswer = 'net';
    await press('add-docker-network');
    expect(names()).toEqual(['networks']);
    open(pj('docker', undefined));
    promptAnswer = 'vol';
    await press('add-docker-volume');
    expect(names()).toEqual(['volumes']);
  });

  it('stack sections, key files and imports start the list', async () => {
    open(pj('prometheus', undefined));
    document.querySelector<HTMLElement>('[data-action="add-stack-chunk"]')!.click();
    await flush();
    expect(chunks()).toHaveLength(1);
    open(pj('nginx', undefined, 'internal'));
    await press('add-nginx-key');
    await choose('Blank key file');
    expect(names()).toEqual(['key-1']);
    open(pj('nginx', undefined, 'internal'));
    await press('add-nginx-key');
    await choose('Import fullchain.pem');
    await pickFile('a.pem', 'PEM');
    expect(chunks()).toHaveLength(1);
    open(pj('nginx', undefined));
    largeAnswer = 'upstream a { server 127.0.0.1:1; }';
    await press('import-nginx');
    await choose('Paste nginx config…');
    expect(chunks()).toHaveLength(1);
    open(pj('haproxy', undefined));
    largeAnswer = 'frontend fe\n  bind *:80\n';
    await press('import-haproxy');
    await choose('Paste config…');
    expect(chunks().length).toBeGreaterThan(0);
  });

  it('removing duplicate certificate key files from a project with none does nothing', async () => {
    open(pj('nginx', undefined));
    await synth('remove-redundant-cert-chunks', { projectId: 'p' });
    expect(toasts).toEqual([]);
  });

  it('moving or duplicating on a project without chunks does nothing', async () => {
    open(pj('wireguard', undefined));
    await synth('chunk-up', { projectId: 'p', chunkId: 'x' });
    await synth('dup-chunk', { projectId: 'p', chunkId: 'x' });
    await synth('delete-chunk', { projectId: 'p', chunkId: 'x' });
    expect(toasts).toEqual([]);
  });
});

describe('menus and clicks that go nowhere', { timeout: 30000 }, () => {
  it('a click inside the view that is not on a control does nothing', async () => {
    open(pj('apache', []));
    document.querySelector<HTMLElement>('.config-view')!.click();
    await flush();
    expect(toasts).toEqual([]);
  });

  it('an action the view does not know does nothing', async () => {
    open(pj('apache', []));
    await synth('no-such-action', { projectId: 'p' });
    expect(toasts).toEqual([]);
    expect(chunks()).toEqual([]);
  });

  it('jumping with no reference, or editing a certificate with no provider, reports nothing found', async () => {
    open(pj('apache', []));
    await synth('jump-ref', {});
    expect(toasts).toContainEqual(['No vault entry matches ""', 'err']);
    await synth('edit-cert-entry', {});
    expect(toasts).toContainEqual(['Certificate entry not found', 'err']);
  });

  it('importing nginx from a file replaces or appends like pasting', async () => {
    open(pj('nginx', []));
    await press('import-nginx');
    await choose('Import from file');
    await pickFile('site.conf', 'upstream a { server 127.0.0.1:1; }');
    expect(chunks()).toHaveLength(1);
  });

  it('a certificate path that is not under /live/ names no domain', async () => {
    open(pj('nginx', []));
    largeAnswer = 'server {\n  server_name a.test;\n  ssl_certificate /etc/ssl/a.pem;\n}\n';
    await press('import-nginx');
    await choose('Paste nginx config…');
    expect(toasts[toasts.length - 1][0]).toMatch(/^Imported \d+ chunks ✓$/);
  });

  it('importing apache or haproxy from a file replaces, and cancelling a haproxy paste imports nothing', async () => {
    open(pj('apache', []));
    await press('import-apache');
    await choose('Import from file');
    await pickFile('a.conf', '<VirtualHost *:80>\n  ServerName a.test\n</VirtualHost>\n');
    expect(chunks().length).toBeGreaterThan(0);
    open(pj('haproxy', []));
    await press('import-haproxy');
    await choose('Import from file');
    await pickFile('h.cfg', 'frontend fe\n  bind *:80\n');
    expect(chunks().length).toBeGreaterThan(0);
    open(pj('haproxy', []));
    largeAnswer = null;
    await press('import-haproxy');
    await choose('Paste config…');
    expect(chunks()).toEqual([]);
  });

  it('a key file onto a chunk with no fields yet gets a type and content added', async () => {
    open(pj('nginx', [ck('nginx_key', 'bare', [], 'k0')]));
    await synth('import-nginx-key-file', { projectId: 'p', chunkId: 'k0' });
    await pickFile('privkey.pem', 'NEW');
    const f = chunks()[0].fields;
    expect(f.find((x) => x.key === 'key_type')!.value).toBe('privkey');
    expect(f.find((x) => x.key === 'content')!.value).toBe('NEW');
    expect(f[0].key).toBe('key_type');
  });

  it('a name with an unreadable path imports as the fullchain', async () => {
    open(pj('nginx', [ck('nginx_key', 'bare', [['key_type', 'privkey']], 'k0')]));
    await synth('import-nginx-key-file', { projectId: 'p', chunkId: 'k0' });
    await pickFile('weird-privkey-fullchain.pem', 'X');
    expect(chunks()[0].fields.find((x) => x.key === 'key_type')!.value).toBe('fullchain');
  });
});

void makeEntry;
