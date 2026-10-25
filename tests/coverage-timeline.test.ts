// SPDX-License-Identifier: PolyForm-Strict-1.0.0
// Copyright Jubair Hasan.
/** Branch coverage for timeline.ts: every cell state, sort, export path and the calendar-feed section. */
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { loadRealIndexHtml, makeEntry, makeVault, resetState } from './helpers';

const toast = vi.fn();
let confirmAnswer = true;
const confirms: string[] = [];
const downloads: unknown[][] = [];
vi.mock('../src/ts/utils', async (importOriginal) => {
  const real = await importOriginal<typeof import('../src/ts/utils')>();
  return {
    ...real,
    showToast: (...a: unknown[]) => toast(...a),
    showConfirm: async (m: string) => {
      confirms.push(m);
      return confirmAnswer;
    },
    clipboardWrite: async () => true,
  };
});
vi.mock('../src/ts/import-export', async (importOriginal) => {
  const real = await importOriginal<typeof import('../src/ts/import-export')>();
  return { ...real, downloadText: (...a: unknown[]) => downloads.push(a) };
});

const $ = (id: string) => document.getElementById(id)!;
const flush = (ms = 20) => new Promise((r) => setTimeout(r, ms));
const day = (n: number) => new Date(Date.now() + n * 86_400_000).toISOString();

async function boot(
  opts: { invoke?: (cmd: string, a: any) => Promise<unknown>; remote?: any; entries?: any[] } = {},
) {
  vi.resetModules();
  loadRealIndexHtml();
  if (opts.invoke) (window as any).__TAURI__ = { core: { invoke: opts.invoke } };
  else delete (window as any).__TAURI__;
  const state = await import('../src/ts/state');
  (await import('../src/ts/tools-markup')).mountToolsPanes();
  resetState(state.st);
  state.st.vault = makeVault({ api_keys: opts.entries ?? [] });
  if (opts.remote) {
    const r = new state.RemoteVaultStore('http://localhost:1');
    Object.assign(r, opts.remote);
    state.st.store = r;
  }
  const tl = await import('../src/ts/timeline');
  tl.initTimelinePane();
  tl.initTimelinePane();
  return { tl, st: state.st };
}

beforeEach(() => {
  toast.mockClear();
  confirms.length = 0;
  downloads.length = 0;
  confirmAnswer = true;
});

describe('cells', () => {
  it('rotation due: no cadence, no anchor, bad anchor, created anchor', async () => {
    const { tl } = await boot();
    expect(tl.rotationDue(makeEntry({ rotation_days: 0 } as any))).toBeNull();
    expect(tl.rotationDue(makeEntry({ rotation_days: 10 } as any))).toBeNull();
    expect(tl.rotationDue(makeEntry({ rotation_days: 10, created_at: 'nope' } as any))).toBeNull();
    expect(
      tl.rotationDue(makeEntry({ rotation_days: 10, created_at: '2026-01-01T00:00:00Z' } as any)),
    ).toBe('2026-01-11T00:00:00.000Z');
    expect(
      tl.rotationDue(
        makeEntry({
          rotation_days: 10,
          created_at: '2026-01-01T00:00:00Z',
          last_rotated_at: '2026-02-01T00:00:00Z',
        } as any),
      ),
    ).toBe('2026-02-11T00:00:00.000Z');
  });

  it('created cell: known, bound from history, unknown', async () => {
    const { tl } = await boot();
    expect(tl.createdCell(makeEntry({ created_at: '2026-03-04T00:00:00Z' } as any)).certainty).toBe(
      'known',
    );
    expect(
      tl.createdCell(
        makeEntry({
          created_at: 'bad',
          version_history: [{ saved_at: '2026-01-01T00:00:00Z' }],
        } as any),
      ).certainty,
    ).toBe('bound');
    expect(tl.createdCell(makeEntry({} as any))).toEqual({ text: 'unknown', certainty: 'unknown' });
  });

  it('sorting keeps undated entries last every way', async () => {
    const { tl } = await boot();
    const a = makeEntry({
      provider: 'B',
      created_at: '2026-01-01T00:00:00Z',
      expires_at: '2027-01-01',
    } as any);
    const b = makeEntry({ provider: 'A', created_at: '2026-02-01T00:00:00Z' } as any);
    const c = makeEntry({
      provider: 'C',
      version_history: [{ saved_at: '2025-01-01T00:00:00Z' }],
    } as any);
    const d = makeEntry({ provider: 'D' } as any);
    const names = (s: any) => tl.sortEntries([d, a, b, c], s).map((e) => e.provider);
    expect(names('created-asc')).toEqual(['C', 'B', 'A', 'D']);
    expect(names('created-desc')).toEqual(['A', 'B', 'C', 'D']);
    expect(names('expires-asc')[0]).toBe('B');
    expect(names('provider')).toEqual(['A', 'B', 'C', 'D']);
    expect(names('bogus')[3]).toBe('D');
    expect(tl.sortEntries([d, makeEntry({ provider: 'E' } as any)], 'created-asc')).toHaveLength(2);
    expect(tl.sortEntries([d, a], 'expires-asc')[1]).toBe(d);
    expect(tl.sortEntries([a, d], 'expires-asc')[1]).toBe(d);
  });
});

describe('rendering', () => {
  it('renders every expiry and rotation state, and an empty vault', async () => {
    const entries = [
      makeEntry({ provider: 'Expired', expires_at: day(-3) } as any),
      makeEntry({
        provider: 'Soon',
        expires_at: day(10),
        key_id: 'k',
        account_name: 'acct',
      } as any),
      makeEntry({ provider: 'Far', expires_at: day(400), created_at: day(-5) } as any),
      makeEntry({ provider: 'Bad', expires_at: 'junk' } as any),
      makeEntry({ provider: 'Over', rotation_days: 1, created_at: day(-10) } as any),
      makeEntry({ provider: 'DueSoon', rotation_days: 20, created_at: day(-10) } as any),
      makeEntry({ provider: 'DueFar', rotation_days: 400, created_at: day(-10) } as any),
    ];
    const { tl } = await boot({ entries });
    tl.renderTimeline();
    const text = $('tl-rows').textContent!;
    expect(text).toContain('(expired)');
    expect(text).toContain('Soon · k · acct');
    expect(text).toContain('(overdue)');
    expect($('tl-summary').textContent).toContain('oldest dated');
    const empty = await boot();
    empty.tl.renderTimeline();
    expect($('tl-rows').textContent).toContain('No secrets in this vault yet.');
    expect($('tl-summary').textContent).not.toContain('oldest');
  });

  it('the sort control and refresh button repaint', async () => {
    const { tl } = await boot({
      entries: [makeEntry({ provider: 'Z' } as any), makeEntry({ provider: 'A' } as any)],
    });
    tl.renderTimeline();
    const sel = $('tl-sort') as HTMLSelectElement;
    sel.value = 'provider';
    sel.onchange!(new Event('change'));
    expect($('tl-rows').querySelector('td')!.textContent).toBe('A');
    sel.value = '';
    sel.onchange!(new Event('change'));
    $('tl-refresh').click();
  });
});

describe('calendar export', () => {
  const ics = (n: number) => 'BEGIN:VEVENT\n'.repeat(n);

  it('needs a kind, the desktop app and a non-empty answer', async () => {
    const plain = await boot();
    await plain.tl.exportCalendar();
    expect(toast).toHaveBeenCalledWith(
      expect.stringContaining('needs the desktop app'),
      'err',
      4000,
    );
    for (const k of ['created', 'expires', 'rotation'])
      ($(`tl-kind-${k}`) as HTMLInputElement).checked = false;
    await plain.tl.exportCalendar();
    expect(toast).toHaveBeenCalledWith('Pick at least one kind of event', 'err');

    const bad = await boot({ invoke: async () => undefined });
    await bad.tl.exportCalendar();
    expect(toast).toHaveBeenCalledWith('Could not build the calendar', 'err');
    const none = await boot({ invoke: async () => ics(0) });
    await none.tl.exportCalendar();
    expect(toast).toHaveBeenCalledWith(expect.stringContaining('Nothing to export'), 'err', 3500);
  });

  it('confirms with singular and plural counts and downloads only when accepted', async () => {
    await boot({ invoke: async () => ics(1), entries: [makeEntry({} as any)] });
    $('tl-export-ics').click();
    await flush();
    expect(confirms[0]).toContain('Export 1 calendar event?');
    expect(downloads[0][1]).toBe('unenverse-secrets.ics');
    confirmAnswer = false;
    const many = await boot({ invoke: async () => ics(3) });
    await many.tl.exportCalendar();
    expect(confirms[1]).toContain('Export 3 calendar events?');
    expect(downloads).toHaveLength(1);
  });
});

describe('calendar feeds', () => {
  const feeds = [
    { id: 1, name: 'Mine', kinds: ['created'], revoked_at: null },
    { id: 2, name: '', kinds: undefined, revoked_at: '2026-01-01' },
  ];

  it('hides the section locally and lists feeds against a remote', async () => {
    const local = await boot();
    local.tl.renderTimeline();
    expect(($('tl-feeds-section') as HTMLElement).style.display).toBe('none');
    const revoke = vi.fn(async () => true);
    const { tl } = await boot({
      remote: { listCalendarFeeds: async () => feeds, revokeCalendarFeed: revoke },
    });
    tl.renderTimeline();
    await flush();
    expect($('tl-feed-list').textContent).toContain('Mine — created');
    expect($('tl-feed-list').textContent).toContain('Untitled');
    expect($('tl-feed-list').textContent).toContain('revoked');
    ($('tl-feed-list').querySelector('[data-action="tl-feed-revoke"]') as HTMLElement).click();
    await flush();
    expect(revoke).toHaveBeenCalledWith('1');
    expect(toast).toHaveBeenCalledWith('Feed revoked', 'ok');
  });

  it('shows an empty list, a declined or failed revoke', async () => {
    let list: any[] = [];
    const revoke = vi.fn(async () => false);
    const { tl } = await boot({
      remote: { listCalendarFeeds: async () => list, revokeCalendarFeed: revoke },
    });
    tl.renderTimeline();
    await flush();
    expect($('tl-feed-list').textContent).toContain('No feeds yet.');
    list = [feeds[0]];
    tl.renderTimeline();
    await flush();
    const btn = () =>
      $('tl-feed-list').querySelector('[data-action="tl-feed-revoke"]') as HTMLElement;
    confirmAnswer = false;
    btn().click();
    await flush();
    expect(revoke).not.toHaveBeenCalled();
    confirmAnswer = true;
    btn().click();
    await flush();
    expect(toast).toHaveBeenCalledWith('Could not revoke feed', 'err');
    btn().dataset.id = '';
    btn().click();
    await flush();
  });

  it('subscribing needs a remote, a kind and a url', async () => {
    const local = await boot();
    $('tl-feed-subscribe').click();
    await flush();
    expect(toast).not.toHaveBeenCalled();
    let url: string | null = null;
    const create = vi.fn(async () => url);
    const { tl } = await boot({
      remote: { listCalendarFeeds: async () => [], createCalendarFeed: create },
    });
    tl.renderTimeline();
    $('tl-feed-subscribe').click();
    await flush();
    expect(toast).toHaveBeenCalledWith('Could not create feed', 'err');
    url = 'https://feed/abc';
    $('tl-feed-account-names').remove();
    $('tl-feed-subscribe').click();
    await flush();
    expect(toast).toHaveBeenCalledWith(expect.stringContaining('Feed URL copied'), 'ok', 5000);
    for (const k of ['created', 'expires', 'rotation'])
      ($(`tl-kind-${k}`) as HTMLInputElement).checked = false;
    $('tl-feed-subscribe').click();
    await flush();
    expect(toast).toHaveBeenCalledWith('Pick at least one kind of event', 'err');
    void local;
  });
});
