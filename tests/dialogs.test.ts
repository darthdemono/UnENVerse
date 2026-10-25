// SPDX-License-Identifier: PolyForm-Strict-1.0.0
// Copyright Jubair Hasan.
/**
 * The app's own dialogs (confirm, prompt, password prompt, large prompt) and the
 * file-saving helper.
 *
 * These stand in for the native `confirm()`/`prompt()`, which WebKitGTK renders
 * badly or suppresses. So the contract is the native one: OK or Enter says yes,
 * Cancel, Escape or a click outside says no, nothing lingers afterwards, and a
 * secret typed into a password prompt is neither trimmed nor left behind.
 */
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import {
  showConfirm,
  showPrompt,
  showPasswordPrompt,
  showPromptLarge,
  saveFile,
  clipboardWrite,
  onClipboardWrite,
  execCopy,
  errorMessage,
} from '../src/ts/utils';
import { loadRealIndexHtml } from './helpers';

const $ = <T extends HTMLElement = HTMLElement>(id: string) => document.getElementById(id) as T;
const key = (k: string, init: KeyboardEventInit = {}) =>
  document.dispatchEvent(new KeyboardEvent('keydown', { key: k, bubbles: true, ...init }));

beforeEach(() => {
  loadRealIndexHtml();
});
afterEach(() => {
  vi.restoreAllMocks();
  delete (window as any).__TAURI__;
  onClipboardWrite(null);
});

describe('showConfirm', () => {
  it('shows the question, answers yes on OK or Enter, no on Cancel, Escape or the backdrop', async () => {
    const ask = () => {
      const p = showConfirm('Delete it?');
      expect($('confirm-message').textContent).toBe('Delete it?');
      expect($('confirm-overlay').classList.contains('open')).toBe(true);
      return p;
    };
    let p = ask();
    $('confirm-ok').click();
    expect(await p).toBe(true);
    expect($('confirm-overlay').classList.contains('open')).toBe(false);
    p = ask();
    key('Enter');
    expect(await p).toBe(true);
    p = ask();
    $('confirm-cancel').click();
    expect(await p).toBe(false);
    p = ask();
    key('Escape');
    expect(await p).toBe(false);
    p = ask();
    $('confirm-overlay').click();
    expect(await p).toBe(false);
  });

  it('ignores other keys and clicks inside the box, and leaves no listener behind', async () => {
    const p = showConfirm('q');
    key('a');
    $('confirm-message').click();
    expect($('confirm-overlay').classList.contains('open')).toBe(true);
    key('Escape');
    expect(await p).toBe(false);
    const next = showConfirm('again');
    key('Escape');
    expect(await next).toBe(false);
  });
});

describe('showPrompt', () => {
  it('returns the trimmed text on OK or Enter, pre-filled with the default', async () => {
    let p = showPrompt('Name?', 'default');
    expect($<HTMLInputElement>('prompt-input').value).toBe('default');
    $<HTMLInputElement>('prompt-input').value = '  Joy  ';
    $('prompt-ok').click();
    expect(await p).toBe('Joy');
    p = showPrompt('Name?');
    $<HTMLInputElement>('prompt-input').value = ' x ';
    key('Enter');
    expect(await p).toBe('x');
  });

  it('is null when empty, cancelled, escaped or clicked away from', async () => {
    let p = showPrompt('Name?');
    $('prompt-ok').click();
    expect(await p).toBeNull();
    p = showPrompt('Name?', 'abc');
    $('prompt-cancel').click();
    expect(await p).toBeNull();
    p = showPrompt('Name?', 'abc');
    key('Escape');
    expect(await p).toBeNull();
    p = showPrompt('Name?', 'abc');
    $('prompt-overlay').click();
    expect(await p).toBeNull();
    p = showPrompt('Name?');
    key('Enter');
    expect(await p).toBeNull();
  });
});

describe('showPasswordPrompt', () => {
  it('masks the input while open, keeps edge spaces, and clears itself afterwards', async () => {
    const p = showPasswordPrompt('Password?');
    const input = $<HTMLInputElement>('prompt-input');
    expect(input.type).toBe('password');
    input.value = '  spaced pw  ';
    $('prompt-ok').click();
    expect(await p).toBe('  spaced pw  ');
    expect(input.type).toBe('text');
    expect(input.value).toBe('');
  });

  it('cancelling or submitting nothing is null and still wipes the field', async () => {
    for (const finish of [
      () => key('Escape'),
      () => $('prompt-cancel').click(),
      () => $('prompt-overlay').click(),
      () => key('Enter'),
      () => $('prompt-ok').click(),
    ]) {
      const p = showPasswordPrompt('Password?');
      finish();
      expect(await p).toBeNull();
      expect($<HTMLInputElement>('prompt-input').value).toBe('');
    }
    const p = showPasswordPrompt('Password?');
    $<HTMLInputElement>('prompt-input').value = 'enter-submits';
    key('Enter');
    expect(await p).toBe('enter-submits');
  });
});

describe('showPromptLarge', () => {
  it('shows a text area, submits with Ctrl/Cmd+Enter or OK, and puts the layout back', async () => {
    let p = showPromptLarge('Edit:', 'line1\nline2');
    expect($<HTMLTextAreaElement>('prompt-textarea').value).toBe('line1\nline2');
    expect($('prompt-input').style.display).toBe('none');
    expect($('prompt-modal').style.maxWidth).toBe('640px');
    key('Enter');
    expect($('prompt-overlay').classList.contains('open')).toBe(true);
    $<HTMLTextAreaElement>('prompt-textarea').value = '  edited  ';
    key('Enter', { ctrlKey: true });
    expect(await p).toBe('edited');
    expect($('prompt-input').style.display).toBe('');
    expect($('prompt-textarea').style.display).toBe('none');
    expect($('prompt-modal').style.maxWidth).toBe('340px');
    p = showPromptLarge('Edit:');
    $<HTMLTextAreaElement>('prompt-textarea').value = 'ok';
    $('prompt-ok').click();
    expect(await p).toBe('ok');
    p = showPromptLarge('Edit:');
    key('Enter', { metaKey: true });
    expect(await p).toBeNull();
  });

  it('is null on cancel, escape or a click outside', async () => {
    for (const finish of [
      () => $('prompt-cancel').click(),
      () => key('Escape'),
      () => $('prompt-overlay').click(),
    ]) {
      const p = showPromptLarge('Edit:', 'text');
      finish();
      expect(await p).toBeNull();
    }
  });
});

describe('clipboard', () => {
  it('uses the async clipboard, tells the observer, and survives an observer that throws', async () => {
    const write = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, 'clipboard', {
      value: { writeText: write },
      configurable: true,
    });
    const seen: string[] = [];
    onClipboardWrite((t) => seen.push(t));
    await clipboardWrite('secret');
    expect(write).toHaveBeenCalledWith('secret');
    expect(seen).toEqual(['secret']);
    onClipboardWrite(() => {
      throw new Error('log failed');
    });
    await expect(clipboardWrite('again')).resolves.toBeUndefined();
    expect(write).toHaveBeenLastCalledWith('again');
  });

  it('falls back to copying through a hidden text area when the async clipboard refuses or is missing', async () => {
    const exec = vi.spyOn(document, 'execCommand').mockReturnValue(true);
    Object.defineProperty(navigator, 'clipboard', {
      value: { writeText: vi.fn().mockRejectedValue(new Error('denied')) },
      configurable: true,
    });
    await clipboardWrite('fallback');
    expect(exec).toHaveBeenCalledWith('copy');
    Object.defineProperty(navigator, 'clipboard', { value: undefined, configurable: true });
    await clipboardWrite('no api');
    expect(exec).toHaveBeenCalledTimes(2);
    expect(document.querySelector('textarea[style*="-9999px"]')).toBeNull();
  });

  it('execCopy removes its text area and reports a failed copy as an Error', async () => {
    vi.spyOn(document, 'execCommand').mockImplementation(() => {
      throw 'blocked' as unknown as Error;
    });
    await expect(execCopy('x')).rejects.toThrow('blocked');
    expect(document.querySelector('textarea[style*="-9999px"]')).toBeNull();
    vi.spyOn(document, 'execCommand').mockImplementation(() => {
      throw new Error('real error');
    });
    await expect(execCopy('x')).rejects.toThrow('real error');
  });
});

describe('saveFile', () => {
  it('in a plain browser downloads through an anchor and reports no path', async () => {
    const create = vi.fn(() => 'blob:x');
    const revoke = vi.fn();
    (URL as any).createObjectURL = create;
    (URL as any).revokeObjectURL = revoke;
    const click = vi.spyOn(HTMLAnchorElement.prototype, 'click').mockImplementation(() => {});
    vi.useFakeTimers();
    const r = await saveFile('data', 'out.txt', 'text/plain');
    expect(r).toEqual({ ok: true, path: null });
    expect(click).toHaveBeenCalled();
    expect(revoke).not.toHaveBeenCalled();
    vi.advanceTimersByTime(1100);
    expect(revoke).toHaveBeenCalledWith('blob:x');
    vi.useRealTimers();
  });

  it('in a plain browser reports a failure instead of throwing', async () => {
    (URL as any).createObjectURL = () => {
      throw new Error('no blobs');
    };
    expect(await saveFile('d', 'f')).toEqual({ ok: false, error: 'no blobs' });
  });

  it('in the desktop app writes a real file and says where, or why not', async () => {
    const invoke = vi.fn(async () => '/home/me/out.txt');
    (window as any).__TAURI__ = { core: { invoke } };
    expect(await saveFile('data', 'out.txt')).toEqual({ ok: true, path: '/home/me/out.txt' });
    expect(invoke).toHaveBeenCalledWith('write_export_file', {
      filename: 'out.txt',
      content: 'data',
    });
    invoke.mockResolvedValueOnce(undefined as never);
    expect(await saveFile('d', 'f')).toEqual({ ok: false, error: 'Export did not return a path' });
    invoke.mockRejectedValueOnce(new Error('disk full'));
    expect(await saveFile('d', 'f')).toEqual({ ok: false, error: 'disk full' });
    invoke.mockRejectedValueOnce('plain text' as unknown as Error);
    expect(await saveFile('d', 'f')).toEqual({ ok: false, error: 'plain text' });
  });
});

describe('errorMessage', () => {
  it('uses the message of an Error and the text of anything else', () => {
    expect(errorMessage(new Error('boom'))).toBe('boom');
    expect(errorMessage('plain')).toBe('plain');
    expect(errorMessage(42)).toBe('42');
  });
});

describe('clicks and keys that must not answer a dialog', () => {
  const inner = () => $('prompt-overlay').firstElementChild as HTMLElement;
  const cases: [string, () => Promise<string | null>][] = [
    ['prompt', () => showPrompt('Name?', 'x')],
    ['password prompt', () => showPasswordPrompt('Password?')],
    ['large prompt', () => showPromptLarge('Paste', 'x')],
  ];

  it.each(cases)(
    '%s: a click inside the dialog, or an unrelated key, leaves it open',
    async (_n, ask) => {
      const p = ask();
      inner().dispatchEvent(new MouseEvent('click', { bubbles: true }));
      key('a');
      expect($('prompt-overlay').classList.contains('open')).toBe(true);
      key('Escape');
      expect(await p).toBeNull();
    },
  );

  it('the large prompt does not answer on a bare Enter (that is a new line), only on Ctrl+Enter', async () => {
    const p = showPromptLarge('Paste');
    $<HTMLTextAreaElement>('prompt-textarea').value = '  hello  ';
    key('Enter');
    expect($('prompt-overlay').classList.contains('open')).toBe(true);
    key('Enter', { ctrlKey: true });
    expect(await p).toBe('hello');
  });

  it('the large prompt gives null for blank text, and the backdrop cancels it', async () => {
    let p = showPromptLarge('Paste');
    $<HTMLTextAreaElement>('prompt-textarea').value = '   ';
    $('prompt-ok').click();
    expect(await p).toBeNull();
    p = showPromptLarge('Paste');
    $('prompt-overlay').click();
    expect(await p).toBeNull();
  });

  it('a saved file that cannot be written reports the failure as a message, whatever was thrown', async () => {
    (window as any).__TAURI__ = {
      core: {
        invoke: async () => {
          throw 'plain string failure' as unknown as Error;
        },
      },
    };
    const r = await saveFile('x', 'a.txt');
    expect(r).toEqual({ ok: false, error: 'plain string failure' });
  });
});
