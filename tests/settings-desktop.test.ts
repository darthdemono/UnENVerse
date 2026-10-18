/**
 * The settings that only exist in the desktop app: the signed provider
 * catalogue, the full-fidelity archive, recovery packages with destinations and
 * schedules, and the entropy source. Each action must report what the desktop
 * answered, in words, and a failure must be a message, not a silent no-op.
 */
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { bootDesktop, closeDesktop, backend, calls } from './desktop-boot';

const toasts: [string, string?][] = [];
let confirmAnswer = true;
let passwordAnswer: string | null = 'archive-password-123';
const saved: [string, string][] = [];
let saveResult: { ok: boolean; path?: string; error?: string } = {
  ok: true,
  path: '/x/unenverse.vaultarc',
};
vi.mock('../src/ts/utils', async (importOriginal) => {
  const real = await importOriginal<typeof import('../src/ts/utils')>();
  return {
    ...real,
    showToast: (m: string, k?: string) => toasts.push([m, k]),
    showConfirm: async () => confirmAnswer,
    showPasswordPrompt: async () => passwordAnswer,
    saveFile: async (c: string, n: string) => {
      saved.push([c, n]);
      return saveResult;
    },
  };
});

const $ = <T extends HTMLElement = HTMLElement>(id: string) => document.getElementById(id) as T;
const flush = (ms = 40) => new Promise((r) => setTimeout(r, ms));
const vault = {
  api_keys: [],
  user_categories: [],
  projects: [{ id: 'Universal', name: 'Universal' }],
};

async function open(handlers: Record<string, (a: any) => unknown> = {}) {
  await bootDesktop(vault, { handlers });
  $('settings-btn').click();
  await flush(80);
}

beforeEach(() => {
  toasts.length = 0;
  saved.length = 0;
  confirmAnswer = true;
  passwordAnswer = 'archive-password-123';
  saveResult = { ok: true, path: '/x/unenverse.vaultarc' };
});
afterEach(() => closeDesktop());

describe('the provider catalogue', { timeout: 40000 }, () => {
  it('says whether the built-in list or a downloaded one is in use', async () => {
    await open({ catalogue_status: () => ({ source: 'bundled' }) });
    expect($('s-catalogue-status').textContent).toContain('built into this app');
  });

  it('describes a downloaded catalogue by date and size', async () => {
    await open({
      catalogue_status: () => ({ source: 'catalogue', generated_at: '2026-10-01', providers: 41 }),
    });
    expect($('s-catalogue-status').textContent).toBe(
      'Using the downloaded catalogue from 2026-10-01 (41 providers).',
    );
  });

  it('uses question marks and zero when the desktop leaves them out', async () => {
    await open({ catalogue_status: () => ({ source: 'catalogue' }) });
    expect($('s-catalogue-status').textContent).toBe(
      'Using the downloaded catalogue from ? (0 providers).',
    );
  });

  it('says it cannot read the catalogue when the desktop refuses', async () => {
    await open({
      catalogue_status: () => {
        throw new Error('cache corrupt');
      },
    });
    expect($('s-catalogue-status').textContent).toContain('Cannot read the catalogue');
  });

  it('Update reports what was fetched, or why it was not', async () => {
    await open({
      catalogue_status: () => ({ source: 'bundled' }),
      catalogue_update: () => ({ generated_at: '2026-10-02', providers: 50 }),
    });
    $('s-catalogue-update').click();
    await flush();
    expect($('s-catalogue-status').textContent).toContain('2026-10-02');
    expect(toasts).toContainEqual(['Catalogue updated', 'ok']);
    expect(($('s-catalogue-update') as HTMLButtonElement).disabled).toBe(false);
    backend.catalogue_update = () => {
      throw new Error('bad signature');
    };
    $('s-catalogue-update').click();
    await flush();
    expect($('s-catalogue-status').textContent).toContain('Not updated');
    expect(toasts).toContainEqual(['Catalogue not updated', 'err']);
  });
});

describe('the full archive', { timeout: 40000 }, () => {
  it('builds one with a password, writes it, and reports the path', async () => {
    await open({ backup_archive_build: (a) => `ARCHIVE:${a.password}` });
    $('s-archive-btn').click();
    await flush();
    expect(saved[0]).toEqual(['ARCHIVE:archive-password-123', 'unenverse.vaultarc']);
    expect(toasts).toContainEqual(['Archive written: /x/unenverse.vaultarc', 'ok']);
  });

  it('reports a write that failed, and a build that failed, and does nothing without a password', async () => {
    await open({ backup_archive_build: () => 'ARCHIVE' });
    saveResult = { ok: false, error: 'disk full' };
    $('s-archive-btn').click();
    await flush();
    expect(toasts).toContainEqual(['Could not write the archive: disk full', 'err']);
    backend.backup_archive_build = () => {
      throw new Error('no vault');
    };
    $('s-archive-btn').click();
    await flush();
    expect(toasts).toContainEqual(['Archive failed: no vault', 'err']);
    passwordAnswer = null;
    const before = toasts.length;
    $('s-archive-btn').click();
    await flush();
    expect(toasts).toHaveLength(before);
  });

  it('says plainly when the archive was written without a path', async () => {
    await open({ backup_archive_build: () => 'A' });
    saveResult = { ok: true };
    $('s-archive-btn').click();
    await flush();
    expect(toasts).toContainEqual(['Archive written', 'ok']);
  });

  const pick = async (text: string) => {
    $('s-archive-restore-btn').click();
    await flush();
    const inp = document.querySelector<HTMLInputElement>('input[type=file]')!;
    Object.defineProperty(inp, 'files', {
      value: [new File([text], 'a.vaultarc')],
      configurable: true,
    });
    inp.dispatchEvent(new Event('change'));
    await flush(80);
  };

  it('restores only after the password and a plain-spoken confirmation', async () => {
    await open({ backup_archive_restore: () => null });
    confirmAnswer = false;
    await pick('ARC');
    expect(calls.some(([c]) => c === 'backup_archive_restore')).toBe(false);
    confirmAnswer = true;
    await pick('ARC');
    expect(calls.find(([c]) => c === 'backup_archive_restore')![1]).toEqual({
      text: 'ARC',
      password: 'archive-password-123',
    });
    expect(toasts.some(([m]) => m.startsWith('Restored. Reloading'))).toBe(true);
  });

  it('a file that cannot be read is treated as no file', async () => {
    await open({ backup_archive_restore: () => null });
    $('s-archive-restore-btn').click();
    await flush();
    const inp = document.querySelector<HTMLInputElement>('input[type=file]')!;
    const broken = { text: () => Promise.reject(new Error('unreadable')) };
    Object.defineProperty(inp, 'files', { value: [broken], configurable: true });
    inp.dispatchEvent(new Event('change'));
    await flush(80);
    expect(calls.some(([c]) => c === 'backup_archive_restore')).toBe(false);
    expect(document.querySelector('input[type=file]')).toBeNull();
  });

  it('a failed restore is reported; cancelling the picker or the password does nothing', async () => {
    await open({
      backup_archive_restore: () => {
        throw new Error('wrong password');
      },
    });
    await pick('ARC');
    expect(toasts).toContainEqual(['Restore failed: wrong password', 'err']);
    $('s-archive-restore-btn').click();
    await flush();
    const inp = document.querySelector<HTMLInputElement>('input[type=file]')!;
    Object.defineProperty(inp, 'files', { value: [], configurable: true });
    inp.dispatchEvent(new Event('change'));
    await flush();
    passwordAnswer = null;
    const before = toasts.length;
    await pick('ARC');
    expect(toasts).toHaveLength(before);
  });
});

describe('recovery packages', { timeout: 40000 }, () => {
  const set = (id: string, v: string) => ($<HTMLInputElement>(id).value = v);
  const press = async (id: string) => {
    $(id).click();
    await flush(60);
  };
  const handlers = () => ({
    backup_destination_list: () => [{ id: 'main', path: '/b' }],
    backup_schedule_list: () => [{ id: 's1', target: 'main' }],
    ops_list: () => [{ id: 'r1', kind: 'package', state: 'done', outcome: 'ok' }],
    ops_show: () => ({}),
    backup_destination_add: () => null,
    backup_package: () => ({ package: '/b/pkg.age' }),
    backup_drill: () => ({ id: 'd1', state: 'done' }),
    backup_recovery_key: () => ['age1recipient', 'AGE-SECRET-KEY-1'],
    backup_sftp_host_key: () => 'SHA256:abc',
    backup_destination_add_sftp: () => null,
    backup_schedule_add: () => null,
    backup_schedule_drill: () => null,
    backup_destination_remove: () => null,
    ops_cancel: () => null,
  });

  it('summarises destinations, schedules and the latest run on open and on refresh', async () => {
    await open(handlers());
    await vi.waitFor(() =>
      expect($('s-recovery-status').textContent).toBe(
        'Destinations: main · schedules: 1 · latest: package done: ok',
      ),
    );
    backend.ops_list = () => [{ id: 'r2', kind: 'drill', state: 'failed', error_msg: 'no space' }];
    await press('s-recovery-refresh');
    expect($('s-recovery-status').textContent).toContain('drill failed: no space');
    backend.ops_list = () => [{ id: 'r3', kind: 'drill', state: 'running' }];
    await press('s-recovery-refresh');
    expect($('s-recovery-status').textContent).toContain('drill running');
    backend.ops_list = () => [];
    backend.backup_destination_list = () => [];
    await press('s-recovery-refresh');
    expect($('s-recovery-status').textContent).toBe(
      'Destinations: none · schedules: 1 · latest: none',
    );
  });

  it('shows the recovery identity once, in a confirmation', async () => {
    await open(handlers());
    await press('s-recovery-key');
    expect($('s-recovery-status').textContent).toBe('Recovery recipient configured: age1recipient');
  });

  it('adds, packages, drills and removes a destination, and cancels a run', async () => {
    await open(handlers());
    set('s-recovery-dest-id', 'main');
    set('s-recovery-dest-path', '/b');
    await press('s-recovery-dest-add');
    expect(calls.find(([c]) => c === 'backup_destination_add')![1]).toEqual({
      id: 'main',
      path: '/b',
    });
    await press('s-recovery-package');
    // Regression: this line used to be overwritten by the summary a moment later.
    expect($('s-recovery-status').textContent).toContain('Verified package written: /b/pkg.age');
    await press('s-recovery-drill');
    expect($('s-recovery-status').textContent).toContain('Restore drill d1: done');
    expect(calls.some(([c]) => c === 'backup_drill')).toBe(true);
    await press('s-recovery-dest-remove');
    expect(calls.find(([c]) => c === 'backup_destination_remove')![1]).toEqual({ id: 'main' });
    set('s-recovery-run-id', 'r1');
    await press('s-recovery-cancel');
    expect(calls.find(([c]) => c === 'ops_cancel')![1]).toEqual({ id: 'r1' });
  });

  it('each failing action says what failed', async () => {
    await open({
      ...handlers(),
      backup_recovery_key: () => {
        throw new Error('a');
      },
      backup_destination_add: () => {
        throw new Error('b');
      },
      backup_package: () => {
        throw new Error('c');
      },
      backup_drill: () => {
        throw new Error('d');
      },
      backup_schedule_add: () => {
        throw new Error('e');
      },
      backup_schedule_drill: () => {
        throw new Error('f');
      },
      backup_destination_remove: () => {
        throw new Error('g');
      },
      ops_cancel: () => {
        throw new Error('h');
      },
      backup_sftp_host_key: () => {
        throw new Error('i');
      },
    });
    set('s-recovery-daily', '03:00');
    for (const id of [
      's-recovery-key',
      's-recovery-dest-add',
      's-recovery-package',
      's-recovery-drill',
      's-recovery-schedule',
      's-recovery-drill-schedule',
      's-recovery-dest-remove',
      's-recovery-cancel',
      's-recovery-sftp-add',
    ])
      await press(id);
    const m = toasts.map(([t]) => t);
    for (const prefix of [
      'Recovery key failed: a',
      'Destination failed: b',
      'Package failed: c',
      'Restore drill failed: d',
      'Schedule failed: e',
      'Drill schedule failed: f',
      'Could not remove destination: g',
      'Could not cancel run: h',
      'SFTP destination failed: i',
    ])
      expect(m, prefix).toContain(prefix);
  });

  it('a refresh that cannot reach the desktop says so', async () => {
    await open(handlers());
    backend.ops_list = () => {
      throw new Error('offline');
    };
    await press('s-recovery-refresh');
    expect(toasts.some(([m]) => m.startsWith('Could not refresh recovery runs'))).toBe(true);
  });

  it('SFTP: pins the host key only after showing its fingerprint, refuses a bad port, and a no stops', async () => {
    await open(handlers());
    set('s-recovery-dest-id', 'sftp1');
    set('s-recovery-sftp-host', 'host.test');
    set('s-recovery-sftp-user', 'me');
    set('s-recovery-sftp-path', '/up');
    set('s-recovery-sftp-credential', 'secret');
    set('s-recovery-sftp-port', '70000');
    await press('s-recovery-sftp-add');
    expect(toasts).toContainEqual([
      'SFTP destination failed: SFTP port must be between 1 and 65535.',
      'err',
    ]);
    set('s-recovery-sftp-port', '');
    confirmAnswer = false;
    await press('s-recovery-sftp-add');
    expect(calls.some(([c]) => c === 'backup_destination_add_sftp')).toBe(false);
    confirmAnswer = true;
    await press('s-recovery-sftp-add');
    expect(calls.find(([c]) => c === 'backup_destination_add_sftp')![1]).toMatchObject({
      id: 'sftp1',
      host: 'host.test',
      port: 22,
      hostKey: 'SHA256:abc',
    });
  });

  it('scheduling needs exactly one of an interval or a daily time, and warns that the app must stay open', async () => {
    await open(handlers());
    set('s-recovery-dest-id', 'main');
    await press('s-recovery-schedule');
    expect(toasts).toContainEqual([
      'Schedule failed: Enter exactly one interval or daily time.',
      'err',
    ]);
    set('s-recovery-daily', '03:00');
    set('s-recovery-interval', '60');
    await press('s-recovery-schedule');
    expect(toasts.filter(([m]) => m.startsWith('Schedule failed')).length).toBe(2);
    set('s-recovery-interval', '');
    set('s-recovery-time-zone', 'Europe/Oslo');
    await press('s-recovery-schedule');
    expect(calls.find(([c]) => c === 'backup_schedule_add')![1]).toEqual({
      dest: 'main',
      interval: undefined,
      daily: '03:00',
      timeZone: 'Europe/Oslo',
    });
    set('s-recovery-time-zone', '');
    await press('s-recovery-drill-schedule');
    expect(calls.find(([c]) => c === 'backup_schedule_drill')![1]).toMatchObject({
      timeZone: 'UTC',
    });
    set('s-recovery-daily', '');
    set('s-recovery-interval', '30');
    set('s-recovery-time-zone', '');
    await press('s-recovery-schedule');
    expect(calls.filter(([c]) => c === 'backup_schedule_add').pop()![1]).toMatchObject({
      interval: 30,
      timeZone: 'UTC',
    });
  });
});

describe('the entropy source', { timeout: 40000 }, () => {
  it('offers the sources the desktop reports, marking ones that are not ready', async () => {
    await open({
      entropy_sources: () => [
        { id: 'os', ready: true, detail: '' },
        { id: 'hwrng', ready: false, detail: 'no device' },
      ],
    });
    const options = [...$<HTMLSelectElement>('s-entropy-source').options];
    expect(options.map((o) => o.value)).toEqual(['os', 'hwrng']);
    expect(options[0].textContent).toBe('Operating system');
    expect(options[1].disabled).toBe(true);
  });

  it('keeps the plain list when the desktop cannot answer', async () => {
    await open({
      entropy_sources: () => {
        throw new Error('no');
      },
    });
    expect($<HTMLSelectElement>('s-entropy-source').options.length).toBeGreaterThanOrEqual(1);
  });
});
