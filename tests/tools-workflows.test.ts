/**
 * Tools that act on the vault: bulk selection and bundling, the side-by-side
 * secret diff, the expiry calendar and the migration preview/apply.
 *
 * What matters to a person here is that nothing is changed by surprise (a
 * bundle asks about the rest of its pool, a migration cannot be applied over a
 * conflict) and that nothing secret is displayed that did not need to be (the
 * diff masks key and secret).
 */
import { describe, it, expect, beforeAll, beforeEach, afterEach, vi } from 'vitest';
import { st, resetViewState } from '../src/ts/state';
import { loadRealIndexHtml, makeEntry, makeVault, resetState } from './helpers';

const toasts: [string, string?][] = [];
let confirmAnswer = true;
let promptAnswer: string | null = 'Team bundle';
const confirms: string[] = [];
vi.mock('../src/ts/utils', async (importOriginal) => {
  const real = await importOriginal<typeof import('../src/ts/utils')>();
  return {
    ...real,
    showToast: (m: string, k?: string) => toasts.push([m, k]),
    showConfirm: async (m: string) => {
      confirms.push(m);
      return confirmAnswer;
    },
    showPrompt: async () => promptAnswer,
  };
});

const $ = <T extends HTMLElement = HTMLElement>(id: string) => document.getElementById(id) as T;
const flush = (ms = 30) => new Promise((r) => setTimeout(r, ms));
let boundNodes: ChildNode[] = [];

beforeAll(async () => {
  loadRealIndexHtml();
  (await import('../src/ts/tools-markup')).mountToolsPanes();
  (await import('../src/ts/tools')).initTools();
  boundNodes = Array.from(document.body.childNodes);
});

beforeEach(() => {
  document.body.innerHTML = '';
  for (const n of boundNodes) document.body.appendChild(n);
  resetState(st);
  resetViewState();
  toasts.length = 0;
  confirms.length = 0;
  confirmAnswer = true;
  promptAnswer = 'Team bundle';
  delete (window as any).__TAURI__;
  st.store = { save: async () => {}, isRemote: false } as never;
});

const vault = (entries: object[]) => {
  st.vault = makeVault({ api_keys: entries.map((o) => makeEntry(o as never)) });
  st.vaultOpen = true;
};

describe('bulk selection mode', () => {
  it('Select enters the mode, shows the bar and a running count; Exit Select leaves it', () => {
    vault([{ id: 'a', provider: 'A' }]);
    $('bulk-select-btn').click();
    expect(st.bulkMode).toBe(true);
    expect($('bulk-select-btn').textContent).toBe('Exit Select');
    expect($('bulk-bar').classList.contains('active')).toBe(true);
    expect($('bulk-count').textContent).toBe('0 selected');
    expect(window.__unvBulkToggle?.(0)).toBe(true);
    expect($('bulk-count').textContent).toBe('1 selected');
    window.__unvBulkToggle?.(0);
    expect($('bulk-count').textContent).toBe('0 selected');
    $('bulk-select-btn').click();
    expect(st.bulkMode).toBe(false);
    expect($('bulk-select-btn').textContent).toBe('Select');
    expect($('bulk-bar').classList.contains('active')).toBe(false);
  });

  it('Cancel leaves the mode and forgets the selection', () => {
    vault([{ id: 'a', provider: 'A' }]);
    $('bulk-select-btn').click();
    window.__unvBulkToggle?.(0);
    $('bulk-cancel-btn').click();
    expect(st.bulkMode).toBe(false);
    expect(st.bulkSelected.size).toBe(0);
  });

  it('toggling does nothing outside the mode, and ignores a position that no longer exists', () => {
    vault([{ id: 'a', provider: 'A' }]);
    expect(window.__unvBulkToggle?.(0)).toBe(false);
    $('bulk-select-btn').click();
    expect(window.__unvBulkToggle?.(5)).toBe(true);
    expect(st.bulkSelected.size).toBe(0);
    expect(window.__unvIsBulkMode?.()).toBe(true);
  });

  it('exporting with nothing ticked says so; exported variable names are upper-cased and safe', async () => {
    vault([{ id: 'a', provider: 'My-API (eu)', api_key: 'sk-1' }]);
    $('bulk-select-btn').click();
    $('bulk-export-btn').click();
    await flush();
    expect(toasts).toContainEqual(['Nothing selected', 'err']);
    window.__unvBulkToggle?.(0);
    (URL as any).createObjectURL = vi.fn(() => 'blob:x');
    const click = vi.spyOn(HTMLAnchorElement.prototype, 'click').mockImplementation(() => {});
    $('bulk-export-btn').click();
    await flush();
    const text = await ((URL as any).createObjectURL.mock.calls[0][0] as Blob).text();
    expect(text).toBe('MY_API__EU_=sk-1');
    expect(confirms[0]).toContain('Write 1 secret to an unencrypted export.env file?');
    click.mockRestore();
  });
});

describe('bundling the selection', () => {
  const select = (...ids: string[]) => {
    $('bulk-select-btn').click();
    ids.forEach((id) => st.bulkSelected.add(id));
  };
  const bundle = async () => {
    $('bulk-bundle-btn').click();
    await flush();
  };

  it('needs at least two unbundled entries', async () => {
    vault([
      { id: 'a', provider: 'A' },
      { id: 'b', provider: 'B' },
    ]);
    select('a');
    await bundle();
    expect(toasts).toContainEqual(['Select at least two unbundled entries', 'err']);
    expect(st.vault.api_keys).toHaveLength(2);
  });

  it('bundles the selection under the given name, giving each member a slot and an order', async () => {
    vault([
      { id: 'a', provider: 'Bot' },
      { id: 'b', provider: 'Webhook' },
      { id: 'c', provider: 'Other' },
    ]);
    select('a', 'b');
    await bundle();
    const b = st.vault.api_keys.find((e) => e.secretType === 'bundle')!;
    expect(b.provider).toBe('Team bundle');
    const members = st.vault.api_keys.filter((e) => e.bundle_id === b.id);
    expect(members.map((m) => [m.provider, m.bundle_slot, m.bundle_order])).toEqual([
      ['Bot', 'Bot', 10],
      ['Webhook', 'Webhook', 20],
    ]);
    expect(st.vault.api_keys.find((e) => e.id === 'c')!.bundle_id).toBeUndefined();
    expect(toasts).toContainEqual(['Bundled 2 entries', 'ok']);
    expect(st.bulkMode).toBe(false);
  });

  it('a cancelled name creates nothing', async () => {
    vault([
      { id: 'a', provider: 'A' },
      { id: 'b', provider: 'B' },
    ]);
    promptAnswer = '  ';
    select('a', 'b');
    await bundle();
    expect(st.vault.api_keys.some((e) => e.secretType === 'bundle')).toBe(false);
  });

  it('entries already in a bundle, and bundles themselves, are not bundled again', async () => {
    vault([
      { id: 'x', provider: 'Already', bundle_id: 'old' },
      { id: 'y', provider: 'Bun', secretType: 'bundle' },
      { id: 'z', provider: 'Free' },
    ]);
    select('x', 'y', 'z');
    await bundle();
    expect(toasts).toContainEqual(['Select at least two unbundled entries', 'err']);
  });

  it('a pool is taken whole or not at all: it asks, includes the rest on yes, and stops on no', async () => {
    vault([
      { id: 'a', provider: 'A', pool: 'main' },
      { id: 'b', provider: 'B', pool: 'main' },
      { id: 'c', provider: 'C', pool: 'main' },
      { id: 'd', provider: 'D' },
    ]);
    confirmAnswer = false;
    select('a', 'd');
    await bundle();
    expect(confirms[0]).toContain('Include all 3 entries in pool "main"?');
    expect(st.vault.api_keys.some((e) => e.secretType === 'bundle')).toBe(false);
    confirmAnswer = true;
    await bundle();
    const b = st.vault.api_keys.find((e) => e.secretType === 'bundle')!;
    expect(st.vault.api_keys.filter((e) => e.bundle_id === b.id).map((e) => e.id)).toEqual([
      'a',
      'b',
      'c',
      'd',
    ]);
  });

  it('a pool that already straddles a bundle cannot be bundled', async () => {
    vault([
      { id: 'a', provider: 'A', pool: 'main' },
      { id: 'b', provider: 'B', pool: 'main', bundle_id: 'old' },
      { id: 'd', provider: 'D' },
    ]);
    select('a', 'd');
    await bundle();
    expect(toasts).toContainEqual(['Pool "main" already crosses a bundle boundary', 'err']);
  });
});

describe('secret diff', () => {
  const open = () => {
    document.querySelector<HTMLElement>('.tool-nav-btn[data-tool="diff"]')!.click();
  };
  const compare = (a: string, b: string) => {
    $<HTMLSelectElement>('diff-a').value = a;
    $<HTMLSelectElement>('diff-b').value = b;
    $('diff-run').click();
  };
  const rows = () =>
    Object.fromEntries(
      [...$('diff-output').querySelectorAll('tbody tr')].map((r) => [
        r.children[0].textContent,
        [r.children[1].textContent, r.children[2].textContent, r.className],
      ]),
    );

  it('lists every entry by name and account, and compares two of them, marking what differs', () => {
    vault([
      {
        id: 'a',
        provider: 'Alpha',
        account_name: 'me',
        api_key: 'AAAA-secret-1',
        environment: 'production',
        price_type: 'paid',
      },
      {
        id: 'b',
        provider: 'Beta',
        api_key: 'BBBB-secret-2',
        environment: 'production',
        price_type: 'free',
      },
    ]);
    open();
    const opts = [...$<HTMLSelectElement>('diff-a').options].map((o) => o.textContent);
    expect(opts).toEqual(['Select secret…', 'Alpha / me', 'Beta']);
    compare('a', 'b');
    const r = rows();
    expect(r.Provider).toEqual(['Alpha', 'Beta', 'diff-changed']);
    expect(r.Environment).toEqual(['production', 'production', 'diff-same']);
    expect(r.Price).toEqual(['paid', 'free', 'diff-changed']);
    expect(r.Account[0]).toBe('me');
    expect(r.Account[1]).toBe('—');
  });

  it('never shows the key or secret itself, only that they differ', () => {
    vault([
      { id: 'a', provider: 'Alpha', api_key: 'AAAA-secret-1', api_secret: 'S-ONE' },
      { id: 'b', provider: 'Beta', api_key: 'BBBB-secret-2' },
    ]);
    open();
    compare('a', 'b');
    const text = $('diff-output').textContent!;
    expect(text).not.toContain('AAAA-secret-1');
    expect(text).not.toContain('BBBB-secret-2');
    expect(text).not.toContain('S-ONE');
    const r = rows();
    expect(r['Key (masked)']).toEqual(['••••••••', '••••••••', 'diff-changed']);
    expect(r.Secret).toEqual(['••••••••', '—', 'diff-changed']);
  });

  it('two entries with the same key still compare equal without revealing it', () => {
    vault([
      { id: 'a', provider: 'Alpha', api_key: 'same-secret-value' },
      { id: 'b', provider: 'Alpha2', api_key: 'same-secret-value' },
    ]);
    open();
    compare('a', 'b');
    expect(rows()['Key (masked)'][2]).toBe('diff-same');
  });

  it('wants two different entries', () => {
    vault([
      { id: 'a', provider: 'Alpha' },
      { id: 'b', provider: 'Beta' },
    ]);
    open();
    for (const [a, b] of [
      ['', ''],
      ['a', ''],
      ['a', 'a'],
    ]) {
      toasts.length = 0;
      compare(a, b);
      expect(toasts, `${a}/${b}`).toContainEqual(['Select two different entries', 'err']);
    }
  });

  it('an entry deleted since the list was built is reported and the list refreshed', () => {
    vault([
      { id: 'a', provider: 'Alpha' },
      { id: 'b', provider: 'Beta' },
    ]);
    open();
    $<HTMLSelectElement>('diff-a').value = 'a';
    $<HTMLSelectElement>('diff-b').value = 'b';
    st.vault.api_keys = st.vault.api_keys.filter((e) => e.id !== 'b');
    $('diff-run').click();
    expect(toasts).toContainEqual(['That secret no longer exists — reopen the Diff tool', 'err']);
    expect([...$<HTMLSelectElement>('diff-a').options].map((o) => o.textContent)).toEqual([
      'Select secret…',
      'Alpha',
    ]);
  });

  it('escapes entry names in the headings and the list', () => {
    vault([
      { id: 'a', provider: '<img src=x onerror=1>' },
      { id: 'b', provider: 'Beta' },
    ]);
    open();
    compare('a', 'b');
    expect($('diff-output').querySelector('img')).toBeNull();
    expect($('diff-a').querySelector('img')).toBeNull();
  });
});

describe('expiry calendar', () => {
  const day = (n: number) => new Date(Date.now() + n * 86_400_000);
  const iso = (d: Date) => d.toISOString().slice(0, 10);

  beforeEach(() => {
    vi.useFakeTimers({ toFake: ['setTimeout', 'Date'] });
    vi.setSystemTime(new Date(2026, 5, 15, 12));
  });
  afterEach(() => vi.useRealTimers());

  const show = async () => {
    document.querySelector<HTMLElement>('.tool-nav-btn[data-tool="expiry-calendar"]')!.click();
    await vi.advanceTimersByTimeAsync(80);
    // The calendar remembers the month it was last showing; start each test from June 2026.
    const months = [
      'January',
      'February',
      'March',
      'April',
      'May',
      'June',
      'July',
      'August',
      'September',
      'October',
      'November',
      'December',
    ];
    const here = () => {
      const [m, y] = $('cal-month-label').textContent!.split(' ');
      return Number(y) * 12 + months.indexOf(m);
    };
    const target = 2026 * 12 + 5;
    while (here() !== target) $(here() < target ? 'cal-next' : 'cal-prev').click();
  };
  const cell = (n: number) =>
    [...document.querySelectorAll<HTMLElement>('#cal-grid .cal-cell:not(.empty)')][n - 1];

  it('labels the month, lays out its days from the right weekday, and marks today', async () => {
    vault([]);
    await show();
    expect($('cal-month-label').textContent).toMatch(/June 2026/);
    expect(document.querySelectorAll('#cal-grid .cal-cell:not(.empty)')).toHaveLength(30);
    expect(document.querySelectorAll('#cal-grid .cal-cell.empty')).toHaveLength(
      new Date(2026, 5, 1).getDay(),
    );
    expect(cell(15).classList.contains('cal-today')).toBe(true);
  });

  it('colours a day by what expires on it: past, within 30 days, later', async () => {
    vault([
      { id: 'a', provider: 'Past', expires_at: '2026-06-03' },
      { id: 'b', provider: 'Soon', expires_at: iso(day(5)) },
      { id: 'c', provider: 'LaterInJune', expires_at: '2026-06-30T10:00:00Z' },
      { id: 'd', provider: 'Elsewhere', expires_at: '2026-08-20' },
      { id: 'e', provider: 'Never' },
    ]);
    await show();
    expect(cell(3).classList.contains('cal-expired')).toBe(true);
    expect(cell(20).classList.contains('cal-warn')).toBe(true);
    expect(cell(30).classList.contains('cal-warn')).toBe(true);
    expect(cell(3).title).toBe('Past');
    expect(
      document.querySelectorAll('#cal-grid .cal-expired, #cal-grid .cal-warn, #cal-grid .cal-safe'),
    ).toHaveLength(3);
    // More than thirty days out is calm.
    $('cal-next').click();
    $('cal-next').click();
    expect(cell(20).classList.contains('cal-safe')).toBe(true);
  });

  it('several entries on one day are listed in its tooltip, with dots capped and the rest counted', async () => {
    vault(
      Array.from({ length: 7 }, (_, i) => ({
        id: `e${i}`,
        provider: `P${i}`,
        expires_at: '2026-06-20',
      })),
    );
    await show();
    const c = cell(20);
    expect(c.title).toBe('P0, P1, P2, P3, P4 +2');
    expect(c.querySelectorAll('.cal-entry-dot')).toHaveLength(4);
    expect(c.querySelector('.cal-extra')!.textContent).toBe('+3');
  });

  it('previous and next step through months and across the year boundary', async () => {
    vault([]);
    await show();
    for (let i = 0; i < 6; i++) $('cal-next').click();
    expect($('cal-month-label').textContent).toMatch(/December 2026/);
    $('cal-next').click();
    expect($('cal-month-label').textContent).toMatch(/January 2027/);
    $('cal-prev').click();
    $('cal-prev').click();
    expect($('cal-month-label').textContent).toMatch(/November 2026/);
    for (let i = 0; i < 11; i++) $('cal-prev').click();
    expect($('cal-month-label').textContent).toMatch(/December 2025/);
  });
});

describe('migration', () => {
  beforeEach(async () => {
    vi.resetModules();
    loadRealIndexHtml();
    (await import('../src/ts/tools-markup')).mountToolsPanes();
  });

  const start = async () => (await import('../src/ts/tools')).initTools();

  const fill = (
    over: Partial<Record<'migrate-format' | 'migrate-source-label' | 'migrate-text', string>> = {},
  ) => {
    $<HTMLSelectElement>('migrate-format').value =
      over['migrate-format'] ?? $<HTMLSelectElement>('migrate-format').options[0].value;
    $<HTMLInputElement>('migrate-source-label').value =
      over['migrate-source-label'] ?? 'old-manager';
    $<HTMLTextAreaElement>('migrate-text').value = over['migrate-text'] ?? 'some export';
  };
  const tauri = (handler: (cmd: string, args: any) => unknown) => {
    (window as any).__TAURI__ = { core: { invoke: async (c: string, a: any) => handler(c, a) } };
  };
  const plan = (conflicts: number) => ({
    format: 'x',
    counts: { read: 3, imported: 2, archived: 0, skipped: 1, conflicts },
    items: [
      { name: 'Item <b>', source_id: '1', status: 'ready' },
      { name: 'Dup', source_id: '2', status: 'conflict', conflict: 'exists' },
    ],
    unsupported: ['attachments'],
  });
  const preview = async () => {
    $('migrate-preview').click();
    await flush();
  };

  it('asks for a format, a source name and text before previewing', async () => {
    await start();
    fill({ 'migrate-source-label': '' });
    await preview();
    expect($('migrate-status').textContent).toBe(
      'Choose a format, name the source, and paste configuration text.',
    );
    expect($('migrate-status').className).toContain('err');
  });

  it('says previewing needs the desktop app', async () => {
    await start();
    fill();
    await preview();
    expect($('migrate-status').textContent).toBe(
      'Migration preview is available in the desktop app.',
    );
    expect($('migrate-status').className).toContain('warn');
  });

  it('previews counts, lists each item (escaped) and anything that will only be archived', async () => {
    tauri(() => plan(0));
    await start();
    fill();
    await preview();
    expect($('migrate-status').textContent).toBe('3 item(s): 2 ready, 1 skipped, 0 conflict(s).');
    expect($('migrate-results').textContent).toContain('Dup');
    expect($('migrate-results').textContent).toContain('conflict: exists');
    expect($('migrate-results').textContent).toContain('Archived with this migration: attachments');
    expect($('migrate-results').querySelector('b')).toBeNull();
  });

  it('can be applied only after a clean preview, and any edit to the input withdraws that', async () => {
    tauri(() => plan(0));
    await start();
    fill();
    expect($<HTMLButtonElement>('migrate-apply').disabled).toBe(true);
    await preview();
    expect($<HTMLButtonElement>('migrate-apply').disabled).toBe(false);
    $('migrate-text').dispatchEvent(new Event('input'));
    expect($<HTMLButtonElement>('migrate-apply').disabled).toBe(true);
    await preview();
    $('migrate-format').dispatchEvent(new Event('change'));
    expect($<HTMLButtonElement>('migrate-apply').disabled).toBe(true);
  });

  it('a conflict keeps Apply disabled', async () => {
    tauri(() => plan(1));
    await start();
    fill();
    await preview();
    expect($<HTMLButtonElement>('migrate-apply').disabled).toBe(true);
    expect($('migrate-status').textContent).toContain('1 conflict(s)');
  });

  it('applying reports what was imported, and cannot be repeated', async () => {
    const calls: string[] = [];
    tauri((c) => {
      calls.push(c);
      return c === 'migrate_plan'
        ? plan(0)
        : { counts: { imported: 2, archived: 1, skipped: 0, conflicts: 0 } };
    });
    await start();
    fill();
    await preview();
    $('migrate-apply').click();
    await flush();
    expect($('migrate-status').textContent).toBe(
      'Migration applied: 2 imported, 1 archived, 0 skipped.',
    );
    $('migrate-apply').click();
    await flush();
    expect(calls.filter((c) => c === 'migrate_apply')).toHaveLength(1);
  });

  it('shows the reason when preview or apply fails', async () => {
    tauri(() => {
      throw new Error('unreadable export');
    });
    await start();
    fill();
    await preview();
    expect($('migrate-status').textContent).toBe('unreadable export');
    expect($('migrate-status').className).toContain('err');
    tauri((c) => {
      if (c === 'migrate_plan') return plan(0);
      throw new Error('disk full');
    });
    await preview();
    $('migrate-apply').click();
    await flush();
    expect($('migrate-status').textContent).toBe('disk full');
  });
});
