// SPDX-License-Identifier: PolyForm-Strict-1.0.0
// Copyright Jubair Hasan.
/** Branch coverage for nodes-pane.ts: every answer shape, button and read-back path. */
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { loadRealIndexHtml, resetState } from './helpers';

const confirms: string[] = [];
let confirmAnswer = true;
let promptAnswer: string | null = null;
let saveResult: any = { ok: true, path: '/dl/x' };
const toast = vi.fn();
vi.mock('../src/ts/utils', async (importOriginal) => {
  const real = await importOriginal<typeof import('../src/ts/utils')>();
  return {
    ...real,
    showToast: (...a: unknown[]) => toast(...a),
    showConfirm: async (m: string) => {
      confirms.push(m);
      return confirmAnswer;
    },
    showPrompt: async () => promptAnswer,
    saveFile: async () => saveResult,
  };
});

const $ = (id: string) => document.getElementById(id)!;
const flush = (ms = 20) => new Promise((r) => setTimeout(r, ms));

type Answer = { status: number; body: unknown };
type Calls = [string, string, unknown][];
let handler: (m: string, p: string, b: unknown) => Answer | Error | string = () => ({
  status: 200,
  body: {},
});
let calls: Calls = [];
let tauri: ((cmd: string, args: any) => unknown) | null = null;

async function boot(withRemote = true, projects: any[] = []) {
  vi.resetModules();
  loadRealIndexHtml();
  if (tauri)
    (window as any).__TAURI__ = { core: { invoke: async (c: string, a: any) => tauri!(c, a) } };
  else delete (window as any).__TAURI__;
  const state = await import('../src/ts/state');
  (await import('../src/ts/tools-markup')).mountToolsPanes();
  resetState(state.st);
  state.st.vault = { api_keys: [], user_categories: [], projects } as never;
  state.st.store = { save: async () => {}, isRemote: false } as any;
  calls = [];
  if (withRemote) {
    const remote = new state.RemoteVaultStore('http://localhost:1');
    remote.nodesRequest = async (m, p, b) => {
      calls.push([m, p, b]);
      const r = handler(m, p, b);
      if (r instanceof Error) throw r;
      if (typeof r === 'string') throw r as unknown as Error;
      return r;
    };
    state.st.store = remote;
  }
  const mod = await import('../src/ts/nodes-pane');
  mod.initNodesPane();
  return { mod, st: state.st };
}

const ok = (body: unknown = {}): Answer => ({ status: 200, body });
const target = (over: object = {}) => ({
  id: 't1',
  mode: 'push',
  apply: true,
  status: 'ok',
  ...over,
});
const node = (over: object = {}) => ({
  id: 'n1',
  name: 'vps',
  fingerprint: 'ab'.repeat(32),
  projects: ['p'],
  targets: [],
  ...over,
});
const load = async (nodes: any[]) => {
  handler = (m, p) => (p === '/api/nodes' ? ok({ nodes }) : ok());
  $('nodes-refresh-btn').click();
  await flush();
};
const click = async (sel: string, wait = 20) => {
  (document.querySelector(sel) as HTMLElement).click();
  await flush(wait);
};

beforeEach(() => {
  confirms.length = 0;
  confirmAnswer = true;
  promptAnswer = null;
  saveResult = { ok: true, path: '/dl/x' };
  toast.mockClear();
  tauri = null;
  handler = () => ok();
});
afterEach(() => {
  vi.useRealTimers();
  delete (window as any).__TAURI__;
});

describe('server answers', () => {
  it('names each refusal', async () => {
    await boot();
    for (const [answer, re] of [
      [{ status: 404, body: null }, /--nodes/],
      [{ status: 403, body: null }, /vault owner/],
      [{ status: 429, body: null }, /Rate limited/],
      [{ status: 500, body: { error: 'boom' } }, /Refused \(500\): boom/],
      [{ status: 500, body: null }, /^Refused \(500\)$/],
    ] as const) {
      handler = () => answer;
      $('nodes-refresh-btn').click();
      await flush();
      expect($('nodes-status').textContent).toMatch(re);
    }
    handler = () => new Error('net');
    $('nodes-refresh-btn').click();
    await flush();
    expect($('nodes-status').textContent).toContain('Could not reach the server: net');
    handler = () => 'plain';
    $('nodes-refresh-btn').click();
    await flush();
    expect($('nodes-status').textContent).toContain('plain');
  });
});

describe('rendering', () => {
  it('draws an empty list, a missing body and plural counts', async () => {
    await boot();
    await load([]);
    expect($('nodes-list').textContent).toContain('No nodes enrolled.');
    expect($('nodes-status').textContent).toBe('0 nodes');
    handler = () => ok(null);
    $('nodes-refresh-btn').click();
    await flush();
    await load([node(), node({ id: 'n2' })]);
    expect($('nodes-status').textContent).toBe('2 nodes');
  });

  it('draws every node and target variant', async () => {
    await boot();
    await load([
      node({
        revoked_at: 't',
        approval: 'device',
        listen: { endpoint: 'https://n:1', cert_sha256: 'c'.repeat(64) },
        last_polled: null,
        host: { hostname: 'h' },
        approvals: [
          {
            id: 'a1',
            target: 't1',
            sha256: 'f'.repeat(64),
            status: 'pending',
            requested_at: 'r',
            from_seq: 1,
            to_seq: 2,
          },
          {
            id: 'a2',
            target: 't1',
            sha256: 'e'.repeat(64),
            status: 'approved',
            requested_at: 'r',
            decided_by: 'me',
          },
          { id: 'a3', target: 't1', sha256: 'd'.repeat(64), status: 'rejected', requested_at: 'r' },
        ],
        targets: [target({ refusal: 'no', snapshot: { seq: 3, at: 'x' } })],
      }),
      node({
        id: 'n2',
        name: 'n2',
        approval: 'required',
        last_seen: 'seen',
        last_polled: 'p',
        listen: { endpoint: 'e', cert_sha256: 'c'.repeat(64) },
        host: { hostname: 'h', os: 'linux', arch: 'x', version: '1' },
        approvals: [
          { id: 'a9', target: 't', sha256: 'f'.repeat(64), status: 'pending', requested_at: 'r' },
        ],
        targets: [
          target({ mode: 'pull', exporter: 'env', project: 'p', error: 'bad' }),
          target({ id: 't2', mode: 'pull', exporter: 'compose-env' }),
          target({ id: 't3', mode: 'pull', exporter: 'nginx' }),
          target({ id: 't4', mode: 'pull', exporter: 'toml' }),
          target({ id: 't5', mode: 'pull' }),
        ],
      }),
      node({
        id: 'n3',
        name: 'n3',
        approval: 'none',
        approvals: [
          { id: 'z', target: 't', sha256: 'f'.repeat(64), status: 'approved', requested_at: 'r' },
        ],
      }),
    ]);
    const text = $('nodes-list').textContent!;
    expect(text).toContain('(revoked)');
    expect(text).toContain('file matches history #3');
    expect(text).toContain('Every push to this node needs a yes signed on your own device');
    expect(text).toContain('Every push to this node is held for your approval');
    expect(text).toContain('by me');
    expect(document.querySelectorAll('[data-node-act="into-chunk"]')).toHaveLength(3);
    expect(document.querySelectorAll('[data-node-act="policy"]').length).toBeGreaterThan(2);
  });
});

describe('policy, revoke, accept, blast, approvals', () => {
  it('toggles approval policy, revokes and accepts', async () => {
    await boot();
    await load([node({ approval: 'required', targets: [target({ mode: 'pull' })] })]);
    await click('[data-node-act="policy"]');
    expect(
      calls.some(([, p, b]) => p === '/api/nodes/n1/policy' && (b as any).approval === 'none'),
    ).toBe(true);
    confirmAnswer = false;
    await click('[data-node-act="revoke"]');
    expect(calls.some(([m]) => m === 'DELETE')).toBe(false);
    confirmAnswer = true;
    await click('[data-node-act="revoke"]');
    expect(calls.some(([m]) => m === 'DELETE')).toBe(true);
    await load([node({ targets: [target({ mode: 'pull' })] })]);
    await click('[data-node-act="accept"]');
    expect(calls.some(([, p]) => p === '/api/nodes/n1/accept')).toBe(true);
    handler = (m, p) =>
      p === '/api/nodes'
        ? ok({ nodes: [node({ targets: [target({ mode: 'pull' })] })] })
        : { status: 500, body: null };
    await click('[data-node-act="accept"]');
    await click('[data-node-act="policy"]');
    await click('[data-node-act="revoke"]');
  });

  it('blast radius: with and without a since, failure, and the full report text', async () => {
    const { mod } = await boot();
    await load([node()]);
    ($('nodes-since') as HTMLInputElement).value = '2026-01-01';
    handler = (m, p) =>
      p === '/api/history'
        ? ok({
            host: 'vps',
            since: '2026-01-01',
            deployments: 2,
            entries: [
              {
                provider: 'GH',
                key_id: 'k',
                fields: ['f'],
                first_seen: 'a',
                last_seen: 'b',
                times: 2,
                still_current: true,
                removed: false,
                short: true,
                console_url: 'https://c',
              },
              {
                provider: 'AWS',
                key_id: '',
                fields: ['f'],
                first_seen: 'a',
                last_seen: 'b',
                times: 1,
                still_current: false,
                removed: false,
                short: false,
              },
              {
                provider: 'Old',
                key_id: '',
                fields: [],
                first_seen: 'a',
                last_seen: 'b',
                times: 1,
                still_current: false,
                removed: true,
                short: false,
              },
            ],
            unaccounted: [{ at: 't', via: 'v', sha256: 'f'.repeat(64) }],
            command: 'unv rotate x',
          })
        : ok();
    await click('[data-node-act="blast"]');
    const out = $('nodes-output').textContent!;
    expect(out).toContain('since 2026-01-01');
    expect(out).toContain('ROTATE  GH:k');
    expect(out).toContain('[short value');
    expect(out).toContain('revoke at https://c');
    expect(out).toContain('rotated AWS');
    expect(out).toContain('removed Old');
    expect(out).toContain('have no recorded contents');
    expect(out).toContain('unv rotate x');
    expect((calls[calls.length - 1][2] as any).args.since).toBe('2026-01-01');
    ($('nodes-since') as HTMLInputElement).value = '';
    await click('[data-node-act="blast"]');
    expect((calls[calls.length - 1][2] as any).args.since).toBeUndefined();
    expect(
      mod.formatBlast({ host: 'h', deployments: 0, entries: [], unaccounted: [], command: '' }),
    ).toContain('Nothing in the vault needs rotating');
    handler = () => ({ status: 500, body: null });
    $('nodes-output').textContent = 'kept';
    await click('[data-node-act="blast"]');
    expect($('nodes-output').textContent).toBe('kept');
  });

  const approvalNode = (over: object = {}, a: object = {}) =>
    node({
      approvals: [
        {
          id: 'a1',
          target: 't1',
          sha256: 'f'.repeat(64),
          status: 'pending',
          requested_at: 'r',
          from_seq: 1,
          to_seq: 2,
          ...a,
        },
      ],
      ...over,
    });

  it('review: diff, whole file, missing proposal, failures and an identical diff', async () => {
    await boot();
    await load([approvalNode()]);
    handler = (m, p, b) =>
      p === '/api/history'
        ? (b as any).op === 'diff'
          ? ok({ diff: '-a\n+b' })
          : ok({ text: 'l1\nl2' })
        : ok();
    await click('[data-node-act="review"]');
    expect($('nodes-output').textContent).toBe('-a\n+b');
    handler = () => ok({});
    await click('[data-node-act="review"]');
    expect($('nodes-output').textContent).toBe('The two render identically.');
    handler = () => ({ status: 500, body: null });
    $('nodes-output').textContent = 'kept';
    await click('[data-node-act="review"]');
    expect($('nodes-output').textContent).toBe('kept');

    await load([approvalNode({}, { from_seq: null })]);
    handler = () => ok({ text: 'l1\nl2' });
    await click('[data-node-act="review"]');
    expect($('nodes-output').textContent).toContain('+l1\n+l2');
    handler = () => ok({});
    await click('[data-node-act="review"]');
    expect($('nodes-output').textContent).toContain('whole proposed file.\n+');
    handler = () => ({ status: 500, body: null });
    $('nodes-output').textContent = 'kept';
    await click('[data-node-act="review"]');
    expect($('nodes-output').textContent).toBe('kept');

    await load([approvalNode({}, { to_seq: null })]);
    await click('[data-node-act="review"]');
    expect($('nodes-output').textContent).toContain('not in the history');
  });

  it('approve and reject, including a declined question and a failure', async () => {
    await boot();
    await load([approvalNode()]);
    confirmAnswer = false;
    await click('[data-node-act="approve"]');
    expect(calls.some(([, p]) => p.includes('/approve'))).toBe(false);
    confirmAnswer = true;
    await click('[data-node-act="approve"]');
    expect(calls.some(([, p]) => p === '/api/node-approvals/a1/approve')).toBe(true);
    await load([approvalNode()]);
    await click('[data-node-act="reject"]');
    expect(calls.some(([, p]) => p === '/api/node-approvals/a1/reject')).toBe(true);
    await load([approvalNode()]);
    handler = () => ({ status: 500, body: null });
    await click('[data-node-act="reject"]');
  });

  it('approve with a device key signs here, and explains what it cannot do', async () => {
    await boot();
    await load([approvalNode({ approval: 'device' })]);
    await click('[data-node-act="approve"]');
    expect($('nodes-status').textContent).toContain('Signing needs the desktop app');
    tauri = async () => {
      throw new Error('no key');
    };
    await boot();
    await load([approvalNode({ approval: 'device' })]);
    await click('[data-node-act="approve"]');
    expect($('nodes-status').textContent).toContain('Could not sign: no key');
    tauri = async () => {
      throw 'plain' as unknown as Error;
    };
    await boot();
    await load([approvalNode({ approval: 'device' })]);
    await click('[data-node-act="approve"]');
    expect($('nodes-status').textContent).toContain('plain');
    tauri = async () => ({ token: 't', sig: 's' });
    await boot();
    await load([approvalNode({ approval: 'device' })]);
    await click('[data-node-act="approve"]');
    expect((calls[calls.length - 2][2] as any).signed).toEqual({ token: 't', sig: 's' });
  });

  it('an approval button outside an approval row does nothing', async () => {
    await boot();
    await load([node()]);
    const card = document.querySelector('section')!;
    const b = document.createElement('button');
    b.dataset.nodeAct = 'approve';
    card.appendChild(b);
    b.click();
    await flush();
    expect(confirms).toHaveLength(0);
  });
});

describe('pulling files', () => {
  const pullNode = () =>
    node({ targets: [target({ mode: 'pull', exporter: 'env', project: 'p' })] });

  it('fetches the file once the node answers and saves it', async () => {
    await boot();
    await load([pullNode()]);
    vi.useFakeTimers();
    let polls = 0;
    handler = (m, p) => {
      if (p.endsWith('/pull')) return ok();
      polls++;
      return polls < 2 ? { status: 204, body: null } : ok({ content_b64: btoa('SECRET=1') });
    };
    (document.querySelector('[data-node-act="pull"]') as HTMLElement).click();
    await vi.advanceTimersByTimeAsync(2000);
    expect($('nodes-status').textContent).toContain('File saved to /dl/x');
    saveResult = { ok: true };
    (document.querySelector('[data-node-act="pull"]') as HTMLElement).click();
    await vi.advanceTimersByTimeAsync(2000);
    expect($('nodes-status').textContent).toMatch(/^File saved \(the file/);
    saveResult = { ok: false, error: 'denied' };
    (document.querySelector('[data-node-act="pull"]') as HTMLElement).click();
    await vi.advanceTimersByTimeAsync(2000);
    expect($('nodes-status').textContent).toContain('Could not save the file: denied');
    handler = (m, p) => (p.endsWith('/pull') ? ok() : ok({}));
    (document.querySelector('[data-node-act="pull"]') as HTMLElement).click();
    await vi.advanceTimersByTimeAsync(2000);
  });

  it('gives up on a refusal, a failed request, and a node that never answers', async () => {
    await boot();
    await load([pullNode()]);
    vi.useFakeTimers();
    handler = (m, p) => (p.endsWith('/pull') ? { status: 500, body: null } : ok());
    (document.querySelector('[data-node-act="pull"]') as HTMLElement).click();
    await vi.advanceTimersByTimeAsync(100);
    handler = (m, p) => (p.endsWith('/pull') ? ok() : { status: 410, body: null });
    (document.querySelector('[data-node-act="pull"]') as HTMLElement).click();
    await vi.advanceTimersByTimeAsync(2000);
    expect($('nodes-status').textContent).toBe('Refused (410)');
    handler = (m, p) => (p.endsWith('/pull') ? ok() : { status: 204, body: null });
    (document.querySelector('[data-node-act="pull"]') as HTMLElement).click();
    await vi.advanceTimersByTimeAsync(70_000);
    expect($('nodes-status').textContent).toContain('did not answer within about a minute');
  });
});

describe('reading a pulled file into chunks', () => {
  const intoChunk = (exporter: string, project = 'p') =>
    node({ targets: [target({ mode: 'pull', exporter, project })] });
  const answerFile = (text: string) => {
    handler = (m, p) =>
      p.endsWith('/pull')
        ? ok()
        : p === '/api/nodes'
          ? ok({ nodes: [] })
          : ok({ content_b64: btoa(text) });
  };
  const run = async () => {
    vi.useFakeTimers();
    (document.querySelector('[data-node-act="into-chunk"]') as HTMLElement).click();
    await vi.advanceTimersByTimeAsync(3000);
    vi.useRealTimers();
  };
  const NGINX = 'server {\n  listen 80;\n  server_name a.test;\n}\n';
  const chunk = (name: string, over: object = {}) => ({
    id: name,
    name,
    chunk_type: 'env_file',
    fields: [{ key: 'A', value: '1' }],
    ...over,
  });

  it('needs the desktop app', async () => {
    await boot();
    await load([intoChunk('env')]);
    await click('[data-node-act="into-chunk"]');
    expect($('nodes-status').textContent).toContain('needs the desktop app');
  });

  it('restores a config family from the file: unreadable, declined and accepted', async () => {
    tauri = async () => null;
    const project = { id: 'p', name: 'edge', chunks: [] as any[] };
    await boot(true, [project]);
    await load([intoChunk('nginx', 'edge')]);
    answerFile('garbage');
    await run();
    expect($('nodes-status').textContent).toContain('could be read as chunks');
    answerFile(NGINX);
    confirmAnswer = false;
    await run();
    expect($('nodes-status').textContent).toBe('Nothing was changed.');
    confirmAnswer = true;
    await run();
    expect($('nodes-status').textContent).toContain("'edge' updated from the node's file");
    expect(project.chunks.length).toBeGreaterThan(0);
    handler = (m, p) => (p.endsWith('/pull') ? { status: 500, body: null } : ok());
    await run();
  });

  it('reads into an env chunk: no project, no chunk, a choice, a failed plan, differences', async () => {
    tauri = async (cmd) => {
      if (cmd === 'env_import_plan')
        return {
          fields: [{ key: 'B', value: '2' }],
          added: ['B'],
          changed: ['C'],
          removed: ['A'],
          kept_references: ['R'],
        };
      return null;
    };
    const one = { id: 'p', name: 'edge', chunks: [chunk('first')] };
    const two = { id: 'q', name: 'multi', chunks: [chunk('first'), chunk('second')] };
    const none = { id: 'r', name: 'bare', chunks: [] };
    await boot(true, [one, two, none]);
    await load([intoChunk('env', 'ghost')]);
    answerFile('A=1');
    await run();
    expect($('nodes-status').textContent).toContain('no .env chunk');
    await load([intoChunk('env', 'bare')]);
    await run();
    expect($('nodes-status').textContent).toContain('no .env chunk');

    await load([intoChunk('env', 'multi')]);
    promptAnswer = 'nope';
    await run();
    expect($('nodes-status').textContent).toBe('No such chunk; nothing was read.');
    promptAnswer = 'second';
    answerFile('A=1');
    await run();
    expect($('nodes-status').textContent).toContain("Chunk 'second' updated");
    expect(confirms[confirms.length - 1]).toContain('Added: B');
    expect(confirms[confirms.length - 1]).toContain('Kept as references: R');

    await load([intoChunk('env', 'edge')]);
    handler = (m, p) => (p.endsWith('/pull') ? { status: 500, body: null } : ok());
    await run();
    answerFile('A=1');
    confirmAnswer = false;
    await run();
    expect($('nodes-status').textContent).toBe('Nothing was changed.');
  });

  it('reports an unreadable plan and says when nothing differs', async () => {
    let mode: 'throw' | 'string' | 'same' = 'throw';
    tauri = async () => {
      if (mode === 'throw') throw new Error('bad env');
      if (mode === 'string') throw 'plain' as unknown as Error;
      return { fields: [], added: [], changed: [], removed: [], kept_references: [] };
    };
    const one = { id: 'p', name: 'edge', chunks: [chunk('first')] };
    await boot(true, [one]);
    await load([intoChunk('compose-env', 'edge')]);
    answerFile('A=1');
    await run();
    expect($('nodes-status').textContent).toContain('Could not read the file: bad env');
    mode = 'string';
    await run();
    expect($('nodes-status').textContent).toContain('plain');
    mode = 'same';
    await run();
    expect(confirms[confirms.length - 1]).toContain('Nothing differs.');
  });
});

describe('enrolment', () => {
  it('registers this device only in the desktop app', async () => {
    await boot();
    $('nodes-approver-btn').click();
    await flush();
    expect($('nodes-status').textContent).toContain('approver key lives in the desktop app');
    tauri = async () => ({ public_key: 'PUB', fingerprint: 'f'.repeat(40) });
    await boot();
    $('nodes-approver-btn').click();
    await flush();
    expect($('nodes-status').textContent).toContain('Registered this device');
    handler = () => ({ status: 500, body: null });
    $('nodes-approver-btn').click();
    await flush();
    tauri = async () => {
      throw new Error('locked');
    };
    await boot();
    $('nodes-approver-btn').click();
    await flush();
    expect($('nodes-status').textContent).toContain('Could not register: locked');
    tauri = async () => {
      throw 'plain' as unknown as Error;
    };
    await boot();
    $('nodes-approver-btn').click();
    await flush();
    expect($('nodes-status').textContent).toContain('plain');
  });

  it('mints a token: validation, default ttl, failure, success', async () => {
    await boot();
    const set = (name: string, projects: string, ttl: string) => {
      ($('nodes-name') as HTMLInputElement).value = name;
      ($('nodes-projects') as HTMLInputElement).value = projects;
      ($('nodes-ttl') as HTMLInputElement).value = ttl;
    };
    set('', 'p', '');
    $('nodes-token-btn').click();
    expect($('nodes-status').textContent).toBe('Give the node a name.');
    set('n', ' , ', '');
    $('nodes-token-btn').click();
    expect($('nodes-status').textContent).toContain('Name at least one project');
    set('n', 'a, b', '');
    handler = () => ok({ token: 'TOK' });
    $('nodes-token-btn').click();
    await flush();
    expect($('nodes-output').textContent).toBe('TOK');
    expect((calls[calls.length - 1][2] as any).ttl_secs).toBe(900);
    handler = () => ok({});
    $('nodes-token-btn').click();
    await flush();
    expect($('nodes-output').textContent).toBe('');
    handler = () => ({ status: 500, body: null });
    $('nodes-output').textContent = 'kept';
    $('nodes-token-btn').click();
    await flush();
    expect($('nodes-output').textContent).toBe('kept');
  });
});

describe('buttons with missing data and the pane outside a remote vault', () => {
  const bare = (html: string) => {
    $('nodes-list').innerHTML = html;
  };

  it('a click on the list that is not on a node action, or inside no node, does nothing', async () => {
    await boot();
    await flush(60);
    bare(
      '<div data-node-id="n1"><span id="x"></span></div><button data-node-act="pull" id="orphan"></button>',
    );
    const before = calls.length;
    $('nodes-list').click();
    $('x').click();
    $('orphan').click();
    await flush();
    expect(calls).toHaveLength(before);
  });

  it('actions with no target send an empty one, and a policy with none named is "none"', async () => {
    await boot();
    await flush(60);
    handler = () => ok();
    const card = (act: string) =>
      bare(
        `<div data-node-id="n1" data-node-name="vps"><button data-node-act="${act}" id="b"></button></div>`,
      );
    card('policy');
    await click('#b');
    expect(calls.find(([, p]) => p.endsWith('/policy'))![2]).toEqual({ approval: 'none' });
    card('accept');
    await click('#b');
    expect(calls.find(([, p]) => p.endsWith('/accept'))![2]).toEqual({ target: '' });
  });

  it('approving from a row with no hash or node name still asks, naming "this node"', async () => {
    await boot();
    await flush(60);
    handler = () => ok();
    bare(
      '<div data-node-id="n1"><div data-approval-id="a1"><button data-node-act="approve" id="ap"></button></div></div>',
    );
    await click('#ap');
    expect(confirms[confirms.length - 1]).toContain('for this node');
  });

  it('the list does nothing when this is not a remote vault', async () => {
    await boot(false);
    $('nodes-refresh-btn').click();
    await flush();
    expect(calls).toEqual([]);
  });
});
