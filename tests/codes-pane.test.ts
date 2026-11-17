// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
// Tools, Recovery codes (R36). Parsing and adding codes happen in Rust; these tests stand in for it
// with a recording stub and check what the pane stores, shows and sends.
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { initCodesPane, paintCodes } from '../src/ts/codes-pane';
import { listCodes } from '../src/ts/recovery-codes';
import { st, switchTool } from '../src/ts/state';
import { mountToolsPanes } from '../src/ts/tools-markup';
import { loadRealIndexHtml, makeVault, resetState } from './helpers';

const toasts: string[] = [];
let confirmAnswer = true;
let duringConfirm: (() => void) | null = null;
vi.mock('../src/ts/utils', async (importOriginal) => {
  const real = await importOriginal<typeof import('../src/ts/utils')>();
  return {
    ...real,
    showToast: (m: string) => toasts.push(m),
    showConfirm: async () => {
      duringConfirm?.();
      return confirmAnswer;
    },
  };
});

const $ = <T extends HTMLElement = HTMLElement>(id: string) => document.getElementById(id) as T;
const tick = () => new Promise((r) => setTimeout(r, 0));

let calls: { cmd: string; args: Record<string, unknown> }[] = [];
let answer: (cmd: string, args: Record<string, unknown>) => unknown;
let saves = 0;

function stubTauri() {
  (window as unknown as { __TAURI__: unknown }).__TAURI__ = {
    core: {
      invoke: async (cmd: string, args: Record<string, unknown>) => {
        calls.push({ cmd, args });
        return answer(cmd, args);
      },
    },
  };
}

const codes = () => st.vault.api_keys[0].extra_vars?.find((v) => v.key === 'codes')?.value;

beforeEach(async () => {
  resetState(st);
  loadRealIndexHtml();
  mountToolsPanes();
  toasts.length = 0;
  calls = [];
  saves = 0;
  confirmAnswer = true;
  duringConfirm = null;
  answer = () => null;
  st.vault = makeVault({
    api_keys: [
      {
        id: 'c1',
        provider: 'GitHub codes',
        secretType: 'recovery_codes',
        extra_vars: [{ key: 'codes', value: 'aaaa-1111\nbbbb-2222\ncccc-3333\tUSED 2026-11-01' }],
      } as never,
      { id: 'p1', provider: 'Not codes', secretType: 'password' } as never,
    ],
  });
  st.vaultOpen = true;
  st.store = {
    save: async () => {
      saves += 1;
    },
    isRemote: false,
  } as never;
  stubTauri();
  initCodesPane();
  await tick();
});

afterEach(() => {
  delete (window as unknown as { __TAURI__?: unknown }).__TAURI__;
});

describe('the pane', () => {
  it('has a nav button, labelled controls and lists only recovery codes entries', () => {
    expect(document.querySelector('.tool-nav-btn[data-tool="codes"]')).not.toBeNull();
    const pane = $('tool-codes');
    for (const c of pane.querySelectorAll<HTMLElement>('input, select, textarea')) {
      const labelled =
        c.getAttribute('aria-label') ||
        (c.id && pane.querySelector(`label[for="${c.id}"]`)) ||
        c.closest('label');
      expect(labelled, c.id).toBeTruthy();
    }
    expect([...$<HTMLSelectElement>('cod-entry').options].map((o) => o.text)).toEqual([
      'GitHub codes',
    ]);
  });

  it('opens through the tool switcher', async () => {
    switchTool('codes');
    await tick();
    expect($('tool-codes').style.display).toBe('');
    expect($('cod-status').textContent).toBe('2 of 3 unused');
  });

  it('masks the codes until asked, and shows used ones struck through with their date', () => {
    expect($('cod-list').textContent).not.toContain('aaaa-1111');
    expect($('cod-list').querySelectorAll('.cod-code.used')).toHaveLength(1);
    expect($('cod-list').textContent).toContain('used 2026-11-01');
    $<HTMLInputElement>('cod-reveal').checked = true;
    $('cod-reveal').dispatchEvent(new Event('change'));
    expect($('cod-list').textContent).toContain('aaaa-1111');
  });

  it('says so when there is no recovery codes entry', () => {
    st.vault.api_keys = [];
    paintCodes();
    expect($('cod-status').textContent).toContain('No recovery codes entry yet');
    expect($('cod-list').children).toHaveLength(0);
  });

  it('warns at two codes left and disables marking at none', () => {
    expect($('cod-status').className).toContain('err');
    st.vault.api_keys[0].extra_vars![0].value = 'aaaa\tUSED x';
    paintCodes();
    expect($<HTMLButtonElement>('cod-use').disabled).toBe(true);
  });

  it('follows the account chosen in the list', () => {
    st.vault.api_keys.push({
      id: 'c2',
      provider: 'Google codes',
      secretType: 'recovery_codes',
      extra_vars: [{ key: 'codes', value: 'zzzz-9999' }],
    } as never);
    paintCodes();
    $<HTMLSelectElement>('cod-entry').value = 'c2';
    $('cod-entry').dispatchEvent(new Event('change'));
    expect($('cod-status').textContent).toBe('1 of 1 unused');
  });
});

describe('spending a code', () => {
  it('marks the next code used after a question and saves', async () => {
    $('cod-use').click();
    await tick();
    expect(codes()).toContain('aaaa-1111\tUSED ');
    expect(saves).toBe(1);
    expect(toasts.at(-1)).toBe('1 recovery codes left');
  });

  it('spends nothing when the question is declined', async () => {
    confirmAnswer = false;
    $('cod-use').click();
    await tick();
    expect(codes()).not.toContain('aaaa-1111\tUSED');
    expect(saves).toBe(0);
  });

  it('reports a failure to mark and saves nothing', async () => {
    st.vault.api_keys[0].extra_vars![0].value = 'aaaa-1111\tUSED x';
    $('cod-use').disabled = false;
    $('cod-use').click();
    await tick();
    expect(toasts.at(-1)).toContain('no unused codes remain');
    expect(saves).toBe(0);
  });

  it('marks the entry that is in the vault after the question, not a stale one', async () => {
    const before = st.vault.api_keys[0];
    st.vault.api_keys[0] = {
      ...before,
      extra_vars: [{ key: 'codes', value: 'new-1\nnew-2' }],
    } as never;
    $('cod-use').click();
    await tick();
    expect(codes()).toBe(`new-1\tUSED ${new Date().toISOString().slice(0, 10)}\nnew-2`);
  });

  it('does nothing when the entry vanished while the question was open', async () => {
    duringConfirm = () => {
      st.vault.api_keys = [];
    };
    $('cod-use').click();
    await tick();
    expect(saves).toBe(0);
  });
});

describe('restoring and importing (Rust does the work)', () => {
  it('asks Rust to put a used code back and stores its answer', async () => {
    answer = (cmd) => (cmd === 'codes_unmark' ? 'aaaa-1111\nbbbb-2222\ncccc-3333' : null);
    $('cod-list').querySelector<HTMLElement>('[data-action="cod-restore"]')!.click();
    await tick();
    expect(calls[0]).toMatchObject({ cmd: 'codes_unmark', args: { code: 'cccc-3333' } });
    expect(codes()).toBe('aaaa-1111\nbbbb-2222\ncccc-3333');
    expect(saves).toBe(1);
  });

  it('shows the reason when a restore is refused', async () => {
    answer = (cmd) => {
      if (cmd === 'codes_unmark') throw new Error('that code is not marked used');
      return null;
    };
    $('cod-list').querySelector<HTMLElement>('[data-action="cod-restore"]')!.click();
    await tick();
    expect(toasts.at(-1)).toContain('not marked used');
    expect(saves).toBe(0);
  });

  it('adds pasted codes and clears the box', async () => {
    answer = (cmd) =>
      cmd === 'codes_parse'
        ? ['dddd-4444', 'eeee-5555']
        : cmd === 'codes_add'
          ? { value: 'aaaa-1111\nbbbb-2222\ndddd-4444\neeee-5555', added: 2 }
          : null;
    $<HTMLTextAreaElement>('cod-paste').value = '1. dddd-4444\n2. eeee-5555';
    $('cod-import').click();
    await tick();
    expect(calls.map((c) => c.cmd)).toEqual(['codes_parse', 'codes_add']);
    expect(codes()).toContain('dddd-4444');
    expect($<HTMLTextAreaElement>('cod-paste').value).toBe('');
    expect(toasts.at(-1)).toBe('2 added (2 found)');
  });

  it('adds the variable when the entry has none', async () => {
    st.vault.api_keys[0].extra_vars = undefined;
    answer = (cmd) =>
      cmd === 'codes_parse' ? ['x-1'] : cmd === 'codes_add' ? { value: 'x-1', added: 1 } : null;
    $<HTMLTextAreaElement>('cod-paste').value = 'x-1';
    $('cod-import').click();
    await tick();
    expect(codes()).toBe('x-1');
  });

  it('says when the text holds no codes and changes nothing', async () => {
    answer = (cmd) => (cmd === 'codes_parse' ? [] : null);
    $<HTMLTextAreaElement>('cod-paste').value = 'nothing here';
    $('cod-import').click();
    await tick();
    expect(toasts.at(-1)).toBe('No recovery codes found in that text');
    expect(saves).toBe(0);
  });

  it('shows a Rust error from an import', async () => {
    answer = () => {
      throw new Error('boom');
    };
    $<HTMLTextAreaElement>('cod-paste').value = 'x';
    $('cod-import').click();
    await tick();
    expect(toasts.at(-1)).toContain('boom');
  });

  it('needs the desktop app to restore or import', async () => {
    delete (window as unknown as { __TAURI__?: unknown }).__TAURI__;
    $('cod-import').click();
    $('cod-list').querySelector<HTMLElement>('[data-action="cod-restore"]')!.click();
    await tick();
    expect(toasts).toEqual(['Needs the desktop app.', 'Needs the desktop app.']);
  });

  it('does nothing without an account selected', async () => {
    st.vault.api_keys = [];
    paintCodes();
    $('cod-import').click();
    $('cod-print').click();
    await tick();
    expect(calls).toHaveLength(0);
  });
});

describe('the printable sheet', () => {
  it('lists the unused codes, prints, and empties the sheet again', () => {
    const seen: string[] = [];
    const print = vi.spyOn(window, 'print').mockImplementation(() => {
      seen.push($('cod-sheet').textContent ?? '');
      expect(document.body.classList.contains('printing-codes')).toBe(true);
    });
    $('cod-print').click();
    expect(seen[0]).toBe('GitHub codes\n1. aaaa-1111\n2. bbbb-2222');
    expect($('cod-sheet').textContent).toBe('');
    expect(document.body.classList.contains('printing-codes')).toBe(false);
    print.mockRestore();
  });
});

describe('listCodes', () => {
  it('returns each code with its used date or null', () => {
    expect(listCodes(st.vault.api_keys[0] as never)).toEqual([
      { code: 'aaaa-1111', used: null },
      { code: 'bbbb-2222', used: null },
      { code: 'cccc-3333', used: '2026-11-01' },
    ]);
    expect(listCodes({ extra_vars: undefined })).toEqual([]);
  });
});
