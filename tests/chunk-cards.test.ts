// SPDX-License-Identifier: PolyForm-Strict-1.0.0
// Copyright Jubair Hasan.
/**
 * How a chunk card draws its fields: reference badges (vault, .env, chunk,
 * unresolved), docker service lists as badges, JSON values pretty-printed,
 * images split into registry / name / tag, user ids, key files and certificate
 * cards. The card is the only place a person can see what a chunk will emit, so
 * each branch that changes what is shown is pinned here.
 */
import { describe, it, expect, beforeEach } from 'vitest';
import { st } from '../src/ts/state';
import {
  renderChunkCard,
  renderNginxCertCard,
  exportDockerCompose,
  exportNginx,
  renameProviderRefs,
} from '../src/ts/chunk-ops';
import { loadRealIndexHtml, makeEntry, makeProject, makeVault, resetState } from './helpers';

const f = (key: string, value: string, field_type = 'var', extra: object = {}) => ({
  key,
  value,
  field_type,
  ...extra,
});
const svc = (fields: object[], over: object = {}) => ({
  id: 'c1',
  name: 'web',
  chunk_type: 'docker_service',
  fields,
  ...over,
});
const draw = (
  chunk: any,
  project: any = makeProject({ id: 'p', name: 'P', chunks: [chunk] } as any),
) => {
  const el = renderChunkCard(chunk, project);
  const host = document.createElement('div');
  host.appendChild(el);
  return host;
};
const texts = (host: HTMLElement, sel: string) =>
  [...host.querySelectorAll(sel)].map((n) => n.textContent!.trim());

beforeEach(() => {
  loadRealIndexHtml();
  resetState(st);
  st.vault = makeVault({
    api_keys: [makeEntry({ provider: 'Vault key', api_key: 'real-value-123' } as any)],
    projects: [makeProject({ id: 'Universal', name: 'Universal' })],
  });
});

describe('reference badges', () => {
  it('a reference to a vault entry masks the value and offers a jump', () => {
    const h = draw(svc([f('image', '${Vault key/key}')]));
    expect(h.querySelector('.chunk-ref-jump')!.getAttribute('data-ref')).toBe('Vault key/key');
    expect(h.textContent).not.toContain('real-value-123');
  });

  it('a reference nothing can resolve says unresolved and names it', () => {
    const h = draw(svc([f('image', '${Nobody/key}')]));
    expect(h.querySelector('.chunk-ref-unresolved')!.textContent).toContain('Nobody/key');
  });

  it('a reference to a field of the project’s .env chunk says so, and masks it', () => {
    const env = {
      id: 'e',
      name: 'prod',
      chunk_type: 'env_file',
      fields: [f('TOKEN', 'env-secret-1')],
    };
    const web = svc([f('TOKEN', '${TOKEN}')]);
    const project = makeProject({ id: 'p', name: 'P', chunks: [env, web] } as any);
    st.vault.projects.push(project);
    const h = draw(web, project);
    expect(h.querySelector('.chunk-ref-env')!.textContent).toContain('.env: TOKEN');
    expect(h.textContent).not.toContain('env-secret-1');
  });

  it('a reference to another chunk’s field says so, and masks it', () => {
    const db = { id: 'd', name: 'db', chunk_type: 'pg_connection', fields: [f('host', 'db.test')] };
    const web = svc([f('DB', '${chunk:db/host}')]);
    const project = makeProject({ id: 'p', name: 'P', chunks: [db, web] } as any);
    st.vault.projects.push(project);
    const h = draw(web, project);
    expect(h.querySelector('.chunk-ref-chunk')!.textContent).toContain('chunk: db/host');
  });
});

describe('docker service fields', () => {
  it('draws ports as host → container with the protocol', () => {
    const h = draw(svc([f('ports', '8080:80/tcp\n53/udp', 'port')]));
    expect(texts(h, '.port-host')).toEqual(['8080']);
    expect(texts(h, '.port-container')).toEqual(['80', '53']);
    expect(texts(h, '.port-proto')).toEqual(['tcp', 'udp']);
  });

  it('draws volumes as host → container with the mode, shortening long host paths, and a bare name as a name', () => {
    const h = draw(
      svc([
        f(
          'volumes',
          'data:/var/lib/data\n/very/long/host/path/that/keeps/going/on:/mnt:ro\nnamed-volume',
          'list',
        ),
      ]),
    );
    expect(texts(h, '.vol-host')[0]).toBe('data');
    expect(texts(h, '.vol-host')[1]).toMatch(/^….+/);
    expect(texts(h, '.vol-container')).toEqual(['/var/lib/data', '/mnt']);
    expect(texts(h, '.port-proto')).toEqual(['ro']);
    expect(texts(h, '.vol-name')).toEqual(['named-volume']);
  });

  it('an empty volumes list says so', () => {
    const h = draw(svc([f('volumes', '', 'list')]));
    expect(h.querySelector('.port-list')!.textContent).toContain('empty');
  });

  it('draws networks and capabilities as badges, devices like volumes', () => {
    const h = draw(
      svc([
        f('networks', 'front\nback', 'list'),
        f('cap_add', 'NET_ADMIN\nSYS_TIME', 'list'),
        f('devices', '/dev/ttyUSB0:/dev/ttyUSB0', 'list'),
      ]),
    );
    expect(texts(h, '.net-badge')).toEqual(['front', 'back']);
    expect(texts(h, '.cap-badge')).toEqual(['NET_ADMIN', 'SYS_TIME']);
    expect(texts(h, '.vol-container')).toEqual(['/dev/ttyUSB0']);
  });

  it('empty networks, capabilities and devices each say empty', () => {
    const h = draw(
      svc([f('networks', '', 'list'), f('cap_add', '', 'list'), f('devices', '', 'list')]),
    );
    expect(h.querySelectorAll('.chunk-field-row')).toHaveLength(3);
    expect(h.textContent!.match(/empty/g)).toHaveLength(3);
  });

  it('splits an image into registry, name and tag', () => {
    const h = draw(svc([f('image', 'ghcr.io/org/app:1.2', 'image')]));
    expect(texts(h, '.img-registry')).toEqual(['ghcr.io/org/']);
    expect(h.querySelector('.image-badge')!.textContent).toContain('app');
    expect(texts(h, '.img-tag')).toEqual([':1.2']);
    const plain = draw(svc([f('image', 'nginx', 'image')]));
    expect(plain.querySelector('.img-registry')).toBeNull();
    expect(plain.querySelector('.img-tag')).toBeNull();
  });

  it('shows a user id as uid and gid', () => {
    const h = draw(svc([f('user', '1000:1001', 'user_id'), f('user2', '0', 'user_id')]));
    expect(texts(h, '.user-id-badge')).toEqual(['uid:1000·gid:1001', 'uid:0']);
  });

  it('pretty-prints a JSON value', () => {
    const h = draw(svc([f('labels', '{"a":1,"b":[2]}')]));
    expect(h.querySelector('pre, .json-val, .chunk-field-val')!.textContent).toContain('"a": 1');
  });

  it('copies an environment field as KEY=VALUE', () => {
    const h = draw(svc([f('TOKEN', 'abc', 'env_var')]));
    expect(h.querySelector<HTMLElement>('[data-action="chunk-copy"]')!.dataset.value).toBe(
      'TOKEN=abc',
    );
  });
});

describe('nginx and wireguard fields', () => {
  const nginx = (fields: object[]) => ({
    id: 'n',
    name: 'site',
    chunk_type: 'nginx_server',
    fields,
  });

  it('draws listen as a port with its flags, server names as badges and a redirect as a badge', () => {
    const h = draw(
      nginx([
        f('listen', '443 ssl http2'),
        f('server_name', 'a.test b.test'),
        f('return', '301 https://$host$request_uri'),
        f('proxy_pass', 'http://app:3000'),
        f('add_header', 'X-A 1;\nX-B 2'),
      ]),
    );
    expect(h.querySelector('.listen-tag')).not.toBeNull();
    expect(h.textContent).toContain('a.test');
    expect(h.textContent).toContain('301');
  });

  it('colours a redirect by its status', () => {
    const colour = (code: string) => draw(nginx([f('return', `${code} https://x.test`)])).innerHTML;
    expect(colour('301')).toContain('#4fc97e');
    expect(colour('302')).toContain('#f0ad4e');
    expect(colour('404')).toContain('var(--text2)');
  });

  it('shows the h3 and quic tags on a listen that has them', () => {
    const h = draw(nginx([f('listen', '443 quic reuseport'), f('listen', '443 ssl')]));
    expect(h.querySelectorAll('.listen-tag').length).toBeGreaterThan(0);
  });

  it('draws wireguard addresses as subnets, DNS as addresses and a listen port', () => {
    const wg = {
      id: 'w',
      name: 'wg0',
      chunk_type: 'wg_interface',
      fields: [
        f('Address', '10.0.0.1/24, fd00::1/64'),
        f('DNS', '1.1.1.1; 9.9.9.9'),
        f('ListenPort', '51820'),
        f('Endpoint', 'vpn.test:51820'),
        f('PostUp', 'iptables -A FORWARD\niptables -t nat'),
      ],
    };
    const h = draw(wg);
    expect(texts(h, '.subnet-badge')).toEqual(['10.0.0.1/24', 'fd00::1/64']);
    expect(texts(h, '.ip-badge')).toEqual(['1.1.1.1', '9.9.9.9']);
    expect(h.textContent).toContain('51820');
  });
});

describe('nginx key files and certificate cards', () => {
  const cert = (over: object = {}) =>
    makeEntry({
      provider: 'site.example.test',
      secretType: 'certificate',
      certificate_data:
        '-----BEGIN CERTIFICATE-----\nAAA\n-----END CERTIFICATE-----\n-----BEGIN CERTIFICATE-----\nBBB\n-----END CERTIFICATE-----',
      cert_key_data: '-----BEGIN PRIVATE KEY-----\nKKK\n-----END PRIVATE KEY-----',
      cert_issuer: "Let's Encrypt",
      ...over,
    } as any);

  it('a key file shows its kind, a chain’s certificate count, and which server blocks use it', () => {
    const server = {
      id: 's',
      name: 'site',
      chunk_type: 'nginx_server',
      fields: [f('ssl_certificate', '/etc/letsencrypt/live/site.example.test/fullchain.pem')],
    };
    const key = {
      id: 'k',
      name: 'fullchain',
      chunk_type: 'nginx_key',
      fields: [
        f('path', '/etc/letsencrypt/live/site.example.test/fullchain.pem'),
        f('key_type', 'fullchain'),
        f('content', cert().certificate_data!, 'cert'),
      ],
    };
    const project = makeProject({
      id: 'p',
      name: 'site.example.test',
      chunks: [server, key],
    } as any);
    st.vault.projects.push(project);
    const h = draw(key, project);
    expect(h.textContent).toContain('2 certs');
    expect(h.querySelector('.nginx-key-usedby-badge')).not.toBeNull();
    const priv = {
      ...key,
      id: 'k2',
      fields: [f('key_type', 'privkey'), f('content', cert().cert_key_data!, 'cert')],
    };
    const hp = draw(priv, project);
    expect(hp.textContent).not.toContain('certs');
  });

  it('a certificate card shows both PEMs, the issuer, the chain length and how long it has left', () => {
    const soon = new Date(Date.now() + 5 * 86_400_000).toISOString();
    st.vault.api_keys = [cert({ expires_at: soon })];
    const html = renderNginxCertCard('site.example.test').toString();
    expect(html).toContain('2 certs');
    expect(html).toContain('Let&#39;s Encrypt');
    expect(html).toContain('expires ');
    expect(html).toContain('#f0ad4e');
  });

  it('marks an expired certificate and a healthy one differently, and shows no expiry when there is none', () => {
    st.vault.api_keys = [cert({ expires_at: new Date(Date.now() - 86_400_000).toISOString() })];
    expect(renderNginxCertCard('site.example.test').toString()).toContain('EXPIRED ');
    st.vault.api_keys = [
      cert({ expires_at: new Date(Date.now() + 200 * 86_400_000).toISOString() }),
    ];
    expect(renderNginxCertCard('site.example.test').toString()).toContain('exp ');
    st.vault.api_keys = [cert({ expires_at: undefined, cert_issuer: undefined })];
    const none = renderNginxCertCard('site.example.test').toString();
    expect(none).toContain('no expiry');
    expect(none).not.toContain('cert-link-issuer');
  });

  it('a certificate with no key or chain shows the empty marker', () => {
    st.vault.api_keys = [cert({ certificate_data: '', cert_key_data: '', api_key: '' })];
    expect(renderNginxCertCard('site.example.test').toString()).toContain('— empty —');
  });
});

describe('the docker services card', () => {
  const env = (key: string, value: string) => ({
    key,
    value,
    field_type: 'env_var',
    description: 'env',
  });
  const project = (services: object[], extra: object[] = []) =>
    makeProject({
      id: 'p',
      name: 'P',
      project_type: 'docker',
      chunks: [...services, ...extra],
    } as any);
  const open = async (proj: any) => {
    st.vault.projects.push(proj);
    st.currentSelectedProjectIds = ['p'];
    st.store = { save: async () => {}, isRemote: false } as never;
    (await import('../src/ts/render')).render();
    return document.getElementById('card-grid')!;
  };

  it('shows each environment variable with how its value is linked, masking anything linked', async () => {
    const envChunk = {
      id: 'e',
      name: 'prod',
      chunk_type: 'env_file',
      fields: [f('TOKEN', 't-1', 'secret')],
    };
    const web = svc([
      env('PLAIN', 'visible'),
      env('FROM_VAULT', '${Vault key/key}'),
      env('FROM_ENV', '${TOKEN}'),
      env('GONE', '${Nobody/key}'),
    ]);
    const grid = await open(project([web], [envChunk]));
    const rows = [...grid.querySelectorAll('.dsvc-env-row')].map((r) =>
      r.textContent!.replace(/\s+/g, ' ').trim(),
    );
    expect(rows[0]).toBe('PLAIN=visible');
    expect(rows[1]).toContain('••••••••');
    expect(rows[1]).toContain('→ vault');
    expect(rows[2]).toContain('→ .env');
    expect(rows[3]).toContain('${Nobody/key}');
    expect(grid.querySelector('.chunk-ref-unresolved')).not.toBeNull();
    expect(grid.textContent).not.toContain('real-value-123');
  });

  it('lists scalars in a fixed order, then ports, volumes and lists', async () => {
    const web = svc([
      f('restart', 'always'),
      f('image', 'nginx:1', 'image'),
      f('zzz', 'last'),
      f('ports', '8080:80\nsolo', 'port'),
      f('volumes', 'data:/d\nplain', 'list'),
      f('networks', 'front, back', 'list'),
      f('depends_on', 'db', 'list'),
      f('skipped', ''),
    ]);
    const grid = await open(project([web]));
    expect(texts(grid, '.dsvc-key')).toEqual(['image', 'restart', 'zzz']);
    expect(texts(grid, '.dsvc-port-host')).toEqual(['8080']);
    expect(texts(grid, '.dsvc-port-cont')).toEqual(['80', 'solo']);
    expect(texts(grid, '.dsvc-vol-host')).toEqual(['data', 'plain']);
    expect(texts(grid, '.dsvc-net-badge')).toEqual(['front', 'back', 'db']);
  });

  it('a field with no key is not drawn in the services card', async () => {
    const grid = await open(project([svc([f('', 'orphan'), f('image', 'nginx')])]));
    expect(grid.textContent).not.toContain('orphan');
    expect(texts(grid, '.dsvc-key')).toEqual(['image']);
  });

  it('a service with no fields invites you to edit it', async () => {
    // Regression: the condition tested the array itself, which is always truthy, so the hint never showed.
    const grid = await open(project([svc([])]));
    expect(grid.querySelector('.dsvc-empty')!.textContent).toContain('click Edit');
  });
});

describe('small card and helper corners', () => {
  it('a key file whose PEM was pasted on one line is shown re-wrapped to 64 columns', () => {
    const body = 'A'.repeat(150);
    const key = {
      id: 'k',
      name: 'k',
      chunk_type: 'nginx_key',
      fields: [
        f('key_type', 'privkey'),
        f('content', `-----BEGIN PRIVATE KEY-----${body}-----END PRIVATE KEY-----`, 'cert'),
      ],
    };
    const project = makeProject({ id: 'p', name: 'P', chunks: [key] } as any);
    st.vault.projects.push(project);
    const h = draw(key, project);
    const lines = h.textContent!.split('\n').map((l) => l.trim());
    expect(lines.some((l) => l === 'A'.repeat(64))).toBe(true);
    expect(lines.some((l) => l === 'A'.repeat(22))).toBe(true);
  });

  it('a docker network or service field with no key is left out of the export and the card', async () => {
    const out = exportDockerCompose(
      makeProject({
        id: 'p',
        name: 'P',
        chunks: [
          {
            id: 'n',
            name: 'nets',
            chunk_type: 'docker_network',
            fields: [f('', 'x'), f('real', '')],
          },
          { id: 'v', name: 'vols', chunk_type: 'docker_volume', fields: [f('', 'x')] },
          svc([f('', 'orphan'), f('image', 'nginx')]),
        ],
      } as any),
    );
    expect(out.yaml).toContain('  real:');
    expect(out.yaml).not.toContain('orphan');
  });

  it('an nginx upstream field with no value is not written', () => {
    const out = exportNginx(
      makeProject({
        id: 'p',
        name: 'P',
        chunks: [
          {
            id: 'u',
            name: 'pool',
            chunk_type: 'nginx_upstream',
            fields: [f('server', ''), f('keepalive', '4')],
          },
        ],
      } as any),
    );
    expect(out).toContain('keepalive 4;');
    expect(out).not.toContain('server ;');
  });
});

describe('renaming a provider carries references and their cached names', () => {
  it('rewrites the head of each reference form and the stored reference name', () => {
    const e = makeEntry({ id: 'a', provider: 'Old', key_id: 'k', api_key: 'v' } as any);
    const mk = (value: string, ref_name?: string) => ({
      key: 'K',
      value,
      field_type: 'env_var',
      ref_name,
    });
    const chunk = {
      id: 'c',
      name: 'env',
      chunk_type: 'env_file',
      fields: [
        mk('${Old}', 'Old'),
        mk('${Old_k/key}'),
        mk('${chunk:x/y}'),
        mk('${bundle:Old/z}'),
        mk('plain'),
        mk('${Other}'),
      ],
    };
    st.vault = makeVault({
      api_keys: [e],
      projects: [
        makeProject({ id: 'Universal', name: 'Universal' }),
        makeProject({ id: 'p', name: 'P', chunks: [chunk] } as any),
      ],
    });
    const changed = renameProviderRefs('Old', 'k', 'New', 'k', 'a');
    expect(changed).toBe(2);
    const values = chunk.fields.map((x) => x.value);
    expect(values).toEqual([
      '${New_k}',
      '${New_k/key}',
      '${chunk:x/y}',
      '${bundle:Old/z}',
      'plain',
      '${Other}',
    ]);
    expect(chunk.fields[0].ref_name).toBe('New_k');
  });
});
