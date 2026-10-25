// SPDX-License-Identifier: PolyForm-Strict-1.0.0
// Copyright Jubair Hasan.
/**
 * Adding chunks to a structured project: every "+ Add ..." button the config
 * view draws for each project type must append exactly one chunk of the kind
 * its label says, with the fields a person would expect to fill in, and must
 * not touch the chunks already there.
 */
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { st } from '../src/ts/state';
import { render } from '../src/ts/render';
import { loadRealIndexHtml, makeProject, makeVault, resetState } from './helpers';

const toasts: [string, string?][] = [];
vi.mock('../src/ts/utils', async (importOriginal) => {
  const real = await importOriginal<typeof import('../src/ts/utils')>();
  return {
    ...real,
    showToast: (m: string, k?: string) => toasts.push([m, k]),
    showConfirm: async () => true,
    showPrompt: async () => 'named',
  };
});

const flush = (ms = 20) => new Promise((r) => setTimeout(r, ms));

function open(project: any) {
  st.vault = makeVault({
    api_keys: [],
    projects: [makeProject({ id: 'Universal', name: 'Universal' }), project],
  });
  st.currentSelectedProjectIds = [project.id];
  st.store = { save: async () => {}, isRemote: false } as never;
  render();
}

beforeEach(() => {
  loadRealIndexHtml();
  resetState(st);
  toasts.length = 0;
});

/** Buttons that append a fixed chunk, by project type, with the chunk kind each makes. */
const TABLE: [string, string, string][] = [
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
];

describe('the add buttons of each project type', { timeout: 30000 }, () => {
  it.each(TABLE)('%s: %s adds one %s chunk with fields to fill in', async (type, action, kind) => {
    open(makeProject({ id: 'p', name: 'P', project_type: type, chunks: [] } as any));
    const btn = document.querySelector<HTMLElement>(`#card-grid [data-action="${action}"]`);
    expect(btn, `${action} is drawn for a ${type} project`).not.toBeNull();
    btn!.click();
    await flush();
    const chunks = st.vault.projects.find((p) => p.id === 'p')!.chunks!;
    expect(chunks).toHaveLength(1);
    expect(chunks[0].chunk_type).toBe(kind);
    expect(chunks[0].id).toBeTruthy();
    expect(chunks[0].fields.length).toBeGreaterThan(0);
    // The new chunk is on screen straight away.
    expect(document.getElementById('card-grid')!.textContent).toContain(chunks[0].name);
  });

  it.each(TABLE)(
    '%s: %s also works on a project that has no chunk list yet',
    async (type, action, kind) => {
      open(makeProject({ id: 'p', name: 'P', project_type: type } as any));
      document.querySelector<HTMLElement>(`#card-grid [data-action="${action}"]`)!.click();
      await flush();
      const chunks = st.vault.projects.find((p) => p.id === 'p')!.chunks!;
      expect(chunks.map((c) => c.chunk_type)).toEqual([kind]);
    },
  );

  it('adding a second pg connection numbers it after the first and keeps the first', async () => {
    open(makeProject({ id: 'p', name: 'P', project_type: 'postgres', chunks: [] } as any));
    const add = () =>
      document.querySelector<HTMLElement>('#card-grid [data-action="add-pg-connection"]')!.click();
    add();
    await flush();
    add();
    await flush();
    const names = st.vault.projects.find((p) => p.id === 'p')!.chunks!.map((c) => c.name);
    expect(names).toEqual(['db-1', 'db-2']);
  });
});
