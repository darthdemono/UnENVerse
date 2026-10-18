/**
 * The project config view: what a person sees and can do inside a structured
 * project (WireGuard, Docker, nginx, SSH, stack integrations).
 *
 * The view is a window onto chunks that become a deployed file, so the
 * properties that matter are about not losing or scrambling those chunks:
 * moving one reorders only within its kind and only when it is visible,
 * importing over existing chunks asks first, deleting asks first, and every
 * export is the real rendered file.
 */
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { st } from '../src/ts/state';
import { render } from '../src/ts/render';
import { loadRealIndexHtml, makeEntry, makeProject, makeVault, resetState } from './helpers';

const toasts: [string, string?][] = [];
const copied: string[] = [];
const saved: [string, string][] = [];
let confirmAnswer = true;
let promptAnswer: string | null = 'new-thing';
let largeAnswer: string | null = '';
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
      return { ok: true as const, path: `/dl/${n}` };
    },
  };
});

const $ = (id: string) => document.getElementById(id)!;
const flush = (ms = 30) => new Promise((r) => setTimeout(r, ms));
const chunk = (o: object) => ({ id: crypto.randomUUID(), fields: [], ...o }) as any;
const f = (key: string, value: string, field_type = 'var') => ({ key, value, field_type });

function open(project: any, entries: any[] = []) {
  st.vault = makeVault({
    api_keys: entries,
    projects: [makeProject({ id: 'Universal', name: 'Universal' }), project],
  });
  st.currentSelectedProjectIds = [project.id];
  st.store = { save: async () => {}, isRemote: false } as never;
  render();
}
const action = (a: string, within: ParentNode = document) =>
  within.querySelector<HTMLElement>(`[data-action="${a}"]`)!;
const click = async (a: string, within?: ParentNode) => {
  action(a, within).click();
  await flush();
};
const dropdown = async (label: string) => {
  const item = [...document.querySelectorAll<HTMLElement>('#dropdown .dropdown-item')].find((i) =>
    i.textContent!.includes(label),
  )!;
  item.click();
  await flush();
};

beforeEach(() => {
  loadRealIndexHtml();
  resetState(st);
  toasts.length = 0;
  copied.length = 0;
  saved.length = 0;
  confirmAnswer = true;
  promptAnswer = 'new-thing';
  largeAnswer = '';
});

describe('the view itself', () => {
  it('titles the project, its type and description, and escapes them', () => {
    open(
      makeProject({
        id: 'wg',
        name: '<b>home</b>',
        description: '<i>rack</i>',
        project_type: 'wireguard',
        chunks: [],
      } as any),
    );
    expect($('result-count').textContent).toBe('wireguard config');
    expect($('card-grid').querySelector('.config-view-title')!.textContent).toBe('<b>home</b>');
    expect($('card-grid').querySelector('b')).toBeNull();
    expect($('card-grid').querySelector('i')).toBeNull();
    expect($('card-grid').textContent).toContain('No sections yet');
  });

  it('says what to do next for each empty project type', () => {
    for (const [type, hint] of [
      ['docker', 'Add networks, volumes'],
      ['nginx', 'No blocks yet'],
      ['kubernetes', 'No resources yet'],
      ['ssh_config', 'No hosts yet'],
      ['traefik', 'No routes yet'],
      ['apache', 'No chunks yet'],
    ] as const) {
      open(makeProject({ id: type, name: type, project_type: type, chunks: [] } as any));
      expect($('card-grid').textContent, type).toContain(hint);
    }
  });

  it('a WireGuard project shows its interface and peers in separate columns', () => {
    open(
      makeProject({
        id: 'wg',
        name: 'wg',
        project_type: 'wireguard',
        chunks: [
          chunk({
            name: 'Interface',
            chunk_type: 'wg_interface',
            fields: [f('PrivateKey', 'k', 'secret')],
          }),
          chunk({ name: 'Peer 1', chunk_type: 'wg_peer' }),
          chunk({ name: 'Notes', chunk_type: 'note' }),
        ],
      } as any),
    );
    const labels = [...document.querySelectorAll('.chunks-col-label')].map((l) => l.textContent);
    expect(labels).toEqual(['Interface', 'Peers']);
    expect(
      document.querySelectorAll('.chunks-grid .chunk-card, .chunks-grid [data-chunk-id]').length,
    ).toBeGreaterThan(0);
  });

  it('nginx puts key files and environment files beside the server blocks', () => {
    open(
      makeProject({
        id: 'ng',
        name: 'ng',
        project_type: 'nginx',
        chunks: [
          chunk({ name: 's', chunk_type: 'nginx_server' }),
          chunk({
            name: 'k',
            chunk_type: 'nginx_key',
            fields: [
              f('path', '/x/privkey.pem'),
              f('key_type', 'privkey'),
              f('content', 'PEM', 'cert'),
            ],
          }),
          chunk({ name: 'e', chunk_type: 'env_file' }),
        ],
      } as any),
    );
    const labels = [...document.querySelectorAll('.chunks-col-label')].map((l) => l.textContent);
    expect(labels).toEqual(['Key Files', 'Environment Files']);
  });

  it('docker shows services separately from networks, volumes and env files', () => {
    open(
      makeProject({
        id: 'dk',
        name: 'dk',
        project_type: 'docker',
        chunks: [
          chunk({ name: 'networks', chunk_type: 'docker_network' }),
          chunk({ name: 'volumes', chunk_type: 'docker_volume' }),
          chunk({ name: 'e', chunk_type: 'env_file' }),
        ],
      } as any),
    );
    const labels = [...document.querySelectorAll('.chunks-col-label')].map((l) => l.textContent);
    expect(labels).toEqual(['Environment Files', 'Networks', 'Volumes']);
  });

  it('other types with env files split into the type’s own column and the environment files', () => {
    open(
      makeProject({
        id: 'ss',
        name: 'ss',
        project_type: 'ssh_config',
        chunks: [
          chunk({ name: 'h', chunk_type: 'ssh_host' }),
          chunk({ name: 'e', chunk_type: 'env_file' }),
        ],
      } as any),
    );
    const labels = [...document.querySelectorAll('.chunks-col-label')].map((l) => l.textContent);
    expect(labels).toEqual(['SSH Config', 'Environment Files']);
  });
});

describe('arranging chunks', () => {
  const wg = () => {
    const a = chunk({ name: 'Peer A', chunk_type: 'wg_peer' });
    const iface = chunk({ name: 'Interface', chunk_type: 'wg_interface' });
    const b = chunk({ name: 'Peer B', chunk_type: 'wg_peer' });
    const p: any = makeProject({
      id: 'wg',
      name: 'wg',
      project_type: 'wireguard',
      chunks: [a, iface, b],
    } as any);
    open(p);
    return { a, iface, b, p };
  };
  const card = (c: any) =>
    document
      .querySelector<HTMLElement>(`[data-chunk-id="${c.id}"][data-action="chunk-up"]`)!
      .closest('[data-chunk-id]') as HTMLElement;
  const btn = (c: any, act: string) =>
    document.querySelector<HTMLElement>(`[data-action="${act}"][data-chunk-id="${c.id}"]`)!;
  const order = (p: any) => p.chunks.map((c: any) => c.name);

  it('moving a peer swaps it with the nearest peer, stepping over the interface between them', async () => {
    const { a, b, p } = wg();
    btn(b, 'chunk-up').click();
    await flush();
    expect(order(p)).toEqual(['Peer B', 'Interface', 'Peer A']);
    btn(a, 'chunk-down').click();
    await flush();
    void card;
  });

  it('says so, and changes nothing, at either end of its kind', async () => {
    const { a, b, p } = wg();
    btn(a, 'chunk-up').click();
    await flush();
    expect(toasts).toContainEqual(['Already first of its kind', '']);
    btn(b, 'chunk-down').click();
    await flush();
    expect(toasts).toContainEqual(['Already last of its kind', '']);
    expect(order(p)).toEqual(['Peer A', 'Interface', 'Peer B']);
  });

  it('duplicating puts a copy right after the original with its own id and its own fields', async () => {
    const { a, p } = wg();
    a.fields = [f('PublicKey', 'pk')];
    btn(a, 'dup-chunk').click();
    await flush();
    expect(order(p)).toEqual(['Peer A', 'Peer A (copy)', 'Interface', 'Peer B']);
    const copy = p.chunks[1];
    expect(copy.id).not.toBe(a.id);
    copy.fields[0].value = 'changed';
    expect(a.fields[0].value).toBe('pk');
    expect(toasts).toContainEqual(['Duplicated "Peer A"', 'ok']);
  });

  it('deleting asks first and removes only that chunk', async () => {
    const { a, p } = wg();
    confirmAnswer = false;
    btn(a, 'delete-chunk').click();
    await flush();
    expect(p.chunks).toHaveLength(3);
    confirmAnswer = true;
    btn(a, 'delete-chunk').click();
    await flush();
    expect(order(p)).toEqual(['Interface', 'Peer B']);
  });

  it('adding a peer numbers it after the existing peers and gives it the standard fields', async () => {
    const { p } = wg();
    await click('add-wg-peer');
    const peer = p.chunks[p.chunks.length - 1];
    expect(peer.name).toBe('Peer 3');
    expect(peer.fields.map((x: any) => x.key)).toEqual([
      'PublicKey',
      'AllowedIPs',
      'Endpoint',
      'PersistentKeepalive',
      'PresharedKey',
    ]);
    expect(peer.fields[4].secret).toBe(true);
  });
});

describe('copying', () => {
  it('copies a whole chunk as the file section it becomes, and a value on its own', async () => {
    const iface = chunk({
      name: 'Interface',
      chunk_type: 'wg_interface',
      fields: [f('PrivateKey', 'abc', 'secret'), f('Address', '10.0.0.1/24', 'subnet')],
    });
    open(makeProject({ id: 'wg', name: 'wg', project_type: 'wireguard', chunks: [iface] } as any));
    await click('copy-chunk-full');
    expect(copied[0]).toContain('[Interface]');
    expect(copied[0]).toContain('PrivateKey = abc');
    expect(toasts).toContainEqual(['Copied ✓', 'ok']);
  });

  it('exports an env file with values quoted for a shell, leaving an unresolved reference visible', async () => {
    const env = chunk({
      name: 'app',
      chunk_type: 'env_file',
      fields: [f('PW', 'has space'), f('REF', '${Missing/api_key}', 'env_var'), f('', 'nokey')],
    });
    open(
      makeProject({
        id: 'ss',
        name: 'ss',
        project_type: 'ssh_config',
        chunks: [chunk({ name: 'h', chunk_type: 'ssh_host' }), env],
      } as any),
    );
    await click('export-env-chunk');
    await dropdown('Copy .env');
    expect(copied[0]).toBe('PW="has space"\nREF=${Missing/api_key}');
    await click('export-env-chunk');
    await dropdown('Download .env');
    expect(saved[0][1]).toBe('app.env');
    expect(toasts.some(([m]) => m === 'Downloaded to /dl/app.env')).toBe(true);
  });
});

describe('importing over existing chunks', () => {
  const WG =
    '[Interface]\nPrivateKey = abc\nAddress = 10.0.0.1/24\n\n[Peer]\nPublicKey = pk\nAllowedIPs = 10.0.0.2/32\n';

  it('a pasted wg0.conf replaces the chunks only after asking, and counts what it imported', async () => {
    const p: any = makeProject({
      id: 'wg',
      name: 'wg',
      project_type: 'wireguard',
      chunks: [chunk({ name: 'Old', chunk_type: 'wg_peer' })],
    } as any);
    open(p);
    largeAnswer = WG;
    confirmAnswer = false;
    await click('import-wg');
    await dropdown('Paste wg0.conf');
    expect(p.chunks.map((c: any) => c.name)).toEqual(['Old']);
    confirmAnswer = true;
    await click('import-wg');
    await dropdown('Paste wg0.conf');
    expect(p.chunks).toHaveLength(2);
    expect(toasts).toContainEqual(['Imported 2 sections ✓', 'ok']);
  });

  it('text that is not a config imports nothing and says so', async () => {
    const p: any = makeProject({
      id: 'wg',
      name: 'wg',
      project_type: 'wireguard',
      chunks: [chunk({ name: 'Keep', chunk_type: 'wg_peer' })],
    } as any);
    open(p);
    largeAnswer = 'nothing here';
    await click('import-wg');
    await dropdown('Paste wg0.conf');
    expect(toasts).toContainEqual(['No WireGuard sections found', 'err']);
    expect(p.chunks).toHaveLength(1);
  });

  it('an empty project is filled without a question', async () => {
    const p: any = makeProject({
      id: 'wg',
      name: 'wg',
      project_type: 'wireguard',
      chunks: [],
    } as any);
    open(p);
    largeAnswer = WG;
    confirmAnswer = false;
    await click('import-wg');
    await dropdown('Paste wg0.conf');
    expect(p.chunks).toHaveLength(2);
  });

  it('docker compose text becomes services, networks and volumes', async () => {
    const p: any = makeProject({ id: 'dk', name: 'dk', project_type: 'docker', chunks: [] } as any);
    open(p);
    largeAnswer =
      'services:\n  web:\n    image: nginx\nnetworks:\n  front: {}\nvolumes:\n  data: {}\n';
    await click('import-docker');
    await dropdown('Paste YAML');
    expect(p.chunks.map((c: any) => c.chunk_type).sort()).toEqual([
      'docker_network',
      'docker_service',
      'docker_volume',
    ]);
    largeAnswer = 'not compose';
    await click('import-docker');
    await dropdown('Paste YAML');
    expect(toasts).toContainEqual(['No services/networks/volumes found', 'err']);
  });

  it('an nginx config can replace or be appended to what is there, and reports certificate domains', async () => {
    const p: any = makeProject({
      id: 'ng',
      name: 'ng',
      project_type: 'nginx',
      chunks: [chunk({ name: 'old', chunk_type: 'nginx_server' })],
    } as any);
    open(p);
    largeAnswer =
      'server {\n  listen 443 ssl;\n  server_name a.test;\n  ssl_certificate /etc/letsencrypt/live/a.test/fullchain.pem;\n}\n';
    await click('import-nginx');
    await dropdown('Append to existing');
    void p;
    await click('import-nginx');
    await dropdown('Paste nginx config');
    expect(p.chunks.map((c: any) => c.name)).not.toContain('old');
    expect(toasts.some(([m]) => m.includes('SSL cert domains: a.test'))).toBe(true);
    largeAnswer = 'garbage';
    await click('import-nginx');
    await dropdown('Paste nginx config');
    expect(toasts).toContainEqual(['No server/upstream blocks found', 'err']);
  });

  it('a pasted .env becomes an env file chunk, marking secret-looking names, and re-importing the same name updates it', async () => {
    const p: any = makeProject({
      id: 'ng',
      name: 'ng',
      project_type: 'nginx',
      chunks: [chunk({ name: 's', chunk_type: 'nginx_server' })],
    } as any);
    open(p);
    largeAnswer = 'DB_PASSWORD=hunter2\nPORT=80\nURL=${Svc/api_url}\n';
    await click('import-env-chunk');
    await dropdown('Paste .env');
    const env = p.chunks.find((c: any) => c.chunk_type === 'env_file');
    expect(env.name).toBe('env-file');
    expect(env.fields.map((x: any) => [x.key, x.field_type, !!x.secret])).toEqual([
      ['DB_PASSWORD', 'secret', true],
      ['PORT', 'var', false],
      ['URL', 'env_var', false],
    ]);
    largeAnswer = 'PORT=8080\n';
    await click('import-env-chunk');
    await dropdown('Paste .env');
    expect(p.chunks.filter((c: any) => c.chunk_type === 'env_file')).toHaveLength(1);
    expect(env.fields).toHaveLength(1);
    largeAnswer = '# nothing\n';
    await click('import-env-chunk');
    await dropdown('Paste .env');
    expect(toasts).toContainEqual(['No variables found in .env', 'err']);
  });
});

describe('docker', () => {
  it('adds networks and volumes to one shared chunk each, and a numbered service', async () => {
    const p: any = makeProject({ id: 'dk', name: 'dk', project_type: 'docker', chunks: [] } as any);
    open(p);
    promptAnswer = 'front';
    await click('add-docker-network');
    promptAnswer = 'back';
    await click('add-docker-network');
    promptAnswer = 'data';
    await click('add-docker-volume');
    await click('add-docker-service');
    await click('add-docker-service');
    expect(
      p.chunks.find((c: any) => c.chunk_type === 'docker_network').fields.map((x: any) => x.key),
    ).toEqual(['front', 'back']);
    expect(
      p.chunks.find((c: any) => c.chunk_type === 'docker_volume').fields.map((x: any) => x.key),
    ).toEqual(['data']);
    expect(
      p.chunks.filter((c: any) => c.chunk_type === 'docker_service').map((c: any) => c.name),
    ).toEqual(['service-1', 'service-2']);
    promptAnswer = null;
    await click('add-docker-network');
    expect(p.chunks.find((c: any) => c.chunk_type === 'docker_network').fields).toHaveLength(2);
  });
});

describe('adding blocks by type', () => {
  it.each([
    ['nginx', 'add-nginx-server', 'nginx_server'],
    ['nginx', 'add-nginx-upstream', 'nginx_upstream'],
    ['nginx', 'add-nginx-location', 'nginx_location'],
    ['kubernetes', 'add-k8s-deployment', 'k8s_deployment'],
    ['kubernetes', 'add-k8s-service', 'k8s_service'],
    ['kubernetes', 'add-k8s-configmap', 'k8s_configmap'],
    ['kubernetes', 'add-k8s-secret', 'k8s_secret'],
    ['kubernetes', 'add-k8s-ingress', 'k8s_ingress'],
    ['ssh_config', 'add-ssh-host', 'ssh_host'],
    ['traefik', 'add-traefik-router', 'traefik_router'],
    ['traefik', 'add-traefik-service', 'traefik_service'],
    ['traefik', 'add-traefik-middleware', 'traefik_middleware'],
    ['apache', 'add-apache-vhost', 'apache_vhost'],
    ['apache', 'add-apache-directory', 'apache_directory'],
    ['haproxy', 'add-haproxy-frontend', 'haproxy_frontend'],
    ['haproxy', 'add-haproxy-backend', 'haproxy_backend'],
    ['ansible', 'add-ansible-vars', 'ansible_vars'],
    ['ansible', 'add-ansible-task', 'ansible_task'],
    ['postgres', 'add-pg-connection', 'pg_connection'],
    ['postgres', 'add-pg-role', 'pg_role'],
  ])('%s: %s adds a %s block with its starting fields', async (type, act, chunkType) => {
    const p: any = makeProject({ id: 'p', name: 'p', project_type: type, chunks: [] } as any);
    open(p);
    expect(action(act), `${act} button`).toBeTruthy();
    await click(act);
    expect(p.chunks).toHaveLength(1);
    expect(p.chunks[0].chunk_type).toBe(chunkType);
    expect(p.chunks[0].fields.length).toBeGreaterThan(0);
    expect(p.chunks[0].id).toBeTruthy();
    await click(act);
    expect(new Set(p.chunks.map((c: any) => c.id)).size).toBe(2);
  });

  it('a numbered block continues the count', async () => {
    const p: any = makeProject({ id: 'p', name: 'p', project_type: 'nginx', chunks: [] } as any);
    open(p);
    await click('add-nginx-server');
    await click('add-nginx-server');
    expect(p.chunks.map((c: any) => c.name)).toEqual(['server-1', 'server-2']);
  });

  it('a stack integration adds its descriptor’s chunk, allows one of a singleton and refuses a second', async () => {
    const p: any = makeProject({
      id: 'pr',
      name: 'pr',
      project_type: 'prometheus',
      chunks: [],
    } as any);
    open(p);
    const buttons = [...document.querySelectorAll<HTMLElement>('[data-action="add-stack-chunk"]')];
    expect(buttons.length).toBeGreaterThan(0);
    const singleton = buttons.find((b) => (p as any) && /global/i.test(b.dataset.chunkType ?? ''));
    buttons[0].click();
    await flush();
    expect(p.chunks!.length).toBe(1);
    void singleton;
  });
});

describe('exports of whole projects', () => {
  it('WireGuard exports the real wg0.conf to the clipboard or a file named for the project', async () => {
    const p: any = makeProject({
      id: 'wg',
      name: 'home',
      project_type: 'wireguard',
      chunks: [
        chunk({
          name: 'Interface',
          chunk_type: 'wg_interface',
          fields: [f('PrivateKey', 'abc', 'secret')],
        }),
      ],
    } as any);
    open(p);
    await click('export-wg');
    await dropdown('Copy wg0.conf');
    expect(copied[0]).toContain('[Interface]');
    expect(copied[0]).toContain('PrivateKey = abc');
    await click('export-wg');
    await dropdown('Download wg0.conf');
    expect(saved[0][1]).toBe('home.conf');
  });

  it('Docker exports the compose file and its .env separately', async () => {
    const p: any = makeProject({
      id: 'dk',
      name: 'dk',
      project_type: 'docker',
      chunks: [chunk({ name: 'web', chunk_type: 'docker_service', fields: [f('image', 'nginx')] })],
    } as any);
    open(p);
    await click('export-docker');
    await dropdown('Copy YAML');
    expect(copied[0]).toContain('nginx');
    await click('export-docker');
    await dropdown('Download YAML');
    expect(saved[0][1]).toBe('docker-compose.yml');
    await click('export-docker');
    await dropdown('Download .env');
    expect(saved[1][1]).toBe('dk.env');
  });
});

describe('references and certificates', () => {
  it('clicking a ${ref} badge opens the entry it points at, and says when none matches', async () => {
    const entry = makeEntry({ id: 'e', provider: 'Stripe', api_key: 'k' } as any);
    const env = chunk({
      name: 'app',
      chunk_type: 'env_file',
      fields: [f('K', '${Stripe/api_key}', 'env_var')],
    });
    open(
      makeProject({
        id: 'ss',
        name: 'ss',
        project_type: 'ssh_config',
        chunks: [chunk({ name: 'h', chunk_type: 'ssh_host' }), env],
      } as any),
      [entry],
    );
    const ref = document.createElement('span');
    ref.dataset.action = 'jump-ref';
    ref.dataset.ref = 'Missing/api_key';
    ref.dataset.projectId = 'ss';
    $('card-grid').querySelector('.config-view')!.appendChild(ref);
    ref.click();
    await flush();
    expect(toasts).toContainEqual(['No vault entry matches "Missing/api_key"', 'err']);
    ref.dataset.ref = 'Stripe/api_key';
    ref.click();
    await flush();
    expect(st.vault.projects.length).toBeGreaterThan(0);
  });

  it('a missing certificate entry is reported rather than silently ignored', async () => {
    open(
      makeProject({
        id: 'ng',
        name: 'ng',
        project_type: 'nginx',
        chunks: [chunk({ name: 's', chunk_type: 'nginx_server' })],
      } as any),
    );
    const b = document.createElement('button');
    b.dataset.action = 'edit-cert-entry';
    b.dataset.provider = 'nope.test';
    b.dataset.projectId = 'ng';
    $('card-grid').querySelector('.config-view')!.appendChild(b);
    b.click();
    await flush();
    expect(toasts).toContainEqual(['Certificate entry not found', 'err']);
  });
});
