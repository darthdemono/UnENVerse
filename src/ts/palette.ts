// SPDX-License-Identifier: PolyForm-Strict-1.0.0
// Copyright Jubair Hasan.
/**
 * @file
 * The command palette (Ctrl+Shift+P): type a few letters, press Enter, and the app navigates or acts.
 *
 * It is a keyboard path to what the activity bar and header already do, not a second set of
 * features: every command calls the function the matching button calls. The overlay is built on
 * first use, so it adds nothing to the page until someone asks for it. Focus goes to the input on
 * open and back to where it was on close; Up and Down move through the list, Enter runs the
 * highlighted command, Escape closes.
 */
import { clearAllFilters, switchPanel, triggerRender } from './state';
import { html, setHtml } from './html';
import { lockVault } from './lock';
import { openAdd } from './modals';
import { openSettings } from './settings-panel';
import { showToast } from './utils';

export interface Command {
  id: string;
  label: string;
  /** Shown dimmed beside the label: the shortcut, or where the command goes. */
  hint?: string;
  run: () => void;
}

/**
 * How well `query` matches `label`: -1 when its letters do not appear in order, otherwise higher is
 * better. A match at the start of the label or of a word beats one in the middle, and a run of
 * consecutive letters beats scattered ones.
 */
export function scoreCommand(query: string, label: string): number {
  const q = query.trim().toLowerCase();
  if (!q) return 0;
  const text = label.toLowerCase();
  let at = -1;
  let score = 0;
  let previous = -2;
  for (const ch of q) {
    at = text.indexOf(ch, at + 1);
    if (at < 0) return -1;
    score += 1;
    if (at === previous + 1) score += 3;
    if (at === 0 || text[at - 1] === ' ') score += 4;
    previous = at;
  }
  return score - text.length / 100;
}

export function filterCommands(query: string, commands: Command[]): Command[] {
  return commands
    .map((c) => ({ c, s: scoreCommand(query, c.label) }))
    .filter((x) => x.s >= 0)
    .sort((a, b) => b.s - a.s)
    .map((x) => x.c);
}

export function paletteCommands(): Command[] {
  const go = (id: string, label: string, panel: Parameters<typeof switchPanel>[0]): Command => ({
    id,
    label,
    hint: 'Go to',
    run: () => switchPanel(panel),
  });
  return [
    go('go-secrets', 'Go to Secrets', 'secrets'),
    go('go-tools', 'Go to Tools', 'tools'),
    go('go-remote', 'Go to Remote Vaults', 'remote'),
    go('go-users', 'Go to Users', 'users'),
    go('go-auth', 'Go to Authenticator', 'auth'),
    { id: 'add', label: 'Add a secret', hint: 'Ctrl+N', run: () => openAdd() },
    { id: 'settings', label: 'Open settings', hint: 'S', run: () => openSettings() },
    { id: 'lock', label: 'Lock the vault', run: () => void lockVault() },
    {
      id: 'search',
      label: 'Search secrets',
      hint: 'Ctrl+K',
      run: () => {
        switchPanel('secrets');
        const box = document.getElementById('search') as HTMLInputElement | null;
        box?.focus();
        box?.select();
      },
    },
    {
      id: 'clear-filters',
      label: 'Clear all filters',
      hint: 'Shift+Esc',
      run: () => {
        clearAllFilters();
        triggerRender();
        showToast('Filters cleared', 'ok', 1500);
      },
    },
    {
      id: 'sidebar',
      label: 'Toggle the sidebar',
      hint: 'B',
      run: () => document.getElementById('sidebar-toggle')?.click(),
    },
    {
      id: 'shortcuts',
      label: 'Show keyboard shortcuts',
      hint: '?',
      run: () => document.getElementById('shortcuts-overlay')?.classList.add('open'),
    },
  ];
}

let overlay: HTMLElement | null = null;
let shown: Command[] = [];
let active = 0;
let all: Command[] = [];
let returnTo: HTMLElement | null = null;

export function isPaletteOpen(): boolean {
  return !!overlay?.classList.contains('open');
}

function paint(): void {
  const list = overlay!.querySelector('#palette-list')!;
  const input = overlay!.querySelector<HTMLInputElement>('#palette-input')!;
  setHtml(
    list,
    shown.length
      ? html`${shown.map(
          (c, i) =>
            html`<li id="palette-opt-${i}" role="option" aria-selected="${i === active}" class="palette-opt${
              i === active ? ' active' : ''
            }" data-palette-index="${i}"><span>${c.label}</span>${
              c.hint ? html`<span class="palette-hint">${c.hint}</span>` : ''
            }</li>`,
        )}`
      : html`<li class="palette-empty" role="presentation">No matching command</li>`,
  );
  if (shown.length) input.setAttribute('aria-activedescendant', `palette-opt-${active}`);
  else input.removeAttribute('aria-activedescendant');
  list.querySelector('.active')?.scrollIntoView?.({ block: 'nearest' });
}

function mount(): HTMLElement {
  const el = document.createElement('div');
  el.id = 'palette-overlay';
  el.className = 'palette-overlay';
  setHtml(
    el,
    html`<div class="palette-box" role="dialog" aria-modal="true" aria-label="Command palette"><input id="palette-input" class="palette-input" type="text" role="combobox" aria-expanded="true" aria-controls="palette-list" aria-label="Type a command" autocomplete="off" spellcheck="false" placeholder="Type a command" /><ul id="palette-list" class="palette-list" role="listbox" aria-label="Commands"></ul></div>`,
  );
  const input = el.querySelector<HTMLInputElement>('#palette-input')!;
  input.addEventListener('input', () => {
    shown = filterCommands(input.value, all);
    active = 0;
    paint();
  });
  input.addEventListener('keydown', (e) => {
    if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      e.preventDefault();
      if (!shown.length) return;
      active = (active + (e.key === 'ArrowDown' ? 1 : shown.length - 1)) % shown.length;
      paint();
    } else if (e.key === 'Enter') {
      e.preventDefault();
      runAt(active);
    } else if (e.key === 'Escape') {
      e.preventDefault();
      e.stopPropagation();
      closePalette();
    }
  });
  el.addEventListener('click', (e) => {
    const target = e.target as HTMLElement;
    const opt = target.closest<HTMLElement>('[data-palette-index]');
    if (opt) runAt(Number(opt.dataset.paletteIndex));
    else if (target === el) closePalette();
  });
  document.body.appendChild(el);
  return el;
}

function runAt(i: number): void {
  const command = shown[i];
  if (!command) return;
  closePalette();
  command.run();
}

export function openPalette(commands: Command[] = paletteCommands()): void {
  overlay ??= mount();
  if (!overlay.isConnected) document.body.appendChild(overlay);
  all = commands;
  shown = commands;
  active = 0;
  returnTo = document.activeElement as HTMLElement | null;
  const input = overlay.querySelector<HTMLInputElement>('#palette-input')!;
  input.value = '';
  overlay.classList.add('open');
  paint();
  input.focus();
}

export function closePalette(): void {
  if (!isPaletteOpen()) return;
  overlay!.classList.remove('open');
  returnTo?.focus?.();
  returnTo = null;
}
