// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
/**
 * How the config view lays a project out and what its small controls do: the
 * two-column splits, the nginx certificate column with its stubs and duplicate
 * clean-up, the empty-state hint for each project type, and jumping from a
 * ${ref} badge to the entry it names.
 */
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { st } from '../src/ts/state';
import { render } from '../src/ts/render';
import { loadRealIndexHtml, makeEntry, makeProject, makeVault, resetState } from './helpers';

const toasts: [string, string?][] = [];
const copied: string[] = [];
vi.mock('../src/ts/utils', async (importOriginal) => {
  const real = await importOriginal<typeof import('../src/ts/utils')>();
  return {
    ...real,
    showToast: (m: string, k?: string) => toasts.push([m, k]),
    showConfirm: async () => true,
    clipboardWrite: async (t: string) => void copied.push(t),
  };
});

const flush = (ms = 30) => new Promise((r) => setTimeout(r, ms));
const $ = (id: string) => document.getElementById(id)!;
const ck = (chunk_type: string, name: string, fields: [string, string][] = [], id = name) => ({
  id,
  name,
  chunk_type,
  fields: fields.map(([key, value]) => ({ key, value, field_type: 'var' })),
});

function open(project: any, entries: any[] = []) {
  st.vault = makeVault({
    api_keys: entries,
    projects: [makeProject({ id: 'Universal', name: 'Universal' }), project],
  });
  st.currentSelectedProjectIds = [project.id];
  st.store = { save: async () => {}, isRemote: false } as never;
  render();
}
const labels = () => [...document.querySelectorAll('.chunks-col-label')].map((l) => l.textContent);
const pj = (project_type: string, chunks: object[], name = 'P') =>
  makeProject({ id: 'p', name, project_type, chunks } as any);

beforeEach(() => {
  loadRealIndexHtml();
  resetState(st);
  toasts.length = 0;
  copied.length = 0;
});

describe('wireguard', { timeout: 30000 }, () => {
  it('splits interface and peers into two columns, with anything else below', () => {
    open(
      pj('wireguard', [
        ck('wg_interface', 'wg0'),
        ck('wg_peer', 'laptop'),
        ck('env_file', 'extra'),
      ]),
    );
    expect(labels()).toEqual(['Interface', 'Peers']);
    expect(
      document.querySelectorAll('.chunks-two-col .chunk-card, .chunks-two-col .chunk'),
    ).not.toHaveLength(0);
    expect(document.querySelector('.chunks-grid')!.textContent).toContain('extra');
  });

  it('with only peers (or only an interface) shows one plain grid', () => {
    open(pj('wireguard', [ck('wg_peer', 'laptop')]));
    expect(document.querySelector('.chunks-two-col')).toBeNull();
    expect(document.querySelector('.chunks-grid')!.textContent).toContain('laptop');
  });

  it('with an interface and peers but nothing else shows no extra grid', () => {
    open(pj('wireguard', [ck('wg_interface', 'wg0'), ck('wg_peer', 'laptop')]));
    expect(document.querySelector('.chunks-grid')).toBeNull();
  });
});

describe('nginx certificates', { timeout: 30000 }, () => {
  const server = ck('nginx_server', 'site', [['server_name', 'www.example.test']]);
  const cert = (over: object = {}) =>
    makeEntry({
      id: 'cert1',
      provider: 'www.example.test',
      secretType: 'certificate',
      certificate_data: '-----BEGIN CERTIFICATE-----\nAAA\n-----END CERTIFICATE-----',
      cert_key_data: '-----BEGIN PRIVATE KEY-----\nBBB\n-----END PRIVATE KEY-----',
      ...over,
    } as any);

  it('offers to create the certificate entry for a domain it references, and creates a stub', async () => {
    open(pj('nginx', [server]));
    expect(labels()).toContain('TLS Certificates');
    document.querySelector<HTMLElement>('[data-action="create-cert-stub"]')!.click();
    await flush();
    const stub = st.vault.api_keys.find((e) => e.provider === 'example.test')!;
    expect(stub.secretType).toBe('certificate');
    // Regression: the stub button names a domain but no project, so the new entry used to
    // join the default project only and never showed under the project it was made from.
    expect(stub.projectIds).toEqual(['Universal', 'p']);
    expect(toasts).toContainEqual([
      'Created certificate entry for example.test — paste the PEMs',
      'ok',
    ]);
    expect(document.querySelector('.cert-link-card--missing')).toBeNull();
  });

  it('a stub with no domain does nothing', async () => {
    open(pj('nginx', [server]));
    const btn = document.querySelector<HTMLElement>('[data-action="create-cert-stub"]')!;
    btn.dataset.domain = '';
    btn.click();
    await flush();
    expect(st.vault.api_keys).toHaveLength(0);
  });

  it('Edit on a matched certificate opens that entry; one that has gone says so', async () => {
    open(pj('nginx', [server]), [cert()]);
    const edit = document.querySelector<HTMLElement>('[data-action="edit-cert-entry"]')!;
    edit.click();
    await flush();
    expect($('modal-overlay').classList.contains('open')).toBe(true);
    expect(($('edit-index') as HTMLInputElement).value).toBe('0');
    $('modal-cancel').click();
    edit.dataset.provider = 'gone.example.test';
    edit.click();
    await flush();
    expect(toasts).toContainEqual(['Certificate entry not found', 'err']);
  });

  it('hides key-file chunks that only repeat the certificate entry, and offers to remove them', async () => {
    const dup = (id: string, content: string) =>
      ck(
        'nginx_key',
        id,
        [
          ['path', `${id}.pem`],
          ['key_type', 'fullchain'],
          ['content', content],
        ],
        id,
      );
    const same = cert().certificate_data;
    open(
      pj('nginx', [server, dup('k1', same!), dup('k2', cert().cert_key_data!), dup('k3', 'OTHER')]),
      [cert()],
    );
    expect(labels()).toContain('Key Files');
    const keyFiles = document.querySelector('.chunks-col')!.textContent!;
    expect(keyFiles).toContain('k3');
    expect(keyFiles).not.toContain('k1.pem');
    expect(keyFiles).not.toContain('k2.pem');
    const cleanup = document.querySelector<HTMLElement>('.cert-cleanup-btn')!;
    expect(cleanup.textContent).toBe('Remove 2 duplicate key-file chunks (shown above)');
    cleanup.click();
    await flush();
    expect(st.vault.projects[1].chunks!.map((c) => c.id)).toEqual(['site', 'k3']);
    expect(toasts).toContainEqual(['Removed 2 duplicate key-file chunks', 'ok']);
  });

  it('says "chunk" in the singular for one duplicate', async () => {
    const dup = ck(
      'nginx_key',
      'k1',
      [
        ['path', 'k1.pem'],
        ['content', cert().certificate_data!],
      ],
      'k1',
    );
    open(pj('nginx', [server, dup]), [cert()]);
    expect(document.querySelector('.cert-cleanup-btn')!.textContent).toBe(
      'Remove 1 duplicate key-file chunk (shown above)',
    );
    document.querySelector<HTMLElement>('.cert-cleanup-btn')!.click();
    await flush();
    expect(toasts).toContainEqual(['Removed 1 duplicate key-file chunk', 'ok']);
  });

  it('removing duplicates for a project that has gone, or when none remain, does nothing', async () => {
    open(pj('nginx', [server]), [cert()]);
    const grid = $('card-grid');
    const mk = (projectId: string) => {
      const b = document.createElement('button');
      b.dataset.action = 'remove-redundant-cert-chunks';
      b.dataset.projectId = projectId;
      grid.querySelector('.config-view')!.appendChild(b);
      b.click();
    };
    mk('ghost');
    mk('p');
    await flush();
    expect(toasts).toEqual([]);
  });

  it('lists environment files in their own column', () => {
    open(pj('nginx', [server, ck('env_file', 'prod.env', [['A', '1']])]));
    expect(labels()).toContain('Environment Files');
  });

  it('with no domain to attach certificates to, shows a single grid', () => {
    open(pj('nginx', [ck('nginx_upstream', 'pool', [['name', 'pool']])], 'internal'));
    expect(document.querySelector('.chunks-two-col')).toBeNull();
    expect(document.querySelector('.nginx-grid')).not.toBeNull();
  });
});

describe('docker', { timeout: 30000 }, () => {
  it('shows a hint until there is anything beside the services', () => {
    open(pj('docker', []));
    expect(document.querySelector('.chunks-two-col-docker')!.textContent).toContain(
      'Add networks, volumes, or env files via header buttons.',
    );
  });

  it('puts env files, networks and volumes in labelled groups', () => {
    open(
      pj('docker', [
        ck('env_file', 'app.env', [['A', '1']]),
        ck('docker_network', 'backend'),
        ck('docker_volume', 'data'),
      ]),
    );
    expect(labels()).toEqual(['Environment Files', 'Networks', 'Volumes']);
  });
});

describe('other projects', { timeout: 30000 }, () => {
  it('splits type chunks from env files when both exist', () => {
    open(pj('ssh_config', [ck('ssh_host', 'box'), ck('env_file', 'x.env')]));
    expect(labels()).toEqual(
      ['SSH', 'Environment Files'].map((l) => labels()[l === 'SSH' ? 0 : 1]),
    );
    expect(labels()[1]).toBe('Environment Files');
  });

  it('shows the hint written for the project type when it has no chunks', () => {
    const hint = () => document.querySelector('.config-view')!.lastElementChild!.textContent;
    for (const [type, text] of [
      ['wireguard', 'No sections yet'],
      ['nginx', 'No blocks yet'],
      ['kubernetes', 'No resources yet'],
      ['ssh_config', 'No hosts yet'],
      ['traefik', 'No routes yet'],
      ['apache', 'No chunks yet'],
    ]) {
      open(pj(type as string, []));
      expect(hint(), type as string).toContain(text as string);
    }
  });
});

describe('badges that point at vault entries', { timeout: 30000 }, () => {
  const ref = (value: string) => pj('apache', [ck('env_file', 'e', [['K', value]])]);

  it('copies the value of a field on click', async () => {
    open(ref('plain'));
    const btn = document.createElement('button');
    btn.dataset.action = 'chunk-copy';
    btn.dataset.value = 'copy-me';
    document.querySelector('.config-view')!.appendChild(btn);
    btn.click();
    await flush();
    expect(copied).toEqual(['copy-me']);
    btn.dataset.value = '';
    btn.click();
    await flush();
    expect(copied).toEqual(['copy-me', '']);
  });

  it('jumping to a reference reveals the entry by provider, or by provider and key id, or says it found nothing', async () => {
    const entries = [
      makeEntry({ id: 'e1', provider: 'Stripe', key_id: 'live', api_key: 'sk-1' } as any),
      makeEntry({ id: 'e2', provider: 'Other', api_key: 'o' } as any),
    ];
    open(ref('${Stripe}'), entries);
    const jump = (r: string) => {
      const b = document.createElement('button');
      b.dataset.action = 'jump-ref';
      b.dataset.ref = r;
      document.querySelector('.config-view')!.appendChild(b);
      b.click();
    };
    jump('Other/api_key');
    await flush(120);
    expect(st.expanded.has('e2')).toBe(true);
    st.currentSelectedProjectIds = ['p'];
    render();
    jump('Stripe_live');
    await flush(120);
    expect(st.expanded.has('e1')).toBe(true);
    st.currentSelectedProjectIds = ['p'];
    render();
    jump('Nobody');
    await flush();
    expect(toasts).toContainEqual(['No vault entry matches "Nobody"', 'err']);
  });
});
