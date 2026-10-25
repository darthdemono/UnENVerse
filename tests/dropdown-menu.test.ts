// SPDX-License-Identifier: PolyForm-Strict-1.0.0
// Copyright Jubair Hasan.
/**
 * The small drop-down menus (import/export choices, vault switcher).
 *
 * They close when you click elsewhere, but the outside-click handler is armed a
 * beat after opening so the opening click does not close them. That arming must
 * die with the menu: otherwise a menu dismissed quickly leaves a handler behind
 * that shuts the next menu on the very click that opens it.
 */
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { showDropdown, showContextMenu } from '../src/ts/modals';
import { loadRealIndexHtml } from './helpers';

const dd = () => document.getElementById('dropdown')!;
const open = () => dd().style.display === 'block';

beforeEach(() => {
  loadRealIndexHtml();
  vi.useFakeTimers();
});
afterEach(() => {
  vi.useRealTimers();
});

const anchor = () => {
  const b = document.createElement('button');
  document.body.appendChild(b);
  return b;
};

describe.each([
  ['dropdown', (a: HTMLElement, items: any[]) => showDropdown(a, items)],
  ['context menu', (_a: HTMLElement, items: any[]) => showContextMenu(10, 10, items)],
])('%s', (_name, show) => {
  it('runs the chosen item, closes, and does not close on the click that opened it', async () => {
    const fn = vi.fn();
    const a = anchor();
    show(a, [{ label: 'One', fn }, '---', { label: 'Two', fn: vi.fn() }]);
    expect(open()).toBe(true);
    expect([...dd().querySelectorAll('.dropdown-item')].map((i) => i.textContent)).toEqual([
      'One',
      'Two',
    ]);
    vi.advanceTimersByTime(60);
    dd().querySelector<HTMLElement>('.dropdown-item')!.click();
    expect(fn).toHaveBeenCalledTimes(1);
    expect(open()).toBe(false);
  });

  it('closes on a click anywhere else, but not on a click inside it', () => {
    show(anchor(), [{ label: 'One', fn: vi.fn() }]);
    vi.advanceTimersByTime(60);
    dd().click();
    expect(open()).toBe(true);
    document.body.click();
    expect(open()).toBe(false);
  });

  it('dismissed quickly, it leaves nothing behind to close the next one on its opening click', () => {
    const first = anchor();
    show(first, [{ label: 'One', fn: vi.fn() }]);
    // Chosen well inside the arming delay.
    dd().querySelector<HTMLElement>('.dropdown-item')!.click();
    vi.advanceTimersByTime(60);
    const second = anchor();
    show(second, [{ label: 'Two', fn: vi.fn() }]);
    second.click();
    expect(open()).toBe(true);
  });

  it('opening one replaces the one already open', () => {
    show(anchor(), [{ label: 'One', fn: vi.fn() }]);
    show(anchor(), [{ label: 'Two', fn: vi.fn() }]);
    expect([...dd().querySelectorAll('.dropdown-item')].map((i) => i.textContent)).toEqual(['Two']);
  });
});

describe('the anchored dropdown', () => {
  it('ignores a click on its own anchor so the button can toggle it', () => {
    const a = anchor();
    showDropdown(a, [{ label: 'One', fn: vi.fn() }]);
    vi.advanceTimersByTime(60);
    a.click();
    expect(open()).toBe(true);
  });
});
