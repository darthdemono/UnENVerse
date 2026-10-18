/**
 * Users and classes panel: the owner's console for who can reach what.
 *
 * Two backends sit behind the same screen: a local vault answers through Tauri
 * commands, a remote one through the server's owner-only REST routes. The tests
 * run the same user actions against both and pin what the person sees, what a
 * destructive action asks first, and that nothing secret is left on screen.
 */
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { loadRealIndexHtml, resetState } from './helpers';

// The panel keeps module-level state (it binds its handlers once), so each test
// gets a fresh copy of both modules to match the fresh document it is given.
let st: (typeof import('../src/ts/state'))['st'];
let RemoteVaultStore: (typeof import('../src/ts/state'))['RemoteVaultStore'];
let renderUsersPanel: (typeof import('../src/ts/users'))['renderUsersPanel'];
let renderUserDetail: (typeof import('../src/ts/users'))['renderUserDetail'];
let renderClassesPanel: (typeof import('../src/ts/users'))['renderClassesPanel'];
let openCreateUserForm: (typeof import('../src/ts/users'))['openCreateUserForm'];
let initUsersPanel: (typeof import('../src/ts/users'))['initUsersPanel'];
let resetUsersPanelState: (typeof import('../src/ts/users'))['resetUsersPanelState'];

const toasts: [string, string?][] = [];
let confirmAnswer = true;
const confirms: string[] = [];
let promptAnswer: string | null = 'answer';
let passwordAnswer: string | null = 'pw-answer';
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
    showPasswordPrompt: async () => passwordAnswer,
    clipboardWrite: async () => {},
  };
});

const $ = <T extends HTMLElement = HTMLElement>(id: string) => document.getElementById(id) as T;
const flush = (ms = 20) => new Promise((r) => setTimeout(r, ms));
const click = async (id: string) => {
  $(id).click();
  await flush();
};
const text = () => $('users-workspace').textContent!.replace(/\s+/g, ' ');

const user = (over: object = {}) => ({
  id: 'u1',
  username: 'alice',
  is_owner: false,
  has_password: true,
  totp_enabled: false,
  created_at: '2026-01-02T00:00:00Z',
  last_seen_at: null,
  class_id: null,
  strict_write: false,
  ...over,
});
const cls = (over: object = {}) => ({
  id: 'c1',
  name: 'Dev',
  description: 'devs',
  cap_manage_users: false,
  cap_manage_classes: false,
  cap_delete_projects: false,
  strict_write: false,
  ...over,
});

type Backend = (cmd: string, args: any) => unknown;
let calls: [string, any][] = [];
let backend: Backend;
const defaultBackend: Backend = (cmd) => {
  switch (cmd) {
    case 'list_users':
      return [
        user(),
        user({
          id: 'o',
          username: 'owner',
          is_owner: true,
          totp_enabled: true,
          has_password: true,
          last_seen_at: '2026-03-04T10:00:00Z',
        }),
        user({ id: 't', username: 'bot', has_password: false }),
      ];
    case 'list_user_tokens':
      return [];
    case 'get_user_permissions':
    case 'get_class_permissions':
      return { read: '', write: '' };
    case 'list_user_classes':
      return [
        cls(),
        cls({ id: 'cls-admin', name: 'Admin', description: '', cap_manage_classes: true }),
        cls({ id: 'c3', name: 'Mod', cap_manage_users: true }),
      ];
    case 'totp_status':
      return { enrolled: false, enabled: false };
    default:
      return null;
  }
};

function useLocal() {
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
}

beforeEach(async () => {
  vi.resetModules();
  loadRealIndexHtml();
  ({ st, RemoteVaultStore } = await import('../src/ts/state'));
  ({
    renderUsersPanel,
    renderUserDetail,
    renderClassesPanel,
    openCreateUserForm,
    initUsersPanel,
    resetUsersPanelState,
  } = await import('../src/ts/users'));
  resetState(st);
  toasts.length = 0;
  confirms.length = 0;
  calls = [];
  confirmAnswer = true;
  promptAnswer = 'answer';
  passwordAnswer = 'pw-answer';
  backend = defaultBackend;
  useLocal();
});

describe('the user list', () => {
  it('shows each user with owner, 2FA and token-only badges and when they were last seen', async () => {
    await renderUsersPanel();
    const t = $('users-list').textContent!.replace(/\s+/g, ' ');
    expect(t).toContain('alice');
    expect(t).toContain('Owner');
    expect(t).toContain('2FA');
    expect(t).toContain('⬤ token-only');
    expect(t).toContain('⬤ password · never seen');
    expect(t).toContain('2026-03-04');
    expect($('users-list').querySelectorAll('.owner-badge')).toHaveLength(1);
  });

  it('marks the selected user, says when there are none, and shows a failure instead of an empty list', async () => {
    st.selectedUserId = 'u1';
    await renderUsersPanel();
    expect($('users-list').querySelector('[data-user-id="u1"]')!.classList.contains('active')).toBe(
      true,
    );
    backend = () => [];
    await renderUsersPanel();
    expect($('users-list').textContent).toContain('No users yet');
    backend = () => new Error('server down');
    await renderUsersPanel();
    expect($('users-list').textContent).toContain('Could not load users: server down');
  });

  it('escapes a hostile user name', async () => {
    backend = () => [user({ username: '<img src=x onerror=alert(1)>' })];
    await renderUsersPanel();
    expect($('users-list').querySelector('img')).toBeNull();
  });
});

describe('a user in detail', () => {
  it('lists tokens with their expiry state: never, soon, expired and later', async () => {
    const day = (n: number) => new Date(Date.now() + n * 86_400_000).toISOString();
    backend = (cmd) =>
      cmd === 'list_user_tokens'
        ? [
            { id: 't1', description: 'laptop', created_at: day(-5), expires_at: null },
            { id: 't2', description: null, created_at: day(-5), expires_at: day(3) },
            { id: 't3', description: 'old', created_at: day(-50), expires_at: day(-2) },
            { id: 't4', description: 'later', created_at: day(-5), expires_at: day(90) },
          ]
        : defaultBackend(cmd, {});
    await renderUserDetail('u1');
    const t = text();
    expect(t).toContain('laptop');
    expect(t).toContain('never expires');
    expect(t).toContain('Unnamed token');
    expect(t).toMatch(/⚠ expires \d{4}-\d{2}-\d{2}/);
    expect(t).toMatch(/⚠ EXPIRED \d{4}-\d{2}-\d{2}/);
    expect(t).toMatch(/expires \d{4}-\d{2}-\d{2}/);
    expect($('users-workspace').querySelectorAll('.revoke-token-btn')).toHaveLength(4);
  });

  it('offers no delete, class or two-factor controls for the owner', async () => {
    await renderUserDetail('o');
    expect(document.getElementById('delete-user-btn')).toBeNull();
    expect(document.getElementById('user-class-select')).toBeNull();
    expect(document.getElementById('totp-enroll-btn')).toBeNull();
    expect(document.getElementById('strict-write-user')).toBeNull();
    expect(text()).toContain('Owner');
  });

  it('shows a token-only badge and a class picker with the current class selected', async () => {
    backend = (cmd, a) =>
      cmd === 'list_users'
        ? [user({ has_password: false, class_id: 'c3' })]
        : defaultBackend(cmd, a);
    await renderUserDetail('u1');
    expect(text()).toContain('token-only');
    expect($<HTMLSelectElement>('user-class-select').value).toBe('c3');
  });

  it('applies a class or removes it, and reports a failure', async () => {
    await renderUserDetail('u1');
    $<HTMLSelectElement>('user-class-select').value = 'c1';
    await click('assign-class-btn');
    expect(calls.find(([c]) => c === 'assign_user_class')![1]).toEqual({
      userId: 'u1',
      classId: 'c1',
    });
    expect(toasts).toContainEqual(['Class assigned', 'ok']);
    $<HTMLSelectElement>('user-class-select').value = '';
    await click('assign-class-btn');
    expect(toasts).toContainEqual(['Class removed', 'ok']);
    backend = (cmd, a) =>
      cmd === 'assign_user_class' ? new Error('denied') : defaultBackend(cmd, a);
    await click('assign-class-btn');
    expect(toasts.some(([m, k]) => m === 'Failed: denied' && k === 'err')).toBe(true);
  });

  it('uses the class rules and strict setting when previewing what a user can reach', async () => {
    backend = (cmd, a) => {
      if (cmd === 'list_users') return [user({ class_id: 'c1' })];
      if (cmd === 'list_user_classes') return [cls({ strict_write: true })];
      if (cmd === 'get_class_permissions') return { read: 'tag:prod', write: '' };
      return defaultBackend(cmd, a);
    };
    await renderUserDetail('u1');
    expect(calls.some(([c, a]) => c === 'get_class_permissions' && a.classId === 'c1')).toBe(true);
    backend = (cmd, a) => {
      if (cmd === 'list_users') return [user({ class_id: 'c1' })];
      if (cmd === 'get_class_permissions') return new Error('gone');
      return defaultBackend(cmd, a);
    };
    await renderUserDetail('u1');
    expect($('users-workspace').textContent).toContain('alice');
  });
});

describe('renaming, passwords and deleting', () => {
  beforeEach(async () => {
    await renderUserDetail('u1');
  });

  it('the rename form opens and closes, refuses a blank name and sends the new one', async () => {
    expect($('rename-user-form').style.display).toBe('none');
    await click('rename-user-btn');
    expect($('rename-user-form').style.display).toBe('flex');
    await click('rename-user-btn');
    expect($('rename-user-form').style.display).toBe('none');
    await click('rename-user-btn');
    await click('rename-user-cancel');
    expect($('rename-user-form').style.display).toBe('none');
    $<HTMLInputElement>('rename-user-input').value = '   ';
    await click('rename-user-confirm');
    expect(toasts).toContainEqual(['Username cannot be empty', 'err']);
    $<HTMLInputElement>('rename-user-input').value = ' alicia ';
    await click('rename-user-confirm');
    expect(calls.find(([c]) => c === 'rename_user')![1]).toEqual({
      userId: 'u1',
      newUsername: 'alicia',
    });
    expect(toasts).toContainEqual(['Renamed to "alicia"', 'ok']);
  });

  it('reports a failed rename', async () => {
    backend = (cmd, a) => (cmd === 'rename_user' ? new Error('taken') : defaultBackend(cmd, a));
    await click('rename-user-confirm');
    $<HTMLInputElement>('rename-user-input').value = 'bob';
    await click('rename-user-confirm');
    expect(toasts.some(([m]) => m === 'Rename failed: taken')).toBe(true);
  });

  it('a blank password answer switches to token-only; failure is reported', async () => {
    passwordAnswer = '';
    await click('change-pw-btn');
    expect(calls.find(([c]) => c === 'set_user_password')![1]).toEqual({
      userId: 'u1',
      password: null,
    });
    expect(toasts).toContainEqual(['Switched to token-only auth', 'ok']);
    passwordAnswer = 'new pw';
    await click('change-pw-btn');
    expect(toasts).toContainEqual(['Password updated', 'ok']);
    backend = (cmd, a) =>
      cmd === 'set_user_password' ? new Error('weak') : defaultBackend(cmd, a);
    await click('change-pw-btn');
    expect(toasts.some(([m]) => m === 'Failed: weak')).toBe(true);
  });

  it('deleting asks first, then clears the selection and the workspace', async () => {
    st.selectedUserId = 'u1';
    confirmAnswer = false;
    await click('delete-user-btn');
    expect(calls.some(([c]) => c === 'delete_user')).toBe(false);
    confirmAnswer = true;
    await click('delete-user-btn');
    expect(confirms[confirms.length - 1]).toContain(
      'Delete this user and all their tokens and permissions',
    );
    expect(calls.some(([c, a]) => c === 'delete_user' && a.userId === 'u1')).toBe(true);
    expect(st.selectedUserId).toBeNull();
    expect(text()).toContain('Select a user to manage');
  });

  it('reports a failed delete and leaves the selection', async () => {
    st.selectedUserId = 'u1';
    backend = (cmd, a) => (cmd === 'delete_user' ? new Error('nope') : defaultBackend(cmd, a));
    await click('delete-user-btn');
    expect(toasts.some(([m]) => m === 'Delete failed: nope')).toBe(true);
    expect(st.selectedUserId).toBe('u1');
  });
});

describe('tokens', () => {
  it('a new token is shown once in an overlay that copies and dismisses; cancelling creates nothing', async () => {
    await renderUserDetail('u1');
    promptAnswer = null;
    await click('new-token-btn');
    expect(calls.some(([c]) => c === 'create_user_token')).toBe(false);
    promptAnswer = 'CI';
    backend = (cmd, a) =>
      cmd === 'create_user_token' ? { token: 'unv_tok_123' } : defaultBackend(cmd, a);
    await click('new-token-btn');
    expect(calls.find(([c]) => c === 'create_user_token')![1]).toEqual({
      userId: 'u1',
      description: 'CI',
    });
    expect($('token-display').textContent).toBe('unv_tok_123');
    await click('copy-new-token');
    expect(toasts).toContainEqual(['Copied ✓', 'ok']);
    await click('close-token-overlay');
    expect(document.querySelector('.token-overlay-backdrop')).toBeNull();
    await click('new-token-btn');
    (document.querySelector('.token-overlay-backdrop') as HTMLElement).click();
    expect(document.querySelector('.token-overlay-backdrop')).toBeNull();
  });

  it('says so when the server returns no token or fails', async () => {
    await renderUserDetail('u1');
    await click('new-token-btn');
    expect(toasts).toContainEqual(['Token creation failed', 'err']);
    backend = (cmd, a) =>
      cmd === 'create_user_token' ? new Error('quota') : defaultBackend(cmd, a);
    await click('new-token-btn');
    expect(toasts.some(([m]) => m === 'Token creation failed: quota')).toBe(true);
  });

  it('revoking asks first and then reports', async () => {
    backend = (cmd, a) =>
      cmd === 'list_user_tokens'
        ? [{ id: 't1', description: 'x', created_at: '2026-01-01T00:00:00Z', expires_at: null }]
        : defaultBackend(cmd, a);
    await renderUserDetail('u1');
    confirmAnswer = false;
    $('users-workspace').querySelector<HTMLElement>('.revoke-token-btn')!.click();
    await flush();
    expect(calls.some(([c]) => c === 'revoke_user_token')).toBe(false);
    confirmAnswer = true;
    $('users-workspace').querySelector<HTMLElement>('.revoke-token-btn')!.click();
    await flush();
    expect(calls.find(([c]) => c === 'revoke_user_token')![1]).toEqual({
      tokenId: 't1',
      userId: 'u1',
    });
    expect(toasts).toContainEqual(['Token revoked', 'ok']);
    backend = (cmd, a) => (cmd === 'revoke_user_token' ? new Error('x') : defaultBackend(cmd, a));
    $('users-workspace').querySelector<HTMLElement>('.revoke-token-btn')!.click();
    await flush();
    expect(toasts.some(([m]) => m === 'Revoke failed: x')).toBe(true);
  });
});

describe('two-factor sign-in for a sub-user', () => {
  const withTotp = (state: object) => (cmd: string, a: any) =>
    cmd === 'totp_status' ? state : defaultBackend(cmd, a);

  it('shows its three states honestly, including "not confirmed" meaning not protected', async () => {
    await renderUserDetail('u1');
    expect(text()).toContain('Not enabled. Sign-in needs only this user');
    expect($('totp-enroll-btn').textContent).toBe('Enable Two-Factor');
    backend = withTotp({ enrolled: true, enabled: false });
    await renderUserDetail('u1');
    expect(text()).toContain('Not confirmed');
    expect(text()).toContain('is not protected'.replace('is ', ''));
    expect($('totp-enroll-btn').textContent).toBe('Start Again');
    backend = withTotp({ enrolled: true, enabled: true });
    await renderUserDetail('u1');
    expect(text()).toContain('must enter a code from their authenticator');
    expect($('totp-disable-btn')).toBeTruthy();
  });

  it('a server without the routes degrades to "not enabled" rather than blanking the view', async () => {
    backend = (cmd, a) => (cmd === 'totp_status' ? new Error('404') : defaultBackend(cmd, a));
    await renderUserDetail('u1');
    expect(text()).toContain('Not enabled');
    expect(text()).toContain('alice');
  });

  it('removing it asks first, then destroys the secret', async () => {
    backend = withTotp({ enrolled: true, enabled: true });
    await renderUserDetail('u1');
    confirmAnswer = false;
    await click('totp-disable-btn');
    expect(calls.some(([c]) => c === 'totp_disable')).toBe(false);
    confirmAnswer = true;
    await click('totp-disable-btn');
    expect(calls.some(([c, a]) => c === 'totp_disable' && a.userId === 'u1')).toBe(true);
    expect(toasts).toContainEqual(['Second factor removed', 'ok']);
    backend = (cmd, a) =>
      cmd === 'totp_disable'
        ? new Error('busy')
        : withTotp({ enrolled: true, enabled: true })(cmd, a);
    await click('totp-disable-btn');
    expect(toasts.some(([m]) => m === 'Failed: busy')).toBe(true);
  });

  describe('enrolling', () => {
    const enroll =
      (over: object = {}) =>
      (cmd: string, a: any) =>
        cmd === 'totp_enroll'
          ? { secret: 'ABCDEFGHIJKLMNOP', uri: 'otpauth://totp/x?secret=ABCDEFGHIJKLMNOP', ...over }
          : defaultBackend(cmd, a);

    it('shows the secret in groups of four, with copy buttons, and is not on until a code is accepted', async () => {
      backend = enroll();
      await renderUserDetail('u1');
      await click('totp-enroll-btn');
      expect($('totp-secret-display').textContent).toBe('ABCD EFGH IJKL MNOP');
      expect($('totp-enroll-panel').style.display).toBe('block');
      await click('totp-copy-secret');
      await click('totp-copy-uri');
      expect(toasts.filter(([m]) => m === 'Copied ✓')).toHaveLength(2);
      expect(text()).toContain('Two-factor is not on until a code is accepted');
    });

    it('wants six digits, says a wrong code is a clock problem, and enables on a right one', async () => {
      backend = enroll();
      await renderUserDetail('u1');
      await click('totp-enroll-btn');
      $<HTMLInputElement>('totp-code-input').value = '12 34';
      await click('totp-confirm-btn');
      expect(toasts).toContainEqual(['Enter the six-digit code', 'err']);
      $<HTMLInputElement>('totp-code-input').value = '123 456';
      backend = (cmd, a) => (cmd === 'totp_confirm' ? false : enroll()(cmd, a));
      await click('totp-confirm-btn');
      expect(calls.find(([c]) => c === 'totp_confirm')![1]).toEqual({
        userId: 'u1',
        code: '123456',
      });
      expect(toasts.some(([m]) => m.includes('Incorrect code'))).toBe(true);
      expect($<HTMLButtonElement>('totp-confirm-btn').disabled).toBe(false);
      backend = (cmd, a) => (cmd === 'totp_confirm' ? true : enroll()(cmd, a));
      $<HTMLInputElement>('totp-code-input').dispatchEvent(
        new KeyboardEvent('keydown', { key: 'Enter' }),
      );
      await flush();
      expect(toasts).toContainEqual(['Two-factor enabled', 'ok']);
    });

    it('reports a failed confirmation, and cancelling just repaints the true state', async () => {
      backend = enroll();
      await renderUserDetail('u1');
      await click('totp-enroll-btn');
      $<HTMLInputElement>('totp-code-input').value = '123456';
      backend = (cmd, a) => (cmd === 'totp_confirm' ? new Error('locked out') : enroll()(cmd, a));
      await click('totp-confirm-btn');
      expect(toasts.some(([m]) => m === 'Failed: locked out')).toBe(true);
      await click('totp-cancel-btn');
      expect(document.getElementById('totp-secret-display')).toBeNull();
    });

    it('explains a failed start or an answer with no secret, and re-enables the button', async () => {
      backend = (cmd, a) =>
        cmd === 'totp_enroll' ? new Error('no route') : defaultBackend(cmd, a);
      await renderUserDetail('u1');
      await click('totp-enroll-btn');
      expect(toasts.some(([m]) => m === 'Could not start enrollment: no route')).toBe(true);
      expect($<HTMLButtonElement>('totp-enroll-btn').disabled).toBe(false);
      backend = (cmd, a) => (cmd === 'totp_enroll' ? {} : defaultBackend(cmd, a));
      await click('totp-enroll-btn');
      expect(toasts).toContainEqual(['Server returned no secret', 'err']);
    });

    it('escapes anything the server returns', async () => {
      backend = enroll({ secret: '<img src=x onerror=1>' });
      await renderUserDetail('u1');
      await click('totp-enroll-btn');
      expect($('totp-enroll-panel').querySelector('img')).toBeNull();
    });
  });
});

describe('strict write scoping', () => {
  it('switches on and off per user, and puts the box back if the change fails', async () => {
    await renderUserDetail('u1');
    const box = () => $<HTMLInputElement>('strict-write-user');
    box().checked = true;
    box().dispatchEvent(new Event('change'));
    await flush();
    expect(toasts).toContainEqual(['Strict write scoping on', 'ok']);
    box().checked = false;
    box().dispatchEvent(new Event('change'));
    await flush();
    expect(toasts).toContainEqual(['Strict write scoping off', 'ok']);
    backend = (cmd, a) =>
      cmd === 'set_strict_write' ? new Error('not owner') : defaultBackend(cmd, a);
    box().checked = true;
    box().dispatchEvent(new Event('change'));
    await flush();
    expect(box().checked).toBe(false);
    expect(toasts.some(([m]) => m.startsWith('Could not change it: not owner'))).toBe(true);
  });
});

describe('creating a user', () => {
  it('needs a name, creates the user, selects them and shows their detail', async () => {
    openCreateUserForm();
    await click('confirm-create-user');
    expect(toasts).toContainEqual(['Username is required', 'err']);
    $<HTMLInputElement>('new-user-name').value = ' carol ';
    $<HTMLInputElement>('new-user-password').value = '';
    backend = (cmd, a) =>
      cmd === 'create_user'
        ? user({ id: 'new1', username: 'carol' })
        : cmd === 'list_users'
          ? [user({ id: 'new1', username: 'carol' })]
          : defaultBackend(cmd, a);
    await click('confirm-create-user');
    expect(calls.find(([c]) => c === 'create_user')![1]).toEqual({
      username: 'carol',
      password: undefined,
    });
    expect(toasts).toContainEqual(['User "carol" created', 'ok']);
    expect(st.selectedUserId).toBe('new1');
    expect(text()).toContain('carol');
  });

  it('sends a password when given, reports an empty answer and a failure, and Cancel closes the form', async () => {
    openCreateUserForm();
    $<HTMLInputElement>('new-user-name').value = 'dave';
    $<HTMLInputElement>('new-user-password').value = 'pw 123';
    await click('confirm-create-user');
    expect(calls.find(([c]) => c === 'create_user')![1].password).toBe('pw 123');
    expect(toasts).toContainEqual(['Failed to create user', 'err']);
    backend = (cmd, a) => (cmd === 'create_user' ? new Error('exists') : defaultBackend(cmd, a));
    await click('confirm-create-user');
    expect(toasts.some(([m]) => m === 'Failed: exists')).toBe(true);
    await click('cancel-create-user');
    expect(text()).toContain('Select a user to manage');
  });

  it('does nothing without the workspace', () => {
    document.body.innerHTML = '';
    expect(() => openCreateUserForm()).not.toThrow();
  });
});

describe('classes', () => {
  it('lists classes with their capability dots and descriptions, and handles none or a failure', async () => {
    await renderClassesPanel();
    const t = $('classes-list').textContent!.replace(/\s+/g, ' ');
    expect(t).toContain('Dev');
    expect(t).toContain('devs');
    expect(t).toContain('—');
    expect($('classes-list').querySelector('.admin-dot')).toBeTruthy();
    expect($('classes-list').querySelector('.mod-dot')).toBeTruthy();
    backend = () => [];
    await renderClassesPanel();
    expect($('classes-list').textContent).toContain('No classes found');
    backend = () => new Error('classes unavailable');
    await renderClassesPanel();
    expect($('classes-list').textContent).toContain('classes unavailable');
  });

  const open = async (id: string) => {
    initUsersPanel();
    document.querySelector<HTMLElement>('.users-subnav-btn[data-sub="classes"]')!.click();
    await flush();
    $('classes-list').querySelector<HTMLElement>(`[data-class-id="${id}"]`)!.click();
    await flush();
  };

  it('built-in classes cannot be deleted; custom ones ask first and then go', async () => {
    await open('cls-admin');
    expect(text()).toContain('built-in');
    expect(document.getElementById('delete-class-btn')).toBeNull();
    await open('c1');
    confirmAnswer = false;
    await click('delete-class-btn');
    expect(calls.some(([c]) => c === 'delete_user_class')).toBe(false);
    confirmAnswer = true;
    await click('delete-class-btn');
    expect(calls.find(([c]) => c === 'delete_user_class')![1]).toEqual({ classId: 'c1' });
    expect(text()).toContain('Select a class to manage');
    await open('c1');
    backend = (cmd, a) =>
      cmd === 'delete_user_class' ? new Error('in use') : defaultBackend(cmd, a);
    await click('delete-class-btn');
    expect(toasts.some(([m]) => m === 'Failed: in use')).toBe(true);
  });

  it('saves the name, description and each capability; a blank name is refused', async () => {
    await open('c1');
    $<HTMLInputElement>('cls-name').value = '  ';
    await click('save-class-btn');
    expect(toasts).toContainEqual(['Name is required', 'err']);
    $<HTMLInputElement>('cls-name').value = ' Developers ';
    $<HTMLInputElement>('cls-desc').value = ' all devs ';
    $<HTMLInputElement>('cap-manage-users').checked = true;
    $<HTMLInputElement>('cap-delete-projects').checked = true;
    await click('save-class-btn');
    expect(calls.find(([c]) => c === 'update_user_class')![1]).toEqual({
      classId: 'c1',
      name: 'Developers',
      description: 'all devs',
      capManageUsers: true,
      capManageClasses: false,
      capDeleteProjects: true,
    });
    expect(toasts).toContainEqual(['Class updated', 'ok']);
    backend = (cmd, a) =>
      cmd === 'update_user_class' ? new Error('dup name') : defaultBackend(cmd, a);
    await click('save-class-btn');
    expect(toasts.some(([m]) => m === 'Failed: dup name')).toBe(true);
  });

  it('a class says its rules are ANDed with a member’s own, and can be made strict', async () => {
    await open('c1');
    expect(text()).toContain('ANDed');
    const box = $<HTMLInputElement>('strict-write-class');
    box.checked = true;
    box.dispatchEvent(new Event('change'));
    await flush();
    expect(calls.find(([c]) => c === 'set_strict_write')![1]).toEqual({
      subjectKind: 'class',
      subjectId: 'c1',
      strict: true,
    });
  });

  it('shows "not found" and errors rather than a blank class', async () => {
    initUsersPanel();
    document.querySelector<HTMLElement>('.users-subnav-btn[data-sub="classes"]')!.click();
    await flush();
    backend = (cmd, a) => (cmd === 'list_user_classes' ? [] : defaultBackend(cmd, a));
    const ghost = document.createElement('button');
    ghost.dataset.classId = 'ghost';
    $('classes-list').appendChild(ghost);
    ghost.click();
    await flush();
    expect(text()).toContain('Class not found');
    backend = (cmd, a) =>
      cmd === 'get_class_permissions' ? new Error('perm failure') : defaultBackend(cmd, a);
    const again = document.createElement('button');
    again.dataset.classId = 'ghost';
    $('classes-list').appendChild(again);
    again.click();
    await flush();
    expect(text()).toContain('perm failure');
  });

  it('creating a class needs a name and selects the new class', async () => {
    initUsersPanel();
    document.querySelector<HTMLElement>('.users-subnav-btn[data-sub="classes"]')!.click();
    await flush();
    await click('create-class-btn');
    await click('confirm-create-class');
    expect(toasts).toContainEqual(['Name is required', 'err']);
    $<HTMLInputElement>('new-cls-name').value = ' Ops ';
    $<HTMLInputElement>('new-cls-desc').value = 'on call';
    $<HTMLInputElement>('new-cap-mc').checked = true;
    backend = (cmd, a) =>
      cmd === 'create_user_class'
        ? cls({ id: 'c9', name: 'Ops' })
        : cmd === 'list_user_classes'
          ? [cls({ id: 'c9', name: 'Ops' })]
          : defaultBackend(cmd, a);
    await click('confirm-create-class');
    expect(calls.find(([c]) => c === 'create_user_class')![1]).toEqual({
      name: 'Ops',
      description: 'on call',
      capManageUsers: false,
      capManageClasses: true,
      capDeleteProjects: false,
    });
    expect(toasts).toContainEqual(['Class "Ops" created', 'ok']);
    expect(text()).toContain('Ops');
  });

  it('reports a class the server did not create or refused, and Cancel closes the form', async () => {
    initUsersPanel();
    await click('create-class-btn');
    $<HTMLInputElement>('new-cls-name').value = 'X';
    await click('confirm-create-class');
    expect(toasts).toContainEqual(['Failed to create class', 'err']);
    backend = (cmd, a) =>
      cmd === 'create_user_class' ? new Error('limit') : defaultBackend(cmd, a);
    await click('confirm-create-class');
    expect(toasts.some(([m]) => m === 'Failed: limit')).toBe(true);
    await click('cancel-create-class');
    expect(text()).toContain('Select a class to manage');
  });
});

describe('the panel itself', () => {
  it('switching between Users and Classes swaps the lists, buttons and workspace hint', async () => {
    initUsersPanel();
    const sub = (s: string) =>
      document.querySelector<HTMLElement>(`.users-subnav-btn[data-sub="${s}"]`)!;
    sub('classes').click();
    await flush();
    expect($('users-list').style.display).toBe('none');
    expect($('classes-list').style.display).toBe('');
    expect($('create-user-btn').style.display).toBe('none');
    expect($('create-class-btn').style.display).toBe('');
    expect(text()).toContain('Select a class to configure');
    sub('users').click();
    await flush();
    expect($('users-list').style.display).toBe('');
    expect($('classes-list').style.display).toBe('none');
    expect($('create-class-btn').style.display).toBe('none');
    expect(text()).toContain('Select a user to manage');
  });

  it('clicking a user selects and opens them; initialising again only refreshes', async () => {
    initUsersPanel();
    await renderUsersPanel();
    $('users-list').querySelector<HTMLElement>('[data-user-id="u1"]')!.click();
    await flush(40);
    expect(st.selectedUserId).toBe('u1');
    expect(text()).toContain('Access Tokens');
    $('users-list').click();
    initUsersPanel();
    await flush();
  });

  it('forgets the selection when the vault changes', async () => {
    initUsersPanel();
    st.selectedUserId = 'u1';
    resetUsersPanelState();
    expect(st.selectedUserId).toBeNull();
    expect(text()).toContain('Select a user to manage');
  });
});

describe('against a remote vault', () => {
  let api: ReturnType<typeof vi.fn>;
  const remote = (answers: Record<string, unknown> = {}) => {
    const store = new RemoteVaultStore('http://localhost:1');
    api = vi.fn(async (path: string, method?: string) => {
      const key = `${method ?? 'GET'} ${path}`;
      if (key in answers) return answers[key];
      if (path === '/api/users') return [user()];
      if (path.endsWith('/tokens')) return [];
      if (path.endsWith('/permissions')) return { read: '', write: '' };
      if (path === '/api/classes') return [cls()];
      if (path.endsWith('/totp')) return { enrolled: false, enabled: false };
      return null;
    });
    store.api = api as never;
    st.store = store;
  };
  const sent = () => api.mock.calls.map(([p, m, b]) => [m ?? 'GET', p, b]);

  it('routes every user action to the owner-only REST route that does it', async () => {
    remote();
    await renderUserDetail('u1');
    await click('change-pw-btn');
    await click('rename-user-confirm');
    $<HTMLInputElement>('rename-user-input').value = 'newname';
    await click('rename-user-confirm');
    $<HTMLSelectElement>('user-class-select').value = 'c1';
    await click('assign-class-btn');
    await click('new-token-btn');
    await click('delete-user-btn');
    const s = sent();
    expect(s).toContainEqual(['GET', '/api/users', undefined]);
    expect(s).toContainEqual(['PUT', '/api/users/u1/password', { password: 'pw-answer' }]);
    expect(s).toContainEqual(['PUT', '/api/users/u1/rename', { username: 'newname' }]);
    expect(s).toContainEqual(['PUT', '/api/users/u1/class', { class_id: 'c1' }]);
    expect(s).toContainEqual(['POST', '/api/users/u1/tokens', { description: 'answer' }]);
    expect(s).toContainEqual(['DELETE', '/api/users/u1', undefined]);
    expect(s).toContainEqual(['GET', '/api/users/u1/permissions', undefined]);
  });

  it('routes tokens, two-factor and strict scoping', async () => {
    remote({
      'GET /api/users/u1/tokens': [
        { id: 't1', description: 'x', created_at: '2026-01-01T00:00:00Z', expires_at: null },
      ],
      'POST /api/users/u1/totp/enroll': { secret: 'ABCD', uri: 'u' },
      'POST /api/users/u1/totp/confirm': { enabled: true },
    });
    await renderUserDetail('u1');
    $('users-workspace').querySelector<HTMLElement>('.revoke-token-btn')!.click();
    await flush();
    await click('totp-enroll-btn');
    $<HTMLInputElement>('totp-code-input').value = '123456';
    await click('totp-confirm-btn');
    const box = $<HTMLInputElement>('strict-write-user');
    box.checked = true;
    box.dispatchEvent(new Event('change'));
    await flush();
    const s = sent();
    expect(s).toContainEqual(['DELETE', '/api/users/u1/tokens/t1', undefined]);
    expect(s).toContainEqual(['POST', '/api/users/u1/totp/enroll', undefined]);
    expect(s).toContainEqual(['POST', '/api/users/u1/totp/confirm', { code: '123456' }]);
    expect(s).toContainEqual(['PUT', '/api/users/u1/strict-write', { strict: true }]);
  });

  it('routes the disable of two-factor, class changes and class permissions', async () => {
    remote({ 'GET /api/users/u1/totp': { enrolled: true, enabled: true } });
    await renderUserDetail('u1');
    await click('totp-disable-btn');
    initUsersPanel();
    document.querySelector<HTMLElement>('.users-subnav-btn[data-sub="classes"]')!.click();
    await flush();
    $('classes-list').querySelector<HTMLElement>('[data-class-id="c1"]')!.click();
    await flush();
    $<HTMLInputElement>('cap-manage-users').checked = true;
    await click('save-class-btn');
    const strict = $<HTMLInputElement>('strict-write-class');
    strict.checked = true;
    strict.dispatchEvent(new Event('change'));
    await flush();
    await click('delete-class-btn');
    await click('create-class-btn');
    $<HTMLInputElement>('new-cls-name').value = 'N';
    await click('confirm-create-class');
    const s = sent();
    expect(s).toContainEqual(['DELETE', '/api/users/u1/totp', undefined]);
    expect(s).toContainEqual(['GET', '/api/classes/c1/permissions', undefined]);
    expect(s).toContainEqual([
      'PUT',
      '/api/classes/c1',
      {
        name: 'Dev',
        description: 'devs',
        cap_manage_users: true,
        cap_manage_classes: false,
        cap_delete_projects: false,
      },
    ]);
    expect(s).toContainEqual(['PUT', '/api/classes/c1/strict-write', { strict: true }]);
    expect(s).toContainEqual(['DELETE', '/api/classes/c1', undefined]);
    expect(s).toContainEqual([
      'POST',
      '/api/classes',
      {
        name: 'N',
        description: '',
        cap_manage_users: false,
        cap_manage_classes: false,
        cap_delete_projects: false,
      },
    ]);
  });

  it('creating a user posts the name and a null password when none was typed', async () => {
    remote({ 'POST /api/users': user({ id: 'n2', username: 'erin' }) });
    openCreateUserForm();
    $<HTMLInputElement>('new-user-name').value = 'erin';
    await click('confirm-create-user');
    expect(sent()).toContainEqual(['POST', '/api/users', { username: 'erin', password: null }]);
  });

  it('a remote that answers nothing is treated as an empty result, not a crash', async () => {
    remote({ 'GET /api/users': null });
    await renderUsersPanel();
    expect($('users-list').textContent).toContain('No users yet');
  });
});

describe('without the desktop app or a remote', () => {
  it('the lists are empty rather than broken', async () => {
    delete (window as any).__TAURI__;
    await renderUsersPanel();
    expect($('users-list').textContent).toContain('No users yet');
    await renderClassesPanel();
    expect($('classes-list').textContent).toContain('No classes found');
  });
});
