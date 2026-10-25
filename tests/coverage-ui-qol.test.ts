// SPDX-License-Identifier: PolyForm-Strict-1.0.0
// Copyright Jubair Hasan.
/** Branch coverage for ui-qol.ts: reveal, caps-lock hint, modal focus, strength, search history, times. */
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';

const flush = (ms = 5) => new Promise((r) => setTimeout(r, ms));

async function load(tauriCaps?: () => unknown) {
  vi.resetModules();
  if (tauriCaps)
    (window as any).__TAURI__ = {
      core: { invoke: async (cmd: string) => (cmd === 'caps_lock_state' ? tauriCaps() : null) },
    };
  else delete (window as any).__TAURI__;
  const mod = await import('../src/ts/ui-qol');
  const state = await import('../src/ts/state');
  return { mod, ...state };
}

beforeEach(() => {
  document.body.innerHTML = '';
});
afterEach(() => {
  vi.unstubAllGlobals();
  vi.useRealTimers();
  delete (window as any).__TAURI__;
});

describe('password reveal', () => {
  it('toggles an input, with or without an icon, and skips buttons with no input', async () => {
    const { mod } = await load();
    document.body.innerHTML = `
      <input id="a" type="password"><button data-reveal="a"><svg></svg></button>
      <input id="b" type="password"><button data-reveal="b"></button>
      <button data-reveal="missing"></button>`;
    mod.wireRevealButtons();
    const [bA, bB, bMissing] = [...document.querySelectorAll<HTMLButtonElement>('button')];
    bA.click();
    expect((document.getElementById('a') as HTMLInputElement).type).toBe('text');
    expect(bA.title).toBe('Hide password');
    bA.click();
    expect((document.getElementById('a') as HTMLInputElement).type).toBe('password');
    bB.click();
    expect((document.getElementById('b') as HTMLInputElement).type).toBe('text');
    expect(bMissing.onclick).toBeNull();
  });

  it('resets masked state with every missing piece tolerated', async () => {
    const { mod } = await load();
    mod.resetReveal('nope');
    document.body.innerHTML = `<input id="x" type="text">`;
    mod.resetReveal('x');
    expect((document.getElementById('x') as HTMLInputElement).type).toBe('password');
    document.body.innerHTML = `<input id="y" type="text"><button data-reveal="y"></button>`;
    mod.resetReveal('y');
    document.body.innerHTML = `<input id="z" type="text"><button data-reveal="z" class="active"><svg></svg></button>`;
    mod.resetReveal('z');
    expect(document.querySelector('button')!.classList.contains('active')).toBe(false);
  });
});

describe('caps lock hint', () => {
  const setup = () => {
    document.body.innerHTML = `<input id="pw"><div id="hint" style="display:none"></div>`;
    return {
      input: document.getElementById('pw') as HTMLInputElement,
      hint: document.getElementById('hint')!,
    };
  };
  const key = (type: string, init: KeyboardEventInit, mod?: boolean | 'absent') => {
    const e = new KeyboardEvent(type, init);
    if (mod === 'absent') Object.defineProperty(e, 'getModifierState', { value: undefined });
    else if (mod !== undefined) Object.defineProperty(e, 'getModifierState', { value: () => mod });
    return e;
  };

  it('derives the state from typed characters, ignoring chords and uncased keys', async () => {
    const { mod } = await load();
    const { input, hint } = setup();
    mod.wireCapsLockHint('pw', 'hint');
    input.onkeydown!(key('keydown', { key: 'A' }));
    expect(hint.style.display).toBe('flex');
    input.onkeydown!(key('keydown', { key: '5' }));
    input.onkeydown!(key('keydown', { key: 'Shift' }));
    input.onkeydown!(key('keydown', { key: 'a', ctrlKey: true }));
    expect(hint.style.display).toBe('flex');
    input.onkeydown!(key('keydown', { key: 'a' }));
    expect(hint.style.display).toBe('none');
    input.onkeydown!(key('keydown', { key: 'a', shiftKey: true }));
    expect(hint.style.display).toBe('flex');
    input.onkeydown!({ key: undefined } as any);
  });

  it('learns to trust a platform that agrees twice, then believes it for cheap paths', async () => {
    const { mod } = await load();
    const { input, hint } = setup();
    mod.resetCapsProbe();
    mod.wireCapsLockHint('pw', 'hint');
    input.onkeydown!(key('keydown', { key: 'A' }, true));
    input.onkeydown!(key('keydown', { key: 'B' }, true));
    // trusted: a digit now reads the platform's claim
    input.onkeydown!(key('keydown', { key: '1' }, false));
    expect(hint.style.display).toBe('none');
    input.onkeydown!(key('keydown', { key: '1' }, true));
    expect(hint.style.display).toBe('flex');
    // mousedown believes it too
    input.onmousedown!(key('mousedown', {}, false) as any);
    expect(hint.style.display).toBe('none');
    // the Caps Lock key itself
    input.onkeyup!(key('keyup', { key: 'CapsLock' }, true));
    expect(hint.style.display).toBe('flex');
    input.onkeyup!(key('keyup', { key: 'x' }, true));
    // blur voids the reading
    input.onblur!(new FocusEvent('blur'));
    expect(hint.style.display).toBe('none');
  });

  it('never trusts a platform that contradicts a typed character', async () => {
    const { mod } = await load();
    const { input, hint } = setup();
    mod.resetCapsProbe();
    mod.wireCapsLockHint('pw', 'hint');
    input.onkeydown!(key('keydown', { key: 'a' }, true)); // claims on, character says off
    input.onkeydown!(key('keydown', { key: 'b' }, true)); // already untrusted
    input.onkeydown!(key('keydown', { key: '1' }, true));
    expect(hint.style.display).toBe('none');
    input.onmousedown!(key('mousedown', {}, true) as any);
    expect(hint.style.display).toBe('none');
  });

  it('events without getModifierState carry no platform claim', async () => {
    const { mod } = await load();
    const { input, hint } = setup();
    mod.resetCapsProbe();
    mod.wireCapsLockHint('pw', 'hint');
    input.onkeydown!(key('keydown', { key: 'A' }, 'absent'));
    expect(hint.style.display).toBe('flex');
    // keyup of the Caps Lock key without a belief flips what was known, or leaves nothing known alone
    input.onkeyup!(key('keyup', { key: 'CapsLock' }, 'absent'));
    expect(hint.style.display).toBe('none');
    input.onblur!(new FocusEvent('blur'));
    input.onkeyup!(key('keyup', { key: 'CapsLock' }, 'absent'));
    expect(hint.style.display).toBe('none');
  });

  it('reads the OS state on focus inside the desktop app and ignores stale or failing answers', async () => {
    let answer: unknown = true;
    const { mod } = await load(() => {
      if (answer === 'throw') throw new Error('unsupported');
      return answer;
    });
    const { input, hint } = setup();
    mod.wireCapsLockHint('pw', 'hint');
    input.dispatchEvent(new FocusEvent('focus'));
    await flush();
    expect(hint.style.display).toBe('flex');
    answer = null;
    input.dispatchEvent(new FocusEvent('focus'));
    await flush();
    answer = 'throw';
    input.dispatchEvent(new FocusEvent('focus'));
    await flush();
    // Leaving the field before the answer arrives discards it.
    answer = false;
    input.dispatchEvent(new FocusEvent('focus'));
    input.dispatchEvent(new FocusEvent('blur'));
    await flush();
    expect(hint.style.display).toBe('none');
    // The Caps Lock key re-reads the OS state while focused.
    input.dispatchEvent(new FocusEvent('focus'));
    input.onkeyup!(key('keyup', { key: 'CapsLock' }));
    await flush();
  });
});

describe('modal focus', () => {
  const overlay = (inner = '<button id="b1">1</button><button id="b2">2</button>') => {
    const o = document.createElement('div');
    o.className = 'modal-overlay';
    o.innerHTML = inner;
    document.body.appendChild(o);
    return o;
  };
  const tab = (shift = false) => {
    const e = new KeyboardEvent('keydown', {
      key: 'Tab',
      shiftKey: shift,
      cancelable: true,
      bubbles: true,
    });
    document.dispatchEvent(e);
    return e;
  };

  it('moves focus in, traps Tab both ways, pulls stray focus back and restores on close', async () => {
    const { mod } = await load();
    document.body.innerHTML = '<button id="opener">open</button>';
    (document.getElementById('opener') as HTMLElement).focus();
    mod.wireModalFocus();
    mod.wireModalFocus();
    const o = overlay(
      '<div hidden><button id="ancestor-hidden">x</button></div><button id="b1">1</button><button id="b2">2</button><button hidden id="bh">h</button><input disabled><div style="display:none"><button id="bd">d</button></div>',
    );
    o.classList.add('open');
    await flush(20);
    expect(document.activeElement!.id).toBe('b1');

    (document.getElementById('b1') as HTMLElement).focus();
    expect(tab(true).defaultPrevented).toBe(true);
    expect(document.activeElement!.id).toBe('b2');
    expect(tab(false).defaultPrevented).toBe(true);
    expect(document.activeElement!.id).toBe('b1');
    (document.getElementById('b2') as HTMLElement).focus();
    (document.getElementById('b1') as HTMLElement).focus();
    expect(tab(false).defaultPrevented).toBe(false);

    (document.getElementById('opener') as HTMLElement).focus();
    expect(tab(false).defaultPrevented).toBe(true);
    (document.getElementById('opener') as HTMLElement).focus();
    expect(tab(true).defaultPrevented).toBe(true);
    expect(document.activeElement!.id).toBe('b2');

    const other = new KeyboardEvent('keydown', { key: 'Enter', cancelable: true });
    document.dispatchEvent(other);
    expect(other.defaultPrevented).toBe(false);

    o.classList.remove('open');
    await flush(20);
    expect(document.activeElement!.id).toBe('opener');
  });

  it('does nothing without an open modal, with no focusable items, or when the opener is gone', async () => {
    const { mod } = await load();
    mod.wireModalFocus();
    expect(tab().defaultPrevented).toBe(false);
    const empty = overlay('<p>no controls</p>');
    empty.classList.add('open');
    await flush(20);
    expect(tab().defaultPrevented).toBe(false);
    empty.remove();
    await flush(20);

    document.body.innerHTML = '<button id="opener">o</button>';
    (document.getElementById('opener') as HTMLElement).focus();
    const o = overlay();
    o.classList.add('open');
    await flush(20);
    document.getElementById('opener')!.remove();
    o.classList.remove('open');
    await flush(20);
  });

  it('a modal that already holds focus keeps it; the deferred focus is skipped when the modal closed', async () => {
    const { mod } = await load();
    mod.wireModalFocus();
    const o = overlay('<input id="own"><button id="b1">1</button>');
    (document.getElementById('own') as HTMLElement).focus();
    o.classList.add('open');
    await flush(20);
    expect(document.activeElement!.id).toBe('own');

    const p = overlay();
    p.classList.add('open');
    p.classList.remove('open');
    o.classList.remove('open');
    await flush(20);
  });
});

describe('password strength', () => {
  it('scores every band', async () => {
    const { mod } = await load();
    expect(mod.passwordStrength('').score).toBe(0);
    expect(mod.passwordStrength('aaaaaaaaaaaa').score).toBe(0);
    expect(mod.passwordStrength('abc').label).toBe('Weak');
    expect(mod.passwordStrength('abcdefghijkl').score).toBe(1);
    expect(mod.passwordStrength('Abcdefghijk1').label).toBe('Fair');
    expect(mod.passwordStrength('Abcdefghijklmnop1').label).toBe('Good');
    expect(mod.passwordStrength('Abcdefghijklmnopqrstuvwx1!').label).toBe('Strong');
  });

  it('updates the meter as the user types, and needs all four elements', async () => {
    const { mod } = await load();
    document.body.innerHTML =
      '<input id="p"><div id="w"><span id="w-fill"></span><span id="w-label"></span></div>';
    mod.wirePasswordStrength('p', 'w');
    const input = document.getElementById('p') as HTMLInputElement;
    expect(document.getElementById('w')!.style.display).toBe('none');
    input.value = 'Abcdefghijklmnop1';
    input.oninput!(new InputEvent('input'));
    expect(document.getElementById('w')!.style.display).toBe('flex');
    expect(document.getElementById('w-label')!.textContent).toBe('Good');
    document.body.innerHTML = '<input id="p"><div id="w"></div>';
    mod.wirePasswordStrength('p', 'w');
    mod.wirePasswordStrength('nope', 'w');
  });
});

describe('search history', () => {
  const panelHtml = '<input id="search"><div id="search-history"></div>';

  it('closes and opens with nothing in the way', async () => {
    const { mod, Settings } = await load();
    mod.closeSearchHistory();
    expect(mod.openSearchHistory(() => {})).toBe(false);
    document.body.innerHTML = panelHtml;
    Settings.set('recentSearches', undefined as any);
    expect(mod.openSearchHistory(() => {})).toBe(false);
    Settings.set('recentSearches', []);
    expect(mod.openSearchHistory(() => {})).toBe(false);
    expect(document.getElementById('search-history')!.style.display).toBe('none');
  });

  it('lists recent searches, picks one, clears, and ignores clicks elsewhere', async () => {
    const { mod, Settings } = await load();
    document.body.innerHTML = panelHtml;
    Settings.set('recentSearches', ['alpha', '<b>x</b>']);
    const picked: string[] = [];
    expect(mod.openSearchHistory((q) => picked.push(q))).toBe(true);
    const panel = document.getElementById('search-history')!;
    expect(panel.querySelector('b')).toBeNull();
    panel.querySelector<HTMLElement>('.search-history-head')!.click();
    expect(picked).toEqual([]);
    panel.querySelector<HTMLElement>('[data-recent="alpha"]')!.click();
    expect(picked).toEqual(['alpha']);
    mod.openSearchHistory(() => {});
    panel.querySelector<HTMLElement>('[data-clear-history]')!.click();
    expect(Settings.get('recentSearches')).toEqual([]);
  });

  it('wires focus, input, Enter, Escape and blur on the search box', async () => {
    vi.useFakeTimers();
    const { mod, Settings } = await load();
    mod.wireSearchHistory(() => {});
    document.body.innerHTML = panelHtml;
    mod.wireSearchHistory(() => {});
    const input = document.getElementById('search') as HTMLInputElement;
    const panel = document.getElementById('search-history')!;
    Settings.set('recentSearches', ['one']);
    input.dispatchEvent(new FocusEvent('focus'));
    expect(panel.style.display).toBe('block');
    input.value = 'q';
    input.dispatchEvent(new Event('input'));
    expect(panel.style.display).toBe('none');
    input.value = '';
    input.dispatchEvent(new Event('input'));
    input.value = 'typed';
    input.dispatchEvent(new FocusEvent('focus'));
    expect(panel.style.display).toBe('none');
    input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter' }));
    expect(Settings.get('recentSearches')![0]).toBe('typed');
    input.dispatchEvent(new KeyboardEvent('keydown', { key: 'a' }));
    panel.style.display = 'block';
    input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
    expect(panel.style.display).toBe('none');
    panel.style.display = 'block';
    input.dispatchEvent(new FocusEvent('blur'));
    vi.advanceTimersByTime(200);
    expect(panel.style.display).toBe('none');
  });
});

describe('relative and forward times', () => {
  const ago = (s: number) => new Date(Date.now() - s * 1000).toISOString();
  const ahead = (s: number) => new Date(Date.now() + s * 1000).toISOString();

  it('timeUntil: blank, bad, bare date, expired and upcoming bands', async () => {
    const { mod } = await load();
    expect(mod.timeUntil(null)).toBe('');
    expect(mod.timeUntil('nonsense')).toBe('');
    expect(mod.timeUntil('2999-01-01')).toMatch(/^expires in \d+d$/);
    expect(mod.timeUntil(ago(30))).toBe('expired 1 min ago');
    expect(mod.timeUntil(ago(7200))).toBe('expired 2h ago');
    expect(mod.timeUntil(ago(3 * 86400))).toBe('expired 3d ago');
    expect(mod.timeUntil(ahead(120))).toBe('expires in 2 min');
    expect(mod.timeUntil(ahead(7200))).toBe('expires in 2h');
    expect(mod.timeUntil(ahead(5 * 86400))).toBe('expires in 5d');
  });

  it('relativeTime: never, just now, minutes, hours, days, months, years', async () => {
    const { mod } = await load();
    expect(mod.relativeTime(undefined)).toBe('never connected');
    expect(mod.relativeTime('bad')).toBe('never connected');
    expect(mod.relativeTime(ago(10))).toBe('just now');
    expect(mod.relativeTime(ahead(500))).toBe('just now');
    expect(mod.relativeTime(ago(60))).toBe('1 minute ago');
    expect(mod.relativeTime(ago(600))).toBe('10 minutes ago');
    expect(mod.relativeTime(ago(3600))).toBe('1 hour ago');
    expect(mod.relativeTime(ago(5 * 3600))).toBe('5 hours ago');
    expect(mod.relativeTime(ago(86400))).toBe('yesterday');
    expect(mod.relativeTime(ago(5 * 86400))).toBe('5 days ago');
    expect(mod.relativeTime(ago(35 * 86400))).toBe('1 month ago');
    expect(mod.relativeTime(ago(90 * 86400))).toBe('3 months ago');
    expect(mod.relativeTime(ago(400 * 86400))).toBe('1 year ago');
    expect(mod.relativeTime(ago(800 * 86400))).toBe('2 years ago');
  });
});

describe('close guard', () => {
  it('blocks closing only while an undo is pending', async () => {
    const { mod, st } = await load();
    mod.wireCloseGuard();
    st.undoStack.length = 0;
    const quiet = new Event('beforeunload', { cancelable: true });
    window.dispatchEvent(quiet);
    expect(quiet.defaultPrevented).toBe(false);
    st.undoStack.push({} as any);
    const busy = new Event('beforeunload', { cancelable: true });
    window.dispatchEvent(busy);
    expect(busy.defaultPrevented).toBe(true);
    st.undoStack.length = 0;
  });
});
