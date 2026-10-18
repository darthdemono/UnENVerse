/**
 * Panes are painted into containers the page may not have: a pane that was
 * never mounted, a layout without the sidebar, a window being torn down. Each
 * entry point must then quietly do nothing instead of throwing into whatever
 * called it.
 */
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { st } from '../src/ts/state';
import { resetState } from './helpers';

beforeEach(() => {
  document.body.innerHTML = '';
  resetState(st);
});

describe('entry points with no markup on the page', () => {
  it.each([
    ['history-pane', 'initHistoryPane'],
    ['vendor-import-pane', 'initVendorImportPane'],
    ['enrich-pane', 'initEnrichPane'],
    ['enrich-pane', 'initDoctorPane'],
    ['timeline', 'renderTimeline'],
    ['timeline', 'initTimelinePane'],
    ['users', 'renderUsersPanel'],
    ['users', 'renderClassesPanel'],
    ['remote-panel', 'renderRemotePanel'],
    ['remote-panel', 'initRemotePanel'],
    ['auth-panel', 'renderAuthPanel'],
    ['lan', 'initLanPanel'],
    ['pools', 'initPoolsPane'],
    ['nodes-pane', 'initNodesPane'],
    ['uid-pane', 'initUidPane'],
  ])('%s.%s does nothing and does not throw', async (mod, fn) => {
    const m = (await import(`../src/ts/${mod}`)) as Record<string, () => unknown>;
    expect(typeof m[fn]).toBe('function');
    await expect(Promise.resolve().then(() => m[fn]())).resolves.not.toThrow();
  });

  it('showWifiQr with no overlay does nothing', async () => {
    const { showWifiQr } = await import('../src/ts/wifi-qr');
    expect(() => showWifiQr('WIFI:S:x;;', 'x')).not.toThrow();
  });

  it('wiring a permission editor with no boxes does nothing', async () => {
    const { wirePermEditor } = await import('../src/ts/perm-editor');
    const host = document.createElement('div');
    document.body.appendChild(host);
    expect(() => (wirePermEditor as (...a: unknown[]) => void)(host, 'k', () => {})).not.toThrow();
  });

  it('the caps-lock hint with no input or no hint does nothing', async () => {
    const { wireCapsLockHint } = await import('../src/ts/ui-qol');
    expect(() => wireCapsLockHint('nope', 'nada')).not.toThrow();
    document.body.innerHTML = '<input id="a">';
    expect(() => wireCapsLockHint('a', 'missing')).not.toThrow();
  });
});

describe('panes that find only part of their markup', () => {
  it('the timeline wires whichever controls exist and paints rows without a summary line', async () => {
    vi.resetModules();
    document.body.innerHTML =
      '<select id="tl-sort"><option value="created-desc"></option></select><div id="tl-rows"></div>';
    const { st: freshSt } = await import('../src/ts/state');
    freshSt.vault = {
      api_keys: [{ id: 'a', provider: 'A', api_key: 'k', created_at: '2026-01-01T00:00:00Z' }],
      user_categories: [],
      projects: [],
    } as never;
    const m = await import('../src/ts/timeline');
    expect(() => m.initTimelinePane()).not.toThrow();
    m.initTimelinePane();
    document.getElementById('tl-sort')!.dispatchEvent(new Event('change'));
    expect(document.getElementById('tl-rows')!.textContent).toContain('A');
  });

  it('the timeline copes with a vault that has no entry list', async () => {
    vi.resetModules();
    document.body.innerHTML = '<div id="tl-rows"></div><div id="tl-summary"></div>';
    const { st: freshSt } = await import('../src/ts/state');
    freshSt.vault = {} as never;
    const m = await import('../src/ts/timeline');
    expect(() => m.renderTimeline()).not.toThrow();
    expect(document.getElementById('tl-summary')!.textContent).toContain('0');
  });
});
