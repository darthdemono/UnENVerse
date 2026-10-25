// SPDX-License-Identifier: PolyForm-Strict-1.0.0
// Copyright Jubair Hasan.
/**
 * The buttons and keys the shell wires at start-up that no other pane owns:
 * the sidebar resize handle, the category and project trees, the settings
 * panel's controls, the copy-all menu, the create dialogs, the lock button, the
 * undo bar and the shortcut overlays. Booted for real and driven through the DOM.
 */
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { bootApp, entry } from './app-boot';

const toasts: [string, string?][] = [];
const copied: string[] = [];
const saved: { text: string; name: string }[] = [];
let confirmAnswer = true;
let promptAnswer: string | null = null;
vi.mock('../src/ts/utils', async (importOriginal) => {
  const real = await importOriginal<typeof import('../src/ts/utils')>();
  return {
    ...real,
    showToast: (m: string, k?: string) => toasts.push([m, k]),
    showConfirm: async () => confirmAnswer,
    showPrompt: async () => promptAnswer,
    clipboardWrite: async (t: string) => void copied.push(t),
    saveFile: async (text: string, name: string) => {
      saved.push({ text, name });
      return { ok: true, path: '/tmp/' + name };
    },
  };
});

const $ = <T extends HTMLElement = HTMLElement>(id: string) => document.getElementById(id) as T;
const flush = (ms = 30) => new Promise((r) => setTimeout(r, ms));
const key = (k: string, init: KeyboardEventInit = {}, target: EventTarget = document) =>
  target.dispatchEvent(
    new KeyboardEvent('keydown', { key: k, bubbles: true, cancelable: true, ...init }),
  );

let st: Awaited<ReturnType<typeof bootApp>>['st'];
let Settings: Awaited<ReturnType<typeof bootApp>>['Settings'];

beforeEach(async () => {
  toasts.length = 0;
  copied.length = 0;
  saved.length = 0;
  confirmAnswer = true;
  promptAnswer = null;
  ({ st, Settings } = await bootApp(
    [
      entry('a', 'Alpha', { projectIds: ['Universal', 'web'], categories: ['infra'] }),
      entry('b', 'Beta', { projectIds: ['Universal'], categories: ['infra'] }),
    ],
    {
      categories: ['infra', 'extra'],
      projects: [
        { id: 'Universal', name: 'Universal' },
        { id: 'web', name: 'Web' },
      ],
    },
  ));
}, 30000);

describe('the sidebar resize handle', { timeout: 30000 }, () => {
  const handle = () => $('sidebar-resizer');
  const width = () => $('sidebar').style.width;

  it('double-clicking resets the width to the stylesheet default and says so', () => {
    $('sidebar').style.width = '400px';
    handle().dispatchEvent(new MouseEvent('dblclick', { bubbles: true }));
    expect(width()).toBe('');
    expect(Settings.get('sidebarWidth')).toBe(0);
    expect(toasts.some(([m]) => m === 'Sidebar width reset')).toBe(true);
  });

  it('Enter and Space do the same as a double-click', () => {
    for (const k of ['Enter', ' ']) {
      toasts.length = 0;
      $('sidebar').style.width = '400px';
      key(k, {}, handle());
      expect(width()).toBe('');
      expect(toasts.some(([m]) => m === 'Sidebar width reset')).toBe(true);
    }
  });

  it('Home and End jump to the bounds; the arrows step by 10, or 40 with Shift', () => {
    key('Home', {}, handle());
    const min = parseInt(width());
    key('End', {}, handle());
    const max = parseInt(width());
    expect(max).toBeGreaterThan(min);
    expect(Settings.get('sidebarWidth')).toBe(max);
    expect(handle().getAttribute('aria-valuenow')).toBe(String(max));
    // jsdom has no layout, so the current width reads 0 and a step clamps to the lower bound.
    key('ArrowRight', {}, handle());
    expect(parseInt(width())).toBeGreaterThanOrEqual(min);
    key('ArrowLeft', { shiftKey: true }, handle());
    expect(parseInt(width())).toBe(min);
  });

  it('an unrelated key is left alone', () => {
    $('sidebar').style.width = '333px';
    const ev = new KeyboardEvent('keydown', { key: 'a', bubbles: true, cancelable: true });
    handle().dispatchEvent(ev);
    expect(ev.defaultPrevented).toBe(false);
    expect(width()).toBe('333px');
  });
});

describe('the category and project trees', { timeout: 30000 }, () => {
  it('a project row selects it, and selecting it again returns to the Universal project', () => {
    const row = () => document.querySelector<HTMLElement>('#project-list [data-project-id="web"]')!;
    row().click();
    expect(st.currentSelectedProjectIds).toEqual(['web']);
    row().click();
    expect(st.currentSelectedProjectIds).toEqual(['Universal']);
  });

  it('the rename button on a project row asks for a new name', async () => {
    promptAnswer = 'Website';
    document.querySelector<HTMLElement>('#project-list .rename-proj[data-project="web"]')!.click();
    await flush(80);
    expect(st.vault.projects.find((p) => p.id === 'web' || p.name === 'Website')?.name).toBe(
      'Website',
    );
  });

  it('the delete button on a project row asks first and removes it from its entries', async () => {
    document.querySelector<HTMLElement>('#project-list .delete-proj[data-project="web"]')!.click();
    await flush(80);
    expect(st.vault.projects.some((p) => p.id === 'web')).toBe(false);
    expect(st.vault.api_keys[0].projectIds).not.toContain('web');
  });

  it('the rename and delete buttons on a category row act on that category', async () => {
    promptAnswer = 'platform';
    document
      .querySelector<HTMLElement>('#category-tree .rename-cat[data-category="infra"]')!
      .click();
    await flush(80);
    expect(st.vault.api_keys[0].categories).toEqual(['platform']);
    document
      .querySelector<HTMLElement>('#category-tree .delete-cat[data-category="platform"]')!
      .click();
    await flush(80);
    expect(st.vault.user_categories).not.toContain('platform');
    expect(st.vault.api_keys[0].categories).not.toContain('platform');
  });

  it('a click on empty space in either tree does nothing', () => {
    const before = JSON.stringify(st.vault);
    $('category-tree').click();
    $('project-list').click();
    expect(JSON.stringify(st.vault)).toBe(before);
  });
});

describe('the entry form buttons', { timeout: 30000 }, () => {
  it('Duplicate on an open entry copies it and closes the form', async () => {
    document.querySelector<HTMLElement>('#card-grid [data-action="edit"][data-idx="0"]')!.click();
    await flush();
    expect($('modal-overlay').classList.contains('open')).toBe(true);
    $('modal-duplicate').click();
    await flush();
    expect(st.vault.api_keys).toHaveLength(3);
    expect($('modal-overlay').classList.contains('open')).toBe(false);
  });

  it('Duplicate on a form that is not editing an entry only closes it', async () => {
    $('add-btn').click();
    await flush();
    $<HTMLInputElement>('edit-index').value = '-1';
    $('modal-duplicate').click();
    await flush();
    expect(st.vault.api_keys).toHaveLength(2);
    expect($('modal-overlay').classList.contains('open')).toBe(false);
  });

  it('typing an icon name previews it, and clearing the field clears the preview', () => {
    const inp = $<HTMLInputElement>('f-icon');
    inp.value = 'github';
    inp.dispatchEvent(new Event('input', { bubbles: true }));
    expect($('f-icon-preview').innerHTML).not.toBe('');
    inp.value = '';
    inp.dispatchEvent(new Event('input', { bubbles: true }));
    expect($('f-icon-preview').innerHTML).toBe('');
  });

  it('the icon button opens the picker', () => {
    $('f-icon-pick').click();
    expect(
      document.querySelector('.icon-picker, #icon-picker-overlay, [id*="icon-pick"]'),
    ).not.toBeNull();
  });
});

describe('the settings panel', { timeout: 30000 }, () => {
  it('the close button and a click on the backdrop save and close it; a click inside does not', () => {
    $('settings-btn').click();
    expect($('settings-overlay').classList.contains('open')).toBe(true);
    $('settings-overlay').querySelector<HTMLElement>('.modal, .settings-panel, div')!.click();
    expect($('settings-overlay').classList.contains('open')).toBe(true);
    $('settings-overlay').click();
    expect($('settings-overlay').classList.contains('open')).toBe(false);
    $('settings-btn').click();
    $('settings-close').click();
    expect($('settings-overlay').classList.contains('open')).toBe(false);
  });

  it('exporting settings saves the stored settings as JSON and reports the path', async () => {
    $('settings-export-btn').click();
    await flush();
    expect(saved[0].name).toBe('settings.json');
    expect(JSON.parse(saved[0].text).onboardingCompleted).toBe(true);
    expect(toasts.some(([m]) => m === 'Exported to /tmp/settings.json')).toBe(true);
  });

  it('a segmented control marks the chosen button, saves the choice and repaints', () => {
    const ctrl = $('s-card-size');
    const buttons = [...ctrl.querySelectorAll<HTMLButtonElement>('button[data-val]')];
    const target = buttons[buttons.length - 1];
    target.click();
    expect(buttons.filter((b) => b.classList.contains('active'))).toEqual([target]);
    expect(Settings.get('cardSize')).toBe(target.dataset.val);
    // Clicking the gap between buttons selects nothing.
    ctrl.click();
    expect(buttons.filter((b) => b.classList.contains('active'))).toEqual([target]);
  });

  it('each segmented control writes its own setting', () => {
    for (const [id, setting] of [
      ['s-grid-cols', 'gridColumns'],
      ['s-activity-bar-position', 'activityBarPosition'],
      ['s-activity-bar-style', 'activityBarStyle'],
    ] as const) {
      const btn = [...$(id).querySelectorAll<HTMLButtonElement>('button[data-val]')].pop()!;
      btn.click();
      expect(String(Settings.get(setting))).toBe(btn.dataset.val);
    }
  });

  it('the accent colour previews, saves, and resets to the default', () => {
    const inp = $<HTMLInputElement>('s-accent');
    inp.value = '#112233';
    inp.dispatchEvent(new Event('input', { bubbles: true }));
    expect($('s-accent-val').textContent).toBe('#112233');
    expect(Settings.get('accentColor')).toBe('#112233');
    $('s-accent-reset').click();
    expect(inp.value).toBe('#7364c9');
    expect(Settings.get('accentColor')).toBe('#7364c9');
  });

  it('the data buttons export in each format', async () => {
    for (const id of [
      'settings-export-dotenv',
      'settings-export-yaml',
      'settings-export-json',
      'settings-export-k8s',
      'settings-export-tfvars',
    ]) {
      saved.length = 0;
      copied.length = 0;
      $(id).click();
      await flush(60);
      expect(saved.length + copied.length, id).toBeGreaterThan(0);
    }
  });

  it('importing an encrypted backup without a password asks for one first', () => {
    $<HTMLInputElement>('settings-backup-pw').value = '';
    $('settings-import-encrypted').click();
    expect(toasts.some(([m, k]) => m === 'Enter the backup password first' && k === 'err')).toBe(
      true,
    );
    expect(document.querySelector('input[type=file]')).toBeNull();
  });
});

describe('copy all', { timeout: 30000 }, () => {
  it('the arrow offers three formats, each copying every visible entry', async () => {
    $('copy-all-arrow').click();
    const items = [
      ...document.querySelectorAll<HTMLElement>('.dropdown-item, .dd-item, [role="menuitem"]'),
    ];
    expect(items.map((i) => i.textContent?.trim())).toEqual(
      expect.arrayContaining(['Copy .env', 'Copy YAML', 'Copy JSON']),
    );
    for (const label of ['Copy .env', 'Copy YAML', 'Copy JSON']) {
      copied.length = 0;
      $('copy-all-arrow').click();
      [...document.querySelectorAll<HTMLElement>('.dropdown-item, .dd-item, [role="menuitem"]')]
        .find((i) => i.textContent?.trim() === label)!
        .click();
      await flush();
      expect(copied[0], label).toContain('k-a-longenough');
      expect(copied[0], label).toContain('k-b-longenough');
    }
  });
});

describe('the create dialogs', { timeout: 30000 }, () => {
  it('the category dialog saves on Enter, cancels on Escape and on a backdrop click', async () => {
    $('new-category-root-btn').click();
    expect($('category-create-overlay').classList.contains('open')).toBe(true);
    key('Escape', {}, $('category-create-name'));
    expect($('category-create-overlay').classList.contains('open')).toBe(false);
    $('new-category-root-btn').click();
    $('category-create-overlay').click();
    expect($('category-create-overlay').classList.contains('open')).toBe(false);
    $('new-category-root-btn').click();
    $<HTMLInputElement>('category-create-name').value = 'secrets';
    key('Enter', {}, $('category-create-name'));
    await flush();
    expect(st.vault.user_categories).toContain('secrets');
  });

  it('the category dialog closes from its close and cancel buttons', () => {
    for (const id of ['category-create-close', 'category-create-cancel']) {
      $('new-category-root-btn').click();
      $(id).click();
      expect($('category-create-overlay').classList.contains('open'), id).toBe(false);
    }
  });

  it('the project dialog saves on Enter, cancels on Escape, and closes from its buttons', async () => {
    for (const id of ['project-create-close', 'project-create-cancel']) {
      $('new-project-btn').click();
      expect($('project-create-overlay').classList.contains('open'), id).toBe(true);
      $(id).click();
      expect($('project-create-overlay').classList.contains('open'), id).toBe(false);
    }
    $('new-project-btn').click();
    key('Escape', {}, $('project-create-name'));
    expect($('project-create-overlay').classList.contains('open')).toBe(false);
    $('new-project-btn').click();
    $('project-create-overlay').click();
    expect($('project-create-overlay').classList.contains('open')).toBe(false);
    $('new-project-btn').click();
    $<HTMLInputElement>('project-create-name').value = 'Mobile';
    key('Enter', {}, $('project-create-name'));
    await flush();
    expect(st.vault.projects.some((p) => p.name === 'Mobile')).toBe(true);
  });

  it('choosing a project type marks that button and unmarks the rest', () => {
    $('new-project-btn').click();
    const btns = [...document.querySelectorAll<HTMLButtonElement>('.project-type-btn')];
    expect(btns.length).toBeGreaterThan(1);
    btns[1].click();
    expect(btns.map((b) => b.getAttribute('aria-checked'))).toEqual(
      btns.map((_, i) => (i === 1 ? 'true' : 'false')),
    );
    expect(btns[1].classList.contains('active')).toBe(true);
  });
});

describe('the env import and link dialogs', { timeout: 30000 }, () => {
  it('Select all and Select none tick and untick every row; a backdrop click closes it', () => {
    document.body.insertAdjacentHTML(
      'beforeend',
      '<input type="checkbox" class="env-import-check"><input type="checkbox" class="env-import-check">',
    );
    $('env-import-select-all').click();
    expect(
      [...document.querySelectorAll<HTMLInputElement>('.env-import-check')].every((c) => c.checked),
    ).toBe(true);
    $('env-import-select-none').click();
    expect(
      [...document.querySelectorAll<HTMLInputElement>('.env-import-check')].some((c) => c.checked),
    ).toBe(false);
    $('env-import-overlay').classList.add('open');
    $('env-import-overlay').click();
    expect($('env-import-overlay').classList.contains('open')).toBe(false);
  });

  it('the env link dialog closes from its backdrop', () => {
    $('env-link-overlay').classList.add('open');
    $('env-link-overlay').click();
    expect($('env-link-overlay').classList.contains('open')).toBe(false);
  });
});

describe('lock, undo and the overlays', { timeout: 30000 }, () => {
  it('Undo with nothing to undo does nothing; with something, runs it and hides the bar', async () => {
    $('undo-btn').click();
    expect(st.undoStack).toHaveLength(0);
    let ran = 0;
    st.undoStack.push({
      t: setTimeout(() => {}, 100000) as never,
      fn: () => void ran++,
      msg: 'x',
    } as never);
    $('undo-bar').classList.add('visible');
    $('undo-btn').click();
    await flush();
    expect(ran).toBe(1);
    expect($('undo-bar').classList.contains('visible')).toBe(false);
  });

  it('Undo keeps the bar while more undo steps remain', async () => {
    const mk = () =>
      ({ t: setTimeout(() => {}, 100000) as never, fn: () => {}, msg: 'x' }) as never;
    st.undoStack.push(mk(), mk());
    $('undo-bar').classList.add('visible');
    $('undo-btn').click();
    expect($('undo-bar').classList.contains('visible')).toBe(true);
  });

  it('Ctrl+Shift+P opens the command palette and the same keys close it again', () => {
    key('p', { ctrlKey: true, shiftKey: true });
    expect(document.getElementById('palette-overlay')!.classList.contains('open')).toBe(true);
    key('P', { metaKey: true, shiftKey: true });
    expect(document.getElementById('palette-overlay')!.classList.contains('open')).toBe(false);
  });

  it('the shortcuts overlay opens from its button and closes from its button, a backdrop click and Escape', () => {
    const ov = $('shortcuts-overlay');
    $('shortcuts-btn').click();
    expect(ov.classList.contains('open')).toBe(true);
    $('shortcuts-close').click();
    expect(ov.classList.contains('open')).toBe(false);
    $('shortcuts-btn').click();
    ov.click();
    expect(ov.classList.contains('open')).toBe(false);
    $('shortcuts-btn').click();
    key('Escape');
    expect(ov.classList.contains('open')).toBe(false);
    // A click on the dialog itself leaves it open.
    $('shortcuts-btn').click();
    ov.firstElementChild!.dispatchEvent(new MouseEvent('click', { bubbles: true }));
    expect(ov.classList.contains('open')).toBe(true);
  });

  it('the Wi-Fi QR overlay closes from its button and from its backdrop', () => {
    const ov = $('wifi-qr-overlay');
    ov.classList.add('open');
    $('wifi-qr-close').click();
    expect(ov.classList.contains('open')).toBe(false);
    ov.classList.add('open');
    ov.click();
    expect(ov.classList.contains('open')).toBe(false);
  });

  it('Escape cancels the settings panel', () => {
    $('settings-btn').click();
    expect($('settings-overlay').classList.contains('open')).toBe(true);
    key('Escape');
    expect($('settings-overlay').classList.contains('open')).toBe(false);
  });
});

describe('the vault switcher', { timeout: 30000 }, () => {
  it('lists the local vault and the saved remotes, and offers to add one', () => {
    Settings.set('remoteSaved', [
      { id: 'r1', name: 'Home server', url: 'https://x.test', token: 't' },
    ] as never);
    $('vault-switcher').click();
    const labels = [
      ...document.querySelectorAll<HTMLElement>('.dropdown-item, .dd-item, [role="menuitem"]'),
    ].map((i) => i.textContent?.trim());
    expect(labels).toEqual(
      expect.arrayContaining([
        '⬤ Local Vault',
        'Home server',
        '+ Add Remote Vault',
        'Remote settings…',
      ]),
    );
  });

  it('choosing a saved remote opens the remote panel on it; choosing Local while local does nothing', () => {
    Settings.set('remoteSaved', [
      { id: 'r1', name: 'Home server', url: 'https://x.test', token: 't' },
    ] as never);
    const click = (text: string) =>
      [...document.querySelectorAll<HTMLElement>('.dropdown-item, .dd-item, [role="menuitem"]')]
        .find((i) => i.textContent?.includes(text))!
        .click();
    $('vault-switcher').click();
    click('Local Vault');
    expect(st.store.isRemote).toBe(false);
    $('vault-switcher').click();
    click('Home server');
    expect(st.activeRemoteId).toBe('r1');
    $('vault-switcher').click();
    click('Remote settings');
    expect($('settings-overlay').classList.contains('open')).toBe(true);
    $('vault-switcher').click();
    click('Add Remote Vault');
  });
});

const pickFile = async (input: HTMLInputElement, name: string, text: string) => {
  const file = new File([text], name, { type: 'text/plain' });
  Object.defineProperty(input, 'files', { value: [file], configurable: true });
  input.dispatchEvent(new Event('change'));
  await flush(80);
};
const picker = () => document.querySelector<HTMLInputElement>('input[type=file]');

describe('the file picker', { timeout: 30000 }, () => {
  it('the Browse button on the load banner builds a picker that is removed again after a choice', async () => {
    $('load-banner-browse-btn').click();
    const inp = picker()!;
    expect(inp.accept).toBe('.json,.env,text/plain');
    await pickFile(inp, 'vars.env', 'ALPHA=1\nBETA=2\n');
    expect(picker()).toBeNull();
    expect($('env-import-overlay').classList.contains('open')).toBe(true);
  });

  it('a cancelled picker is removed without importing anything', () => {
    $('load-banner-browse-btn').click();
    picker()!.dispatchEvent(new Event('cancel'));
    expect(picker()).toBeNull();
  });

  it('a picker nobody answers is removed when the window regains focus', async () => {
    vi.useFakeTimers();
    try {
      $('load-banner-browse-btn').click();
      await vi.advanceTimersByTimeAsync(350);
      window.dispatchEvent(new Event('focus'));
      expect(picker()).toBeNull();
    } finally {
      vi.useRealTimers();
    }
  });

  it('the authenticator Import button reads the chosen file as text and hands it to the importer', async () => {
    $('auth-import-btn').click();
    const inp = picker()!;
    expect(inp.accept).toContain('.2fas');
    await pickFile(inp, 'codes.txt', 'not an authenticator export');
    expect(picker()).toBeNull();
    expect(toasts.length).toBeGreaterThan(0);
  });

  it('an authenticator picker with no file chosen imports nothing', async () => {
    $('auth-import-btn').click();
    const inp = picker()!;
    Object.defineProperty(inp, 'files', { value: [], configurable: true });
    toasts.length = 0;
    inp.dispatchEvent(new Event('change'));
    await flush();
    expect(toasts).toEqual([]);
    expect(picker()).toBeNull();
  });

  it('the authenticator Export button offers every export format', () => {
    $('auth-export-btn').click();
    const labels = [...document.querySelectorAll('#dropdown .dropdown-item')].map((i) =>
      i.textContent?.trim(),
    );
    expect(labels.length).toBeGreaterThan(1);
  });

  it('the Settings import button accepts json, env and yaml', () => {
    $('settings-import-btn').click();
    expect(picker()!.accept).toBe('.json,.env,.yaml,.yml,text/plain');
  });

  it('the CXF import button reads the chosen file as text', async () => {
    $('settings-cxf-import-btn').click();
    expect(picker()!.accept).toContain('.cxf');
    await pickFile(picker()!, 'x.cxf', '{"not":"cxf"}');
    expect(toasts.length).toBeGreaterThan(0);
  });

  it('the CXF export button writes a file', async () => {
    $('settings-cxf-export-btn').click();
    await flush(100);
    expect(saved.length + toasts.length).toBeGreaterThan(0);
  });
});

describe('the encrypted backup buttons', { timeout: 30000 }, () => {
  it('Export encrypted without a password asks for one rather than writing an unprotected file', async () => {
    $<HTMLInputElement>('settings-backup-pw').value = '';
    $('settings-export-encrypted').click();
    await flush();
    expect(saved).toEqual([]);
    expect(toasts[toasts.length - 1]).toEqual([
      'Backup password must be at least 12 characters',
      'err',
    ]);
  });

  it('Export encrypted with a password saves a .vaultbak that is not plain text', async () => {
    $<HTMLInputElement>('settings-backup-pw').value = 'a long enough backup password';
    $('settings-export-encrypted').click();
    await vi.waitFor(() => expect(saved).toHaveLength(1), { timeout: 20000 });
    expect(saved[0].name).toMatch(/\.vaultbak$/);
    expect(saved[0].text).not.toContain('k-a-longenough');
  });

  it('Import encrypted with a password builds a picker and reports a file that is not a backup', async () => {
    $<HTMLInputElement>('settings-backup-pw').value = 'whatever-password';
    $('settings-import-encrypted').click();
    const inp = picker()!;
    expect(inp.accept).toBe('.vaultbak,.json,text/plain');
    await pickFile(inp, 'x.vaultbak', 'garbage');
    expect(toasts.some(([m, k]) => k === 'err' && /backup/i.test(m))).toBe(true);
    expect(picker()).toBeNull();
  });

  it('a backup written with one password does not open with another', async () => {
    $<HTMLInputElement>('settings-backup-pw').value = 'the right password here';
    $('settings-export-encrypted').click();
    await vi.waitFor(() => expect(saved).toHaveLength(1), { timeout: 20000 });
    const backup = saved[0].text;
    toasts.length = 0;
    $<HTMLInputElement>('settings-backup-pw').value = 'the WRONG password here';
    $('settings-import-encrypted').click();
    await pickFile(picker()!, 'b.vaultbak', backup);
    await vi.waitFor(
      () => expect(toasts.some(([m]) => m.startsWith('Decryption failed'))).toBe(true),
      { timeout: 20000 },
    );
    expect(st.vault.api_keys).toHaveLength(2);
  });

  it('a picker with no file chosen is removed', async () => {
    $<HTMLInputElement>('settings-backup-pw').value = 'whatever-password';
    $('settings-import-encrypted').click();
    const inp = picker()!;
    Object.defineProperty(inp, 'files', { value: [], configurable: true });
    inp.dispatchEvent(new Event('change'));
    await flush();
    expect(picker()).toBeNull();
  });
});

describe('the card context menu and double-click', { timeout: 30000 }, () => {
  const card = (i: number) =>
    document.querySelector<HTMLElement>(`#card-grid .card[data-idx="${i}"]`)!;
  const open = (i = 0) => {
    card(i).dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true }));
    return [...document.querySelectorAll<HTMLElement>('#dropdown .dropdown-item')];
  };
  const choose = async (label: string, i = 0) => {
    open(i)
      .find((x) => x.textContent?.trim() === label)!
      .click();
    await flush(60);
  };

  it('right-clicking a card offers copy, edit, duplicate and delete for that entry', () => {
    expect(open().map((i) => i.textContent?.trim())).toEqual([
      'Copy Provider: Alpha',
      'Copy API Key',
      'Copy .env line',
      'Edit',
      'Duplicate',
      'Delete',
    ]);
  });

  it('right-clicking outside a card opens no menu, and the browser menu is left alone', () => {
    const ev = new MouseEvent('contextmenu', { bubbles: true, cancelable: true });
    $('card-grid').dispatchEvent(ev);
    expect(ev.defaultPrevented).toBe(false);
  });

  it('the copy items put the provider, the key and the .env line on the clipboard', async () => {
    await choose('Copy Provider: Alpha');
    expect(copied.pop()).toBe('Alpha');
    expect(toasts.some(([m]) => m === 'Copied ✓')).toBe(true);
    await choose('Copy API Key');
    expect(copied.pop()).toBe('k-a-longenough');
    await choose('Copy .env line');
    expect(copied.pop()).toContain('k-a-longenough');
  });

  it('an entry with a secret also offers to copy it', async () => {
    st.vault.api_keys[0].api_secret = 'the-secret-half';
    expect(open().map((i) => i.textContent?.trim())).toContain('Copy Secret');
    await choose('Copy Secret');
    expect(copied.pop()).toBe('the-secret-half');
  });

  it('Edit, Duplicate and Delete act on the entry that was right-clicked', async () => {
    await choose('Edit', 1);
    expect($<HTMLInputElement>('f-provider').value).toBe('Beta');
    $('modal-cancel').click();
    await choose('Duplicate', 1);
    expect(st.vault.api_keys).toHaveLength(3);
    await choose('Delete', 0);
    expect(st.vault.api_keys.some((e) => e.provider === 'Alpha')).toBe(false);
  });

  it('a menu item for an entry that has gone since the menu opened says so and does nothing', async () => {
    const items = open();
    st.vault.api_keys.splice(0, 1);
    items.find((x) => x.textContent?.trim() === 'Edit')!.click();
    await flush();
    expect(toasts[toasts.length - 1]).toEqual(['That secret no longer exists', 'err']);
  });

  it('double-clicking a card opens it for editing; double-clicking empty space does not', async () => {
    card(1).dispatchEvent(new MouseEvent('dblclick', { bubbles: true }));
    await flush();
    expect($<HTMLInputElement>('f-provider').value).toBe('Beta');
    $('modal-cancel').click();
    $('card-grid').dispatchEvent(new MouseEvent('dblclick', { bubbles: true }));
    expect($('modal-overlay').classList.contains('open')).toBe(false);
  });
});
