// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { loadRealIndexHtml } from './helpers';
vi.mock('../src/ts/lock', () => ({ lockVault: vi.fn() }));

import { lockVault } from '../src/ts/lock';
import {
  closePalette,
  filterCommands,
  isPaletteOpen,
  openPalette,
  paletteCommands,
  scoreCommand,
  type Command,
} from '../src/ts/palette';

const key = (el: Element, k: string) =>
  el.dispatchEvent(new KeyboardEvent('keydown', { key: k, bubbles: true, cancelable: true }));

function commands(run: () => void): Command[] {
  return [
    { id: 'a', label: 'Add a secret', hint: 'Ctrl+N', run },
    { id: 'b', label: 'Open settings', run: () => {} },
    { id: 'c', label: 'Lock the vault', run: () => {} },
  ];
}

describe('scoring', () => {
  it('needs the letters in order, and prefers word starts and runs', () => {
    expect(scoreCommand('zz', 'Add a secret')).toBe(-1);
    expect(scoreCommand('', 'anything')).toBe(0);
    expect(scoreCommand('set', 'Open settings')).toBeGreaterThan(
      scoreCommand('set', 'Reset a token'),
    );
    expect(scoreCommand('os', 'Open settings')).toBeGreaterThan(
      scoreCommand('os', 'Lock the vault so'),
    );
  });

  it('filters and ranks, dropping what does not match', () => {
    const list = commands(() => {});
    expect(filterCommands('lock', list).map((c) => c.id)).toEqual(['c']);
    expect(filterCommands('', list)).toHaveLength(3);
    expect(filterCommands('xyz', list)).toEqual([]);
  });
});

describe('the palette', () => {
  beforeEach(() => {
    loadRealIndexHtml();
  });
  afterEach(() => {
    closePalette();
    document.getElementById('palette-overlay')?.remove();
  });

  it('opens with focus in a named dialog and lists every command', () => {
    const opener = document.getElementById('add-btn') as HTMLElement;
    opener.focus();
    openPalette(commands(() => {}));
    expect(isPaletteOpen()).toBe(true);
    const dialog = document.querySelector('[role="dialog"][aria-label="Command palette"]');
    expect(dialog).not.toBeNull();
    expect(document.activeElement?.id).toBe('palette-input');
    expect(document.querySelectorAll('[role="option"]')).toHaveLength(3);
    closePalette();
    expect(isPaletteOpen()).toBe(false);
    expect(document.activeElement).toBe(opener);
    closePalette(); // closing twice is harmless
  });

  it('narrows as you type, moves with the arrows, and runs the highlighted command on Enter', () => {
    const run = vi.fn();
    openPalette(commands(run));
    const input = document.getElementById('palette-input') as HTMLInputElement;
    input.value = 'secret';
    input.dispatchEvent(new Event('input'));
    expect(document.querySelectorAll('[role="option"]')).toHaveLength(1);
    expect(input.getAttribute('aria-activedescendant')).toBe('palette-opt-0');
    key(input, 'Enter');
    expect(run).toHaveBeenCalledOnce();
    expect(isPaletteOpen()).toBe(false);
  });

  it('wraps the highlight in both directions and says so when nothing matches', () => {
    openPalette(commands(() => {}));
    const input = document.getElementById('palette-input') as HTMLInputElement;
    key(input, 'ArrowUp');
    expect(document.querySelector('.palette-opt.active')?.textContent).toContain('Lock the vault');
    key(input, 'ArrowDown');
    expect(document.querySelector('.palette-opt.active')?.textContent).toContain('Add a secret');
    input.value = 'qqq';
    input.dispatchEvent(new Event('input'));
    expect(document.querySelector('.palette-empty')?.textContent).toBe('No matching command');
    expect(input.hasAttribute('aria-activedescendant')).toBe(false);
    key(input, 'x'); // an ordinary key is left for the input
    expect(isPaletteOpen()).toBe(true);
    key(input, 'ArrowDown'); // nothing to move through
    key(input, 'Enter'); // nothing to run, stays open
    expect(isPaletteOpen()).toBe(true);
  });

  it('closes on Escape and on a click outside the box, and runs a clicked row', () => {
    const run = vi.fn();
    openPalette(commands(run));
    const input = document.getElementById('palette-input') as HTMLInputElement;
    key(input, 'Escape');
    expect(isPaletteOpen()).toBe(false);
    openPalette(commands(run));
    document.getElementById('palette-overlay')!.click();
    expect(isPaletteOpen()).toBe(false);
    openPalette(commands(run));
    (document.querySelector('[data-palette-index="0"]') as HTMLElement).click();
    expect(run).toHaveBeenCalledOnce();
    openPalette(commands(run));
    (document.querySelector('.palette-box') as HTMLElement).click(); // inside the box: stays
    expect(isPaletteOpen()).toBe(true);
  });

  it('escapes command labels', () => {
    openPalette([{ id: 'x', label: '<img src=x onerror=alert(1)>', run: () => {} }]);
    expect(document.querySelector('#palette-list img')).toBeNull();
    expect(document.querySelector('#palette-list')?.textContent).toContain('<img');
  });

  it('re-attaches itself if the page replaced the body', () => {
    openPalette(commands(() => {}));
    document.getElementById('palette-overlay')!.remove();
    openPalette(commands(() => {}));
    expect(document.getElementById('palette-overlay')).not.toBeNull();
  });
});

describe('the built-in commands', () => {
  beforeEach(() => {
    loadRealIndexHtml();
  });

  it('covers navigation and the top actions, each with a unique id', () => {
    const list = paletteCommands();
    const ids = list.map((c) => c.id);
    expect(new Set(ids).size).toBe(ids.length);
    for (const id of [
      'go-secrets',
      'go-tools',
      'add',
      'settings',
      'lock',
      'search',
      'clear-filters',
    ]) {
      expect(ids).toContain(id);
    }
  });

  it('shortcuts, sidebar and search commands act on the page', () => {
    const list = paletteCommands();
    const run = (id: string) => list.find((c) => c.id === id)!.run();
    run('shortcuts');
    expect(document.getElementById('shortcuts-overlay')!.classList.contains('open')).toBe(true);
    const toggle = document.getElementById('sidebar-toggle')!;
    const clicked = vi.fn();
    toggle.addEventListener('click', clicked);
    run('sidebar');
    expect(clicked).toHaveBeenCalledOnce();
    run('search');
    expect(document.activeElement?.id).toBe('search');
    run('clear-filters');
    run('go-tools');
    expect(document.getElementById('tools-panel')).not.toBeNull();
  });

  it('navigation commands switch the panel the activity bar would', () => {
    const list = paletteCommands();
    for (const [id, tab] of [
      ['go-tools', 'activity-tab-tools'],
      ['go-remote', 'activity-tab-remote'],
      ['go-users', 'activity-tab-users'],
      ['go-auth', 'activity-tab-auth'],
      ['go-secrets', 'activity-tab-secrets'],
    ] as const) {
      list.find((c) => c.id === id)!.run();
      expect(document.getElementById(tab)!.getAttribute('aria-selected'), id).toBe('true');
    }
  });

  it('add, settings and lock call what their buttons call', () => {
    const list = paletteCommands();
    list.find((c) => c.id === 'add')!.run();
    expect(document.getElementById('modal-overlay')!.classList.contains('open')).toBe(true);
    list.find((c) => c.id === 'settings')!.run();
    expect(document.getElementById('settings-overlay')!.classList.contains('open')).toBe(true);
    list.find((c) => c.id === 'lock')!.run();
    expect(lockVault).toHaveBeenCalledOnce();
  });

  it('search and sidebar commands tolerate a page without those controls', () => {
    document.getElementById('search')!.remove();
    document.getElementById('sidebar-toggle')!.remove();
    const list = paletteCommands();
    expect(() => list.find((c) => c.id === 'search')!.run()).not.toThrow();
    expect(() => list.find((c) => c.id === 'sidebar')!.run()).not.toThrow();
  });
});
