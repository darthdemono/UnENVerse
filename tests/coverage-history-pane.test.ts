// SPDX-License-Identifier: PolyForm-Strict-1.0.0
// Copyright Jubair Hasan.
/** Branch coverage for history-pane.ts: error shapes, every button, restore and save paths. */
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { loadRealIndexHtml, resetState } from './helpers';

let confirmAnswer = true;
let saveResult: any = { ok: true, path: '/dl/f' };
vi.mock('../src/ts/utils', async (importOriginal) => {
  const real = await importOriginal<typeof import('../src/ts/utils')>();
  return {
    ...real,
    showConfirm: async () => confirmAnswer,
    saveFile: async () => saveResult,
  };
});
vi.mock('../src/ts/tauri', () => ({
  isTauri: () => false,
  inTauri: false,
  invokeTauri: async () => ({}),
}));

const $ = (id: string) => document.getElementById(id)!;
const flush = (ms = 20) => new Promise((r) => setTimeout(r, ms));
const NGINX = 'server {\n  listen 80;\n  server_name a.test;\n}\n';

type Reply = { status: number; body: unknown } | Error | string;
let handler: (op: string, args: any) => Reply = () => ({ status: 200, body: {} });
const asked: [string, any][] = [];

async function boot(projects: any[] | null = [{ id: 'p1', name: 'edge', chunks: [] }]) {
  loadRealIndexHtml();
  const { RemoteVaultStore, st } = await import('../src/ts/state');
  (await import('../src/ts/tools-markup')).mountToolsPanes();
  resetState(st);
  st.vault = (
    projects === null ? undefined : { api_keys: [], user_categories: [], projects }
  ) as never;
  const remote = new RemoteVaultStore('http://localhost:1');
  remote.nodesRequest = async (_m, _p, b) => {
    const { op, args } = b as { op: string; args: any };
    asked.push([op, args]);
    const r = handler(op, args);
    if (r instanceof Error) throw r;
    if (typeof r === 'string') throw r as unknown as Error;
    return r;
  };
  st.store = remote;
  (await import('../src/ts/history-pane')).initHistoryPane();
  return st;
}

const ok = (body: unknown): Reply => ({ status: 200, body });
const row = (over: object = {}) => ({
  seq: 1,
  project: 'p1',
  project_name: 'edge',
  exporter: 'nginx',
  at: 't',
  sha256: 'ab'.repeat(32),
  bytes: 1,
  cause: 'save',
  ...over,
});
const pickFile = (text: string | null, reject = false) =>
  vi.spyOn(HTMLInputElement.prototype, 'click').mockImplementation(function (
    this: HTMLInputElement,
  ) {
    Object.defineProperty(this, 'files', {
      value:
        text === null
          ? []
          : [{ text: () => (reject ? Promise.reject(new Error('x')) : Promise.resolve(text)) }],
    });
    this.onchange?.(new Event('change'));
  });

beforeEach(() => {
  vi.resetModules();
  vi.restoreAllMocks();
  asked.length = 0;
  confirmAnswer = true;
  saveResult = { ok: true, path: '/dl/f' };
  handler = () => ok({});
});

describe('errors', () => {
  it('shows a server message, a bare status, a thrown Error and a thrown string', async () => {
    await boot();
    handler = () => ({ status: 500, body: { error: 'disk' } });
    $('history-list-btn').click();
    await flush();
    expect($('history-status').textContent).toBe('disk');
    handler = () => ({ status: 502, body: null });
    $('history-list-btn').click();
    await flush();
    expect($('history-status').textContent).toBe('Refused (502)');
    handler = () => new Error('net');
    $('history-list-btn').click();
    await flush();
    expect($('history-status').textContent).toBe('net');
    handler = () => 'plain';
    $('history-list-btn').click();
    await flush();
    expect($('history-status').textContent).toBe('plain');
  });
});

describe('list', () => {
  it('handles a vault with no projects, no snapshots key, singular count and non-restorable rows', async () => {
    await boot(null);
    handler = () => ok({});
    $('history-list-btn').click();
    await flush();
    expect($('history-list').textContent).toContain('No snapshots yet');
    handler = () => ok({ snapshots: [row({ exporter: 'dotenv' })] });
    $('history-list-btn').click();
    await flush();
    expect($('history-status').textContent).toBe('1 snapshot');
    expect($('history-list').querySelector('[data-history-act="restore"]')).toBeNull();
  });

  it('keeps the selected project across a refill and sends the project and exporter filters', async () => {
    await boot();
    ($('history-project') as HTMLSelectElement).innerHTML = '<option value="edge">edge</option>';
    ($('history-project') as HTMLSelectElement).value = 'edge';
    ($('history-exporter') as HTMLInputElement).value = ' nginx ';
    $('history-list-btn').click();
    await flush();
    expect(asked[0][1]).toMatchObject({ project: 'edge', exporter: 'nginx' });
    expect(($('history-project') as HTMLSelectElement).value).toBe('edge');
  });
});

describe('show and diff', () => {
  it('shows an empty text, a failed show, and reveals when confirmed', async () => {
    await boot();
    handler = (op) => (op === 'list' ? ok({ snapshots: [row()] }) : ok({}));
    $('history-list-btn').click();
    await flush();
    const showBtn = () =>
      $('history-list').querySelector('[data-history-act="show"]') as HTMLElement;
    showBtn().click();
    await flush();
    expect($('history-output').textContent).toBe('');
    handler = () => ({ status: 500, body: null });
    $('history-output').textContent = 'kept';
    showBtn().click();
    await flush();
    expect($('history-output').textContent).toBe('kept');
  });

  it('diff: identical, counts, reveal marker, failure, and the toolbar button', async () => {
    await boot();
    ($('history-project') as HTMLSelectElement).innerHTML = '<option value="edge">edge</option>';
    ($('history-project') as HTMLSelectElement).value = 'edge';
    ($('history-reveal') as HTMLInputElement).checked = true;
    handler = () => ok({ from: 1, to: 2 });
    $('history-diff-btn').click();
    await flush();
    expect($('history-output').textContent).toBe('#1 and #2 render identically.');
    expect($('history-status').textContent).toBe('#1 to #2: +0 -0 (real values)');
    ($('history-reveal') as HTMLInputElement).checked = false;
    handler = () => ({ status: 500, body: null });
    $('history-output').textContent = 'kept';
    $('history-diff-btn').click();
    await flush();
    expect($('history-output').textContent).toBe('kept');
  });

  it('compare with a file: no file, unreadable file, identical, reveal and failure', async () => {
    await boot();
    ($('history-project') as HTMLSelectElement).innerHTML = '<option value="edge">edge</option>';
    ($('history-project') as HTMLSelectElement).value = 'edge';
    let spy = pickFile(null);
    $('history-diff-file-btn').click();
    await flush(40);
    expect($('history-status').textContent).toBe('No file chosen.');
    spy.mockRestore();
    spy = pickFile('x', true);
    $('history-diff-file-btn').click();
    await flush(40);
    expect($('history-status').textContent).toBe('No file chosen.');
    spy.mockRestore();

    spy = pickFile('t');
    handler = () => ok({ seq: 3, identical: true });
    $('history-diff-file-btn').click();
    await flush(40);
    expect($('history-status').textContent).toBe('#3 and the file are identical.');
    expect($('history-output').textContent).toBe('');

    ($('history-reveal') as HTMLInputElement).checked = true;
    handler = () => ok({ seq: 3 });
    $('history-diff-file-btn').click();
    await flush(40);
    expect($('history-status').textContent).toBe('#3 against the file: +0 -0 (real values)');

    handler = () => ({ status: 500, body: null });
    $('history-output').textContent = 'kept';
    $('history-diff-file-btn').click();
    await flush(40);
    expect($('history-output').textContent).toBe('kept');
    spy.mockRestore();
  });
});

describe('snapshot, verify, policy, prune', () => {
  it('snapshot records then lists; a failure does neither', async () => {
    await boot();
    handler = (op) => (op === 'snapshot' ? ok({}) : ok({ snapshots: [] }));
    $('history-snapshot-btn').click();
    await flush();
    expect(asked.map((a) => a[0])).toEqual(['snapshot', 'list']);
    asked.length = 0;
    handler = () => ({ status: 500, body: null });
    $('history-snapshot-btn').click();
    await flush();
    expect(asked.map((a) => a[0])).toEqual(['snapshot']);
  });

  it('verify: intact, and a failed call', async () => {
    await boot();
    handler = () => ok({ snapshots: 2, problems: [] });
    $('history-verify-btn').click();
    await flush();
    expect($('history-status').textContent).toBe('2 snapshot(s) intact.');
    handler = () => ({ status: 500, body: null });
    $('history-verify-btn').click();
    await flush();
  });

  it('policy: load fills the form, a failed load and a failed save change nothing', async () => {
    await boot();
    handler = () => ok({ enabled: true, keep: 7, days: 3 });
    $('history-policy-load-btn').click();
    await flush();
    expect($('history-status').textContent).toContain('History is on');
    handler = () => ok({ enabled: false, keep: 7, days: 3 });
    $('history-policy-load-btn').click();
    await flush();
    expect(($('history-keep') as HTMLInputElement).value).toBe('7');
    expect($('history-status').textContent).toContain('History is off');
    $('history-policy-btn').click();
    await flush();
    expect($('history-status').textContent).toBe('Policy saved.');
    ($('history-keep') as HTMLInputElement).value = '0';
    ($('history-days') as HTMLInputElement).value = '';
    $('history-policy-btn').click();
    await flush();
    expect(asked[asked.length - 1][1]).toMatchObject({ keep: 50, days: 0 });
    handler = () => ({ status: 500, body: null });
    $('history-policy-load-btn').click();
    $('history-policy-btn').click();
    await flush();
  });

  it('prune: failed dry run, nothing to prune, and a failed delete', async () => {
    await boot();
    handler = () => ({ status: 500, body: null });
    $('history-prune-btn').click();
    await flush();
    handler = () => ok({ would_delete: 0, bytes_freed: 0 });
    $('history-prune-btn').click();
    await flush();
    expect($('history-status').textContent).toBe('Nothing is old enough to prune.');
    handler = (_op, a) =>
      a.dry_run ? ok({ would_delete: 2, bytes_freed: 5 }) : { status: 500, body: null };
    $('history-prune-btn').click();
    await flush();
  });
});

describe('restore and save', () => {
  const run = async (projects: any[], rowOver: object, text: string | undefined, answer = true) => {
    await boot(projects);
    confirmAnswer = answer;
    handler = (op) =>
      op === 'list'
        ? ok({ snapshots: [row(rowOver)] })
        : text === undefined
          ? ok({})
          : ok({ text });
    $('history-list-btn').click();
    await flush();
    ($('history-list').querySelector('[data-history-act="restore"]') as HTMLElement).click();
    await flush();
  };

  it('a deleted project cannot be restored into', async () => {
    await run([], { project: 'gone' }, NGINX);
    expect($('history-status').textContent).toContain("'gone' no longer exists");
  });

  it('an empty snapshot does nothing', async () => {
    await run([{ id: 'p1', name: 'edge', chunks: [] }], {}, undefined);
    expect($('history-status').textContent).not.toContain('Restored');
  });

  it('nothing readable as chunks', async () => {
    await run(
      [{ id: 'p1', name: 'edge', chunks: [] }],
      { project: 'edge' },
      'garbage with no blocks',
    );
    expect($('history-status').textContent).toContain('Nothing in that snapshot could be read');
  });

  it('declining leaves the project alone; accepting replaces and persists', async () => {
    const projects = [
      {
        id: 'p1',
        name: 'edge',
        chunks: [{ id: 'k', name: 'keep', chunk_type: 'note', fields: [] }],
      },
    ];
    await run(projects, {}, NGINX, false);
    expect($('history-status').textContent).toBe('Nothing was changed.');
    expect(projects[0].chunks).toHaveLength(1);
    asked.length = 0;
    confirmAnswer = true;
    ($('history-list').querySelector('[data-history-act="restore"]') as HTMLElement).click();
    await flush(40);
    expect($('history-status').textContent).toContain("Restored snapshot #1 into 'edge'");
    expect(projects[0].chunks.length).toBeGreaterThan(1);
  });

  it('save: writes the file, reports a failure, and a failed read does nothing', async () => {
    await boot();
    handler = (op) => (op === 'list' ? ok({ snapshots: [row()] }) : ok({ text: 'T' }));
    $('history-list-btn').click();
    await flush();
    const save = () =>
      ($('history-list').querySelector('[data-history-act="save"]') as HTMLElement).click();
    save();
    await flush();
    expect($('history-status').textContent).toContain('Saved to /dl/f');
    saveResult = { ok: true };
    save();
    await flush();
    expect($('history-status').textContent).toMatch(/^Saved \(the file/);
    saveResult = { ok: false, error: 'denied' };
    save();
    await flush();
    expect($('history-status').textContent).toBe('Could not save: denied');
    handler = () => ({ status: 500, body: null });
    save();
    await flush();
  });

  it('restore tolerates a vault with no projects, a tampered exporter and missing row data', async () => {
    await boot(null);
    handler = (op) => (op === 'list' ? ok({ snapshots: [row()] }) : ok({ text: NGINX }));
    $('history-list-btn').click();
    await flush();
    const restore = () =>
      ($('history-list').querySelector('[data-history-act="restore"]') as HTMLElement).click();
    const tr = $('history-list').querySelector('tr')!;
    tr.removeAttribute('data-project');
    tr.removeAttribute('data-exporter');
    restore();
    await flush();
    expect($('history-status').textContent).toContain("'' no longer exists");
    const st = (await import('../src/ts/state')).st;
    st.vault = {
      api_keys: [],
      user_categories: [],
      projects: [{ id: 'p1', name: 'edge', chunks: [] }],
    } as never;
    tr.setAttribute('data-project', 'p1');
    tr.setAttribute('data-exporter', 'dotenv');
    restore();
    await flush();
    expect($('history-status').textContent).toContain('dotenv config cannot be read back');
  });

  it('save with an empty answer writes an empty file', async () => {
    await boot();
    handler = (op) => (op === 'list' ? ok({ snapshots: [row()] }) : ok({}));
    $('history-list-btn').click();
    await flush();
    ($('history-list').querySelector('[data-history-act="save"]') as HTMLElement).click();
    await flush();
    expect($('history-status').textContent).toContain('Saved');
  });
});
