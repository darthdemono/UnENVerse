// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
/**
 * Settings -> backup, recovery and housekeeping controls (desktop app).
 *
 * These are the buttons a person presses when they are worried. The behaviours
 * that matter: nothing is written or replaced without saying what and asking, a
 * host key is shown before it is trusted forever, a backup schedule says that
 * the app must stay open, and a failure is named rather than swallowed.
 */
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { loadRealIndexHtml } from './helpers';

const toasts: [string, string?][] = [];
const confirms: string[] = [];
let confirmAnswer = true;
let passwordAnswer: string | null = 'a-very-long-archive-password';
let saveResult: any = { ok: true, path: '/dl/unenverse.vaultarc' };
const saved: unknown[][] = [];
vi.mock('../src/ts/utils', async (importOriginal) => {
  const real = await importOriginal<typeof import('../src/ts/utils')>();
  return {
    ...real,
    showToast: (m: string, k?: string) => toasts.push([m, k]),
    showConfirm: async (m: string) => {
      confirms.push(m);
      return confirmAnswer;
    },
    showPasswordPrompt: async () => passwordAnswer,
    saveFile: async (...a: unknown[]) => {
      saved.push(a);
      return saveResult;
    },
  };
});

const $ = <T extends HTMLElement = HTMLElement>(id: string) => document.getElementById(id) as T;
const flush = (ms = 20) => new Promise((r) => setTimeout(r, ms));

type Backend = (cmd: string, args: any) => unknown;
let calls: [string, any][] = [];
let backend: Backend;
let panel: typeof import('../src/ts/settings-panel');

async function boot(desktop = true) {
  vi.resetModules();
  loadRealIndexHtml();
  if (desktop)
    (window as any).__TAURI__ = {
      core: {
        invoke: async (cmd: string, args: any) => {
          calls.push([cmd, args]);
          const r = backend(cmd, args);
          if (r instanceof Error) throw r;
          return r;
        },
      },
    };
  else delete (window as any).__TAURI__;
  panel = await import('../src/ts/settings-panel');
  panel.openSettings();
  await flush();
}

const standard: Backend = (cmd) => {
  switch (cmd) {
    case 'catalogue_status':
      return { source: 'bundled' };
    case 'backup_destination_list':
      return [{ id: 'nas', path: '/mnt/nas' }];
    case 'backup_schedule_list':
      return [];
    case 'ops_list':
      return [];
    case 'entropy_sources':
      return [
        { id: 'os', ready: true, detail: '' },
        { id: 'dev', ready: false, detail: 'no device' },
      ];
    default:
      return null;
  }
};

beforeEach(() => {
  toasts.length = 0;
  confirms.length = 0;
  calls = [];
  saved.length = 0;
  confirmAnswer = true;
  passwordAnswer = 'a-very-long-archive-password';
  saveResult = { ok: true, path: '/dl/unenverse.vaultarc' };
  backend = standard;
});
afterEach(() => {
  vi.useRealTimers();
  delete (window as any).__TAURI__;
});

describe('in a plain browser', () => {
  it('points at the command line and disables what needs the desktop app', async () => {
    await boot(false);
    expect($('s-catalogue-status').textContent).toContain('unv catalogue update');
    expect($<HTMLButtonElement>('s-catalogue-update').disabled).toBe(true);
    expect($<HTMLButtonElement>('s-archive-btn').disabled).toBe(true);
    expect($<HTMLButtonElement>('s-archive-restore-btn').title).toContain('unv backup archive');
    expect($<HTMLButtonElement>('s-recovery-package').disabled).toBe(true);
    expect($<HTMLButtonElement>('s-recovery-package').title).toContain('unv backup');
    expect($<HTMLSelectElement>('s-entropy-source').disabled).toBe(true);
  });
});

describe('issuer catalogue', () => {
  it('says which list is in use and can update it', async () => {
    await boot();
    expect($('s-catalogue-status').textContent).toContain('built into this app');
    backend = (cmd) =>
      cmd === 'catalogue_update'
        ? { generated_at: '2026-10-01', providers: 120 }
        : standard(cmd, {});
    $('s-catalogue-update').click();
    await flush();
    expect($('s-catalogue-status').textContent).toBe(
      'Using the downloaded catalogue from 2026-10-01 (120 providers).',
    );
    expect(toasts).toContainEqual(['Catalogue updated', 'ok']);
    expect($<HTMLButtonElement>('s-catalogue-update').disabled).toBe(false);
  });

  it('shows the reason when the status cannot be read or the update is refused', async () => {
    backend = (cmd) => (cmd === 'catalogue_status' ? new Error('corrupt') : standard(cmd, {}));
    await boot();
    expect($('s-catalogue-status').textContent).toContain('Cannot read the catalogue');
    backend = (cmd) =>
      cmd === 'catalogue_update' ? new Error('bad signature') : standard(cmd, {});
    $('s-catalogue-update').click();
    await flush();
    expect($('s-catalogue-status').textContent).toContain('Not updated');
    expect($('s-catalogue-status').textContent).toContain('bad signature');
    expect(toasts).toContainEqual(['Catalogue not updated', 'err']);
    expect($<HTMLButtonElement>('s-catalogue-update').disabled).toBe(false);
  });
});

describe('full archive', () => {
  it('asks for a password, builds the archive in Rust and writes it to a file', async () => {
    backend = (cmd) => (cmd === 'backup_archive_build' ? '{"archive":true}' : standard(cmd, {}));
    await boot();
    $('s-archive-btn').click();
    await flush();
    expect(calls.find(([c]) => c === 'backup_archive_build')![1]).toEqual({
      password: 'a-very-long-archive-password',
    });
    expect(saved[0]).toEqual(['{"archive":true}', 'unenverse.vaultarc', 'application/json']);
    expect(toasts).toContainEqual(['Archive written: /dl/unenverse.vaultarc', 'ok']);
  });

  it('writes nothing without a password, and says why a write or a build failed', async () => {
    backend = (cmd) => (cmd === 'backup_archive_build' ? 'x' : standard(cmd, {}));
    await boot();
    passwordAnswer = null;
    $('s-archive-btn').click();
    await flush();
    expect(calls.some(([c]) => c === 'backup_archive_build')).toBe(false);
    passwordAnswer = 'pw-long-enough-123';
    saveResult = { ok: false, error: 'disk full' };
    $('s-archive-btn').click();
    await flush();
    expect(toasts).toContainEqual(['Could not write the archive: disk full', 'err']);
    saveResult = { ok: true, path: null };
    $('s-archive-btn').click();
    await flush();
    expect(toasts).toContainEqual(['Archive written', 'ok']);
    backend = (cmd) =>
      cmd === 'backup_archive_build' ? new Error('weak password') : standard(cmd, {});
    $('s-archive-btn').click();
    await flush();
    expect(toasts.some(([m]) => m === 'Archive failed: weak password')).toBe(true);
  });

  const pick = (text: string | null) =>
    vi.spyOn(HTMLInputElement.prototype, 'click').mockImplementation(function (
      this: HTMLInputElement,
    ) {
      Object.defineProperty(this, 'files', {
        value: text === null ? [] : [new File([text], 'a.vaultarc')],
      });
      this.onchange?.(new Event('change'));
    });

  it('restoring warns that the current vault is overwritten, and only then restores and reloads', async () => {
    await boot();
    const spy = pick('ARCHIVE');
    vi.useFakeTimers();
    $('s-archive-restore-btn').click();
    await vi.advanceTimersByTimeAsync(50);
    expect(confirms[0]).toContain('overwritten and cannot be recovered');
    expect(calls.find(([c]) => c === 'backup_archive_restore')![1]).toEqual({
      text: 'ARCHIVE',
      password: 'a-very-long-archive-password',
    });
    expect(toasts.some(([m]) => m.startsWith('Restored. Reloading'))).toBe(true);
    spy.mockRestore();
  });

  it('declining, no file, no password or a failed restore changes nothing', async () => {
    await boot();
    let spy = pick('ARCHIVE');
    confirmAnswer = false;
    $('s-archive-restore-btn').click();
    await flush(50);
    expect(calls.some(([c]) => c === 'backup_archive_restore')).toBe(false);
    spy.mockRestore();
    spy = pick(null);
    $('s-archive-restore-btn').click();
    await flush(50);
    spy.mockRestore();
    spy = pick('ARCHIVE');
    passwordAnswer = null;
    $('s-archive-restore-btn').click();
    await flush(50);
    expect(calls.some(([c]) => c === 'backup_archive_restore')).toBe(false);
    passwordAnswer = 'pw';
    confirmAnswer = true;
    backend = (cmd) =>
      cmd === 'backup_archive_restore' ? new Error('wrong password') : standard(cmd, {});
    $('s-archive-restore-btn').click();
    await flush(50);
    expect(toasts.some(([m]) => m === 'Restore failed: wrong password')).toBe(true);
    spy.mockRestore();
  });
});

describe('recovery packages and schedules', () => {
  beforeEach(async () => {
    await boot();
    $<HTMLInputElement>('s-recovery-dest-id').value = 'nas';
  });
  const status = () => $('s-recovery-status').textContent!;

  it('summarises destinations, schedules and the latest run on open and on Refresh', async () => {
    expect(status()).toBe('Destinations: nas · schedules: 0 · latest: none');
    backend = (cmd) =>
      cmd === 'ops_list'
        ? [{ id: 'r1', kind: 'package', state: 'done', outcome: 'ok' }]
        : cmd === 'backup_schedule_list'
          ? [{ id: 's', target: 'nas' }]
          : standard(cmd, {});
    $('s-recovery-refresh').click();
    await flush();
    expect(status()).toBe('Destinations: nas · schedules: 1 · latest: package done: ok');
    expect(calls.some(([c, a]) => c === 'ops_show' && a.id === 'r1')).toBe(true);
    backend = (cmd) =>
      cmd === 'ops_list'
        ? [{ id: 'r2', kind: 'drill', state: 'failed', error_msg: 'no space' }]
        : cmd === 'backup_destination_list'
          ? []
          : standard(cmd, {});
    $('s-recovery-refresh').click();
    await flush();
    expect(status()).toBe('Destinations: none · schedules: 0 · latest: drill failed: no space');
    backend = (cmd) => (cmd === 'ops_list' ? new Error('db locked') : standard(cmd, {}));
    $('s-recovery-refresh').click();
    await flush();
    expect(toasts.some(([m]) => m.startsWith('Could not refresh recovery runs'))).toBe(true);
  });

  it('shows the recovery identity once, and names the recipient', async () => {
    backend = (cmd) =>
      cmd === 'backup_recovery_key' ? ['age1recipient', 'AGE-SECRET-KEY-1ABC'] : standard(cmd, {});
    $('s-recovery-key').click();
    await flush();
    expect(confirms[0]).toContain('shown once only');
    expect(confirms[0]).toContain('age1recipient');
    expect(confirms[0]).toContain('AGE-SECRET-KEY-1ABC');
    expect(status()).toBe('Recovery recipient configured: age1recipient');
    backend = (cmd) => (cmd === 'backup_recovery_key' ? new Error('no key') : standard(cmd, {}));
    $('s-recovery-key').click();
    await flush();
    expect(toasts.some(([m]) => m === 'Recovery key failed: no key')).toBe(true);
  });

  it('adds a folder destination, packages to it, drills it and removes it', async () => {
    $<HTMLInputElement>('s-recovery-dest-path').value = '/mnt/nas';
    $('s-recovery-dest-add').click();
    await flush();
    expect(calls.find(([c]) => c === 'backup_destination_add')![1]).toEqual({
      id: 'nas',
      path: '/mnt/nas',
    });
    backend = (cmd) =>
      cmd === 'backup_package'
        ? { package: 'pkg-2026.vaultpkg' }
        : cmd === 'backup_drill'
          ? { id: 'd1', state: 'verified' }
          : standard(cmd, {});
    $('s-recovery-package').click();
    await flush();
    expect(calls.find(([c]) => c === 'backup_package')![1]).toEqual({ dest: 'nas' });
    $('s-recovery-drill').click();
    await flush();
    expect(calls.find(([c]) => c === 'backup_drill')![1]).toEqual({ dest: 'nas' });
    $('s-recovery-dest-remove').click();
    await flush();
    expect(calls.find(([c]) => c === 'backup_destination_remove')![1]).toEqual({ id: 'nas' });
  });

  it('names the failure of each destination action', async () => {
    for (const [btn, cmd, msg] of [
      ['s-recovery-dest-add', 'backup_destination_add', 'Destination failed: boom'],
      ['s-recovery-package', 'backup_package', 'Package failed: boom'],
      ['s-recovery-drill', 'backup_drill', 'Restore drill failed: boom'],
      ['s-recovery-dest-remove', 'backup_destination_remove', 'Could not remove destination: boom'],
      ['s-recovery-cancel', 'ops_cancel', 'Could not cancel run: boom'],
      ['s-recovery-drill-schedule', 'backup_schedule_drill', 'Drill schedule failed: boom'],
    ] as const) {
      backend = (c) => (c === cmd ? new Error('boom') : standard(c, {}));
      $(btn).click();
      await flush();
      expect(
        toasts.some(([m]) => m === msg),
        msg,
      ).toBe(true);
    }
  });

  it('cancels a run by id', async () => {
    $<HTMLInputElement>('s-recovery-run-id').value = 'run-9';
    $('s-recovery-cancel').click();
    await flush();
    expect(calls.find(([c]) => c === 'ops_cancel')![1]).toEqual({ id: 'run-9' });
  });

  describe('SFTP destination', () => {
    const fillSftp = (port = '2222') => {
      $<HTMLInputElement>('s-recovery-sftp-host').value = 'host.example';
      $<HTMLInputElement>('s-recovery-sftp-port').value = port;
      $<HTMLInputElement>('s-recovery-sftp-user').value = 'backup';
      $<HTMLInputElement>('s-recovery-sftp-path').value = '/srv/b';
      $<HTMLInputElement>('s-recovery-sftp-credential').value = 'cred';
    };

    it('shows the server’s host key and only pins it when accepted', async () => {
      backend = (cmd) => (cmd === 'backup_sftp_host_key' ? 'SHA256:abcdef' : standard(cmd, {}));
      fillSftp();
      confirmAnswer = false;
      $('s-recovery-sftp-add').click();
      await flush();
      expect(confirms[0]).toContain('SHA256:abcdef');
      expect(confirms[0]).toContain('A changed key will always be refused');
      expect(calls.some(([c]) => c === 'backup_destination_add_sftp')).toBe(false);
      confirmAnswer = true;
      $('s-recovery-sftp-add').click();
      await flush();
      expect(calls.find(([c]) => c === 'backup_destination_add_sftp')![1]).toMatchObject({
        id: 'nas',
        host: 'host.example',
        port: 2222,
        user: 'backup',
        path: '/srv/b',
        hostKey: 'SHA256:abcdef',
      });
    });

    it('defaults to port 22 and refuses an impossible port before contacting the server', async () => {
      backend = (cmd) => (cmd === 'backup_sftp_host_key' ? 'FP' : standard(cmd, {}));
      fillSftp('');
      $('s-recovery-sftp-add').click();
      await flush();
      expect(calls.find(([c]) => c === 'backup_sftp_host_key')![1].port).toBe(22);
      calls.length = 0;
      for (const bad of ['0', '70000', '22.5', 'abc']) {
        fillSftp(bad);
        $('s-recovery-sftp-add').click();
        await flush();
        expect(
          toasts.some(([m]) => m.includes('SFTP port must be between 1 and 65535')),
          bad,
        ).toBe(true);
      }
      expect(calls.some(([c]) => c === 'backup_sftp_host_key')).toBe(false);
    });

    it('names a failure to reach the server', async () => {
      backend = (cmd) =>
        cmd === 'backup_sftp_host_key' ? new Error('unreachable') : standard(cmd, {});
      fillSftp();
      $('s-recovery-sftp-add').click();
      await flush();
      expect(toasts.some(([m]) => m === 'SFTP destination failed: unreachable')).toBe(true);
    });
  });

  describe('schedules', () => {
    it('needs exactly one of an interval or a daily time', async () => {
      $<HTMLInputElement>('s-recovery-interval').value = '';
      $<HTMLInputElement>('s-recovery-daily').value = '';
      $('s-recovery-schedule').click();
      await flush();
      expect(toasts.some(([m]) => m.includes('Enter exactly one interval or daily time'))).toBe(
        true,
      );
      $<HTMLInputElement>('s-recovery-interval').value = '60';
      $<HTMLInputElement>('s-recovery-daily').value = '03:00';
      toasts.length = 0;
      $('s-recovery-schedule').click();
      await flush();
      expect(toasts.some(([m]) => m.includes('Enter exactly one interval or daily time'))).toBe(
        true,
      );
      expect(calls.some(([c]) => c === 'backup_schedule_add')).toBe(false);
    });

    it('schedules on an interval or a daily time, defaulting to UTC, and warns the app must stay open', async () => {
      $<HTMLInputElement>('s-recovery-interval').value = '60';
      $('s-recovery-schedule').click();
      await flush();
      expect(calls.find(([c]) => c === 'backup_schedule_add')![1]).toEqual({
        dest: 'nas',
        interval: 60,
        daily: undefined,
        timeZone: 'UTC',
      });
      expect(confirms[0]).toContain('only while UnENVerse remains open and unlocked');
      $<HTMLInputElement>('s-recovery-interval').value = '';
      $<HTMLInputElement>('s-recovery-daily').value = '03:30';
      $<HTMLInputElement>('s-recovery-time-zone').value = 'Europe/Budapest';
      $('s-recovery-schedule').click();
      await flush();
      expect(calls.filter(([c]) => c === 'backup_schedule_add')[1][1]).toEqual({
        dest: 'nas',
        interval: undefined,
        daily: '03:30',
        timeZone: 'Europe/Budapest',
      });
    });

    it('a monthly restore drill says it only restores to a temporary directory', async () => {
      $('s-recovery-drill-schedule').click();
      await flush();
      expect(calls.find(([c]) => c === 'backup_schedule_drill')![1]).toEqual({
        dest: 'nas',
        day: 1,
        at: '03:00',
        timeZone: 'UTC',
      });
      expect(confirms[0]).toContain('never activates the restored vault');
    });

    it('names a failure to schedule', async () => {
      backend = (cmd) =>
        cmd === 'backup_schedule_add' ? new Error('bad zone') : standard(cmd, {});
      $<HTMLInputElement>('s-recovery-interval').value = '10';
      $('s-recovery-schedule').click();
      await flush();
      expect(toasts.some(([m]) => m === 'Schedule failed: bad zone')).toBe(true);
    });
  });
});

describe('entropy source', () => {
  it('offers what the backend reports and marks an unavailable device with its reason', async () => {
    await boot();
    const opts = [...$<HTMLSelectElement>('s-entropy-source').options];
    expect(opts.map((o) => o.value)).toEqual(['os', 'dev']);
    expect(opts[0].textContent).toBe('Operating system');
    expect(opts[1].disabled).toBe(true);
    expect(opts[1].textContent).toContain('unavailable: no device');
  });

  it('says why the list cannot be shown', async () => {
    backend = (cmd) => (cmd === 'entropy_sources' ? new Error('no rng') : standard(cmd, {}));
    await boot();
    expect($<HTMLSelectElement>('s-entropy-source').title).toContain('Cannot list entropy sources');
  });
});

describe('clearing search history', () => {
  it('empties the saved searches and says so', async () => {
    await boot(false);
    const { Settings } = await import('../src/ts/state');
    Settings.set('recentSearches', ['a', 'b']);
    $('s-clear-recent').click();
    expect(Settings.get('recentSearches')).toEqual([]);
    expect(toasts).toContainEqual(['Search history cleared', 'ok']);
  });
});
