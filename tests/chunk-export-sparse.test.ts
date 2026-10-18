/**
 * Exporting chunks that are incomplete: fields with no value, no name, or no
 * chunk list at all. An export is a file someone deploys, so a missing value must
 * never turn into the text "undefined", and an empty project must still produce
 * something valid rather than throw.
 */
import { describe, it, expect, beforeEach } from 'vitest';
import { st } from '../src/ts/state';
import {
  chunkToString,
  exportWireGuard,
  exportDockerCompose,
  exportServicesSection,
  exportNginx,
  exportK8s,
  exportSshConfig,
  exportTraefik,
  exportApache,
  exportHaproxy,
  exportAnsible,
  exportPostgres,
} from '../src/ts/chunk-ops';
import { loadRealIndexHtml, makeEntry, makeProject, makeVault, resetState } from './helpers';

const sparseFields = [
  { key: 'A', value: undefined, field_type: 'var' },
  { key: '', value: 'orphan', field_type: 'var' },
  { key: 'B', value: '', field_type: 'var' },
  { key: 'C', value: 'kept', field_type: 'var' },
];
const mk = (chunk_type: string, name = 'thing', fields: object[] = sparseFields) =>
  ({ id: `${chunk_type}-${name}`, name, chunk_type, fields }) as any;
const proj = (project_type: string, chunks?: object[]) =>
  makeProject({ id: 'p', name: 'P', project_type, chunks } as any);

beforeEach(() => {
  loadRealIndexHtml();
  resetState(st);
  st.vault = makeVault({
    api_keys: [makeEntry({ provider: 'Pg', api_key: 'pg-secret-1' } as any)],
    projects: [makeProject({ id: 'Universal', name: 'Universal' })],
  });
});

const CHUNK_TYPES = [
  'wg_interface',
  'wg_peer',
  'env_file',
  'nginx_server',
  'nginx_location',
  'nginx_upstream',
  'ssh_host',
  'traefik_router',
  'k8s_deployment',
  'docker_service',
  'apache_vhost',
  'haproxy_frontend',
  'ansible_task',
  'pg_connection',
  'something_new',
];

describe('copying one chunk', () => {
  it.each(CHUNK_TYPES)('%s with empty and unnamed fields never prints "undefined"', (type) => {
    const out = chunkToString(mk(type));
    expect(out).not.toContain('undefined');
    expect(out).toContain('C');
  });
});

describe('exporting a whole project', () => {
  const exporters: [string, string, (p: any) => string][] = [
    ['wireguard', 'wg_peer', exportWireGuard],
    ['nginx', 'nginx_server', exportNginx],
    ['kubernetes', 'k8s_deployment', exportK8s],
    ['ssh_config', 'ssh_host', exportSshConfig],
    ['traefik', 'traefik_router', exportTraefik],
    ['apache', 'apache_vhost', exportApache],
    ['haproxy', 'haproxy_frontend', exportHaproxy],
    ['ansible', 'ansible_task', exportAnsible],
    ['postgres', 'pg_connection', exportPostgres],
    ['docker', 'docker_service', (p) => exportDockerCompose(p).yaml],
    ['docker', 'docker_service', exportServicesSection],
  ];

  it.each(exporters)('%s: sparse fields never print "undefined"', (type, chunkType, fn) => {
    const out = fn(proj(type, [mk(chunkType)]));
    expect(out).not.toContain('undefined');
  });

  it.each(exporters)(
    '%s: a project with no chunk list exports without throwing',
    (type, _c, fn) => {
      expect(() => fn(proj(type, undefined))).not.toThrow();
      expect(fn(proj(type, undefined))).not.toContain('undefined');
    },
  );

  it('a disabled chunk is left out of the export', () => {
    const on = mk('wg_peer', 'on', [{ key: 'PublicKey', value: 'k1', field_type: 'var' }]);
    const off = {
      ...mk('wg_peer', 'off', [{ key: 'PublicKey', value: 'k2', field_type: 'var' }]),
      disabled: true,
    };
    const out = exportWireGuard(proj('wireguard', [on, off]));
    expect(out).toContain('k1');
    expect(out).not.toContain('k2');
  });
});

describe('docker compose details', () => {
  const service = (fields: object[]) => mk('docker_service', 'Web App', fields);

  it('writes ports, environment, volumes and lists, and keeps references in the compose file but resolves them in the .env', () => {
    const out = exportDockerCompose(
      proj('docker', [
        service([
          { key: 'image', value: 'nginx:1', field_type: 'image' },
          { key: 'ports', value: '8080:80\n443:443', field_type: 'port' },
          { key: 'volumes', value: 'data:/data\nhost dir:/with space', field_type: 'list' },
          { key: 'networks', value: 'front, back', field_type: 'list' },
          { key: 'depends_on', value: 'db', field_type: 'list' },
          { key: 'DB_PASSWORD', value: '${Pg/key}', field_type: 'env_var' },
          { key: 'PLAIN', value: 'x', field_type: 'env_var' },
          { key: 'restart', value: 'always', field_type: 'var' },
          { key: 'command', value: 'run --flag "x"', field_type: 'var' },
          { key: 'hostname', value: 'web', field_type: 'var' },
          { key: 'extra', value: 'v', field_type: 'var' },
        ]),
        mk('docker_network', 'networks', [
          { key: 'front', value: 'bridge', field_type: 'var' },
          { key: 'back', value: '', field_type: 'var' },
        ]),
        mk('docker_network', 'empty-net', []),
        mk('docker_volume', 'volumes', [{ key: 'data', value: '', field_type: 'var' }]),
        mk('docker_volume', 'empty-vol', []),
      ]),
    );
    expect(out.yaml).toContain('web_app:');
    expect(out.yaml).toContain('image: "nginx:1"');
    expect(out.yaml).toContain('- DB_PASSWORD=${DB_PASSWORD}');
    expect(out.yaml).toContain('- "8080:80"');
    expect(out.yaml).toContain('- "host dir:/with space"');
    expect(out.yaml).toContain('driver: bridge');
    expect(out.yaml).toContain('restart: always');
    expect(out.yaml).toContain('empty-net:');
    expect(out.yaml).toContain('empty-vol:');
    expect(out.envFile).toContain('pg-secret-1');
    expect(out.yaml).not.toContain('pg-secret-1');
  });

  it('a project with no services exports no services block, and an empty env gives an empty file', () => {
    const out = exportDockerCompose(proj('docker', []));
    expect(out.yaml).not.toContain('services:');
    expect(out.envFile).toBe('');
  });
});

describe('exports fall back to sensible defaults and skip what is not theirs', () => {
  const fieldsOf = (pairs: [string, string][]) =>
    pairs.map(([key, value]) => ({ key, value, field_type: 'var' }));

  it('kubernetes: a bare ingress, deployment and secret are valid and use the chunk name', () => {
    const out = exportK8s(
      proj('kubernetes', [
        { id: '1', name: 'site', chunk_type: 'k8s_ingress', fields: [] },
        { id: '2', name: 'app', chunk_type: 'k8s_deployment', fields: [] },
        {
          id: '3',
          name: 'creds',
          chunk_type: 'k8s_secret',
          fields: fieldsOf([
            ['name', 'creds'],
            ['password', 'p£ss'],
            ['empty', ''],
          ]),
        },
        {
          id: '4',
          name: 'cfg',
          chunk_type: 'k8s_configmap',
          fields: fieldsOf([
            ['name', 'cfg'],
            ['k', 'v'],
          ]),
        },
        { id: '5', name: 'ignored', chunk_type: 'something_else', fields: [] },
      ]),
    );
    expect(out).toContain('host: example.com');
    expect(out).toContain('name: site');
    expect(out).toContain('image: nginx:latest');
    expect(out).toContain(
      `password: ${btoa(String.fromCharCode(...new TextEncoder().encode('p£ss')))}`,
    );
    expect(out).toContain('k: "v"');
    expect(out).not.toContain('ignored');
    expect(out.split('\n---\n')).toHaveLength(4);
  });

  it('kubernetes: environment variables with secret or env type go in the deployment', () => {
    const out = exportK8s(
      proj('kubernetes', [
        {
          id: '1',
          name: 'app',
          chunk_type: 'k8s_deployment',
          fields: [
            { key: 'image', value: 'img:1', field_type: 'var' },
            { key: 'TOKEN', value: 'abc', field_type: 'secret' },
            { key: 'MODE', value: 'on', field_type: 'env_var' },
          ],
        },
      ]),
    );
    expect(out).toContain('- name: TOKEN');
    expect(out).toContain('value: "abc"');
    expect(out).toContain('- name: MODE');
  });

  it('traefik: a service without a URL has no server list, a middleware without a type is just its name', () => {
    const out = exportTraefik(
      proj('traefik', [
        { id: '1', name: 'svc', chunk_type: 'traefik_service', fields: [] },
        {
          id: '2',
          name: 'svc2',
          chunk_type: 'traefik_service',
          fields: fieldsOf([
            ['url', 'http://a:1'],
            ['passHostHeader', 'false'],
          ]),
        },
        { id: '3', name: 'mw', chunk_type: 'traefik_middleware', fields: [] },
        {
          id: '4',
          name: 'mw2',
          chunk_type: 'traefik_middleware',
          fields: fieldsOf([
            ['type', 'headers'],
            ['x', 'y'],
          ]),
        },
        {
          id: '5',
          name: 'router',
          chunk_type: 'traefik_router',
          fields: [
            { key: 'entryPoints', value: 'web, websecure', field_type: 'list' },
            { key: 'rule', value: 'Host(`a.test`)', field_type: 'var' },
            { key: 'empty', value: '', field_type: 'var' },
          ],
        },
      ]),
    );
    expect(out).toContain('passHostHeader: true');
    expect(out).toContain('passHostHeader: false');
    expect(out.match(/servers:/g)).toHaveLength(1);
    expect(out).toContain('headers:');
    expect(out).toContain('- web');
    expect(out).toContain('- websecure');
    expect(out).not.toContain('empty');
  });

  it('nginx: upstream servers split on commas and lines; a location without a path is "/"', () => {
    const out = exportNginx(
      proj('nginx', [
        {
          id: '1',
          name: 'pool',
          chunk_type: 'nginx_upstream',
          fields: fieldsOf([
            ['server', 'a:1, b:2\nc:3'],
            ['keepalive', '8'],
          ]),
        },
        { id: '2', name: 'site', chunk_type: 'nginx_server', fields: fieldsOf([['listen', '80']]) },
        {
          id: '3',
          name: 'loc',
          chunk_type: 'nginx_location',
          fields: fieldsOf([['proxy_pass', 'http://pool']]),
        },
      ]),
    );
    expect(out).toContain('server a:1;');
    expect(out).toContain('server b:2;');
    expect(out).toContain('server c:3;');
    expect(out).toContain('keepalive 8;');
    expect(out).toContain('location / {');
  });

  it('ssh: only host chunks are written, skipping fields without values', () => {
    const out = exportSshConfig(
      proj('ssh_config', [
        {
          id: '1',
          name: 'box',
          chunk_type: 'ssh_host',
          fields: fieldsOf([
            ['HostName', '10.0.0.5'],
            ['User', ''],
          ]),
        },
        { id: '2', name: 'note', chunk_type: 'env_file', fields: fieldsOf([['A', '1']]) },
      ]),
    );
    expect(out).toContain('Host box');
    expect(out).toContain('HostName 10.0.0.5');
    expect(out).not.toContain('User');
    expect(out).not.toContain('note');
  });

  it('apache and haproxy: chunks of another kind are left out', () => {
    expect(
      exportApache(
        proj('apache', [
          { id: '1', name: 'x', chunk_type: 'env_file', fields: fieldsOf([['A', '1']]) },
        ]),
      ),
    ).not.toContain('A 1');
    expect(
      exportHaproxy(
        proj('haproxy', [
          { id: '1', name: 'x', chunk_type: 'env_file', fields: fieldsOf([['A', '1']]) },
        ]),
      ),
    ).not.toContain('A 1');
  });
});
