/**
 * Choosing which sidebar sections and activity-bar panels exist and in what
 * order: the arrows, hide and show buttons, drag and drop, and Alt+arrow from the
 * keyboard. The first section ("All Secrets") is locked in place, and nothing
 * the person does here may drop a section from the settings by accident.
 */
import { describe, it, expect, beforeEach, afterAll } from 'vitest';
import { bootApp, entry, teardownApp } from './app-boot';

afterAll(() => teardownApp());

const $ = <T extends HTMLElement = HTMLElement>(id: string) => document.getElementById(id) as T;
type Boot = Awaited<ReturnType<typeof bootApp>>;
let Settings: Boot['Settings'];

const rows = (id: string) => [
  ...document.querySelectorAll<HTMLElement>(`#${id} .sidebar-order-row`),
];
const visibleKeys = (id: string) =>
  rows(id)
    .filter((r) => !r.classList.contains('sidebar-order-hidden'))
    .map((r) => r.dataset.key);
const act = (id: string, key: string, action: string) =>
  document
    .querySelector<HTMLElement>(`#${id} [data-key="${key}"] [data-action="${action}"]`)!
    .click();

beforeEach(async () => {
  ({ Settings } = await bootApp([entry('a', 'Alpha')]));
  $('settings-btn').click();
}, 30000);

describe('the sidebar section list', () => {
  it('lists every section, with the first locked', () => {
    expect(visibleKeys('s-sidebar-sections')).toEqual([
      'all',
      'price',
      'env',
      'category',
      'project',
      'tags',
      'pools',
      'prefixes',
    ]);
    expect(
      (
        document.querySelector(
          '#s-sidebar-sections [data-key="all"] [data-action="locked"]',
        ) as HTMLButtonElement
      ).disabled,
    ).toBe(true);
    expect(rows('s-sidebar-sections')[0].draggable).toBe(false);
    expect(rows('s-sidebar-sections')[1].draggable).toBe(true);
  });

  it('the arrows swap a section with its neighbour, and the ends cannot move past the list', () => {
    act('s-sidebar-sections', 'env', 'up');
    expect(visibleKeys('s-sidebar-sections').slice(0, 4)).toEqual([
      'all',
      'env',
      'price',
      'category',
    ]);
    act('s-sidebar-sections', 'env', 'down');
    expect(visibleKeys('s-sidebar-sections').slice(0, 4)).toEqual([
      'all',
      'price',
      'env',
      'category',
    ]);
    expect(
      (
        document.querySelector(
          '#s-sidebar-sections [data-key="all"] [data-action="up"]',
        ) as HTMLButtonElement
      ).disabled,
    ).toBe(true);
    expect(
      (
        document.querySelector(
          '#s-sidebar-sections [data-key="prefixes"] [data-action="down"]',
        ) as HTMLButtonElement
      ).disabled,
    ).toBe(true);
  });

  it('hiding moves a section to the "Show" list and showing puts it back at the end; the saved order follows', () => {
    act('s-sidebar-sections', 'tags', 'remove');
    expect(Settings.get('sidebarSections')).not.toContain('tags');
    const hidden = document.querySelector<HTMLElement>('#s-sidebar-sections [data-key="tags"]')!;
    expect(hidden.classList.contains('sidebar-order-hidden')).toBe(true);
    act('s-sidebar-sections', 'tags', 'add');
    expect(Settings.get('sidebarSections')![Settings.get('sidebarSections')!.length - 1]).toBe(
      'tags',
    );
  });

  it('a click that is not on a button, or on a disabled lock, changes nothing', () => {
    const before = JSON.stringify(Settings.get('sidebarSections'));
    $('s-sidebar-sections').click();
    document.querySelector<HTMLElement>('#s-sidebar-sections .sidebar-order-label')!.click();
    const lock = document.querySelector<HTMLElement>('#s-sidebar-sections [data-action="locked"]')!;
    lock.click();
    expect(JSON.stringify(Settings.get('sidebarSections'))).toBe(before);
  });

  const drag = (type: string, target: HTMLElement) => {
    const ev = new Event(type, { bubbles: true, cancelable: true }) as DragEvent;
    Object.defineProperty(ev, 'dataTransfer', {
      value: { setData() {}, effectAllowed: '' },
    });
    target.dispatchEvent(ev);
    return ev;
  };
  const row = (k: string) =>
    document.querySelector<HTMLElement>(`#s-sidebar-sections [data-key="${k}"]`)!;

  it('dragging a section onto another puts it before that one', () => {
    drag('dragstart', row('tags'));
    expect(row('tags').classList.contains('dragging')).toBe(true);
    drag('dragover', row('price'));
    expect(row('price').classList.contains('drag-over')).toBe(true);
    drag('drop', row('price'));
    expect(visibleKeys('s-sidebar-sections').slice(0, 3)).toEqual(['all', 'tags', 'price']);
  });

  it('cannot drop onto the locked row, a hidden section, itself, or with nothing being dragged', () => {
    act('s-sidebar-sections', 'pools', 'remove');
    const before = JSON.stringify(Settings.get('sidebarSections'));
    drag('drop', row('price'));
    drag('dragstart', row('tags'));
    drag('dragover', row('all'));
    expect(row('all').classList.contains('drag-over')).toBe(false);
    drag('dragover', row('pools'));
    expect(row('pools').classList.contains('drag-over')).toBe(false);
    drag('drop', row('all'));
    drag('drop', row('pools'));
    drag('drop', row('tags'));
    drag('drop', $('s-sidebar-sections'));
    expect(JSON.stringify(Settings.get('sidebarSections'))).toBe(before);
    drag('dragend', row('tags'));
    expect(row('tags').classList.contains('dragging')).toBe(false);
  });

  it('a drag that starts on the locked row does not start', () => {
    drag('dragstart', row('all'));
    expect(row('all').classList.contains('dragging')).toBe(false);
  });

  const alt = (target: HTMLElement, k: string, altKey = true) =>
    target.dispatchEvent(
      new KeyboardEvent('keydown', { key: k, altKey, bubbles: true, cancelable: true }),
    );

  it('Alt+Down and Alt+Up move the focused section, but never above the locked one or off either end', () => {
    alt(row('price'), 'ArrowDown');
    expect(visibleKeys('s-sidebar-sections').slice(0, 3)).toEqual(['all', 'env', 'price']);
    alt(row('price'), 'ArrowUp');
    expect(visibleKeys('s-sidebar-sections').slice(0, 3)).toEqual(['all', 'price', 'env']);
    const before = JSON.stringify(Settings.get('sidebarSections'));
    alt(row('price'), 'ArrowUp');
    alt(row('prefixes'), 'ArrowDown');
    alt(row('all'), 'ArrowDown');
    alt(row('price'), 'ArrowDown', false);
    alt(row('price'), 'x');
    alt($('s-sidebar-sections'), 'ArrowDown');
    expect(JSON.stringify(Settings.get('sidebarSections'))).toBe(before);
  });

  it('Alt+arrows on a hidden section do nothing', () => {
    act('s-sidebar-sections', 'tags', 'remove');
    const before = JSON.stringify(Settings.get('sidebarSections'));
    alt(row('tags'), 'ArrowDown');
    expect(JSON.stringify(Settings.get('sidebarSections'))).toBe(before);
  });
});

describe('the panel list', () => {
  it('reorders panels, hides the removable ones and shows them again; Secrets is locked', () => {
    expect(visibleKeys('s-panel-order')[0]).toBe('secrets');
    expect(
      (
        document.querySelector(
          '#s-panel-order [data-key="secrets"] [data-action="locked"]',
        ) as HTMLButtonElement
      ).disabled,
    ).toBe(true);
    act('s-panel-order', 'tools', 'down');
    expect(Settings.get('panelOrder')![1]).not.toBe('tools');
    act('s-panel-order', 'users', 'up');
    act('s-panel-order', 'users', 'remove');
    expect(Settings.get('panelOrder')).not.toContain('users');
    act('s-panel-order', 'users', 'add');
    expect(Settings.get('panelOrder')).toContain('users');
  });

  it('a click away from the buttons changes nothing', () => {
    const before = JSON.stringify(Settings.get('panelOrder'));
    $('s-panel-order').click();
    document.querySelector<HTMLElement>('#s-panel-order .sidebar-order-label')!.click();
    expect(JSON.stringify(Settings.get('panelOrder'))).toBe(before);
  });

  it('the up button on the first row and the down button on the last are disabled', () => {
    const r = rows('s-panel-order').filter((x) => !x.classList.contains('sidebar-order-hidden'));
    expect((r[0].querySelector('[data-action="up"]') as HTMLButtonElement).disabled).toBe(true);
    expect(
      (r[r.length - 1].querySelector('[data-action="down"]') as HTMLButtonElement).disabled,
    ).toBe(true);
  });
});

describe('the theme swatches', () => {
  it('choosing one applies it, marks it, and rebuilds the row', () => {
    const swatches = [...document.querySelectorAll<HTMLElement>('#theme-swatches .theme-swatch')];
    expect(swatches.length).toBeGreaterThan(2);
    swatches[2].click();
    expect(Settings.get('theme')).toBe(swatches[2].title.toLowerCase().replace(/\s+/g, '-'));
    expect(document.querySelectorAll('#theme-swatches .theme-swatch.active')).toHaveLength(1);
  });
});
