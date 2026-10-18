// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
/** Branch coverage for perm-editor.ts: every builder field, missing elements, a failing save. */
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { st } from '../src/ts/state';
import { permEditorHtml, wirePermEditor } from '../src/ts/perm-editor';
import { loadRealIndexHtml, makeEntry, makeVault, resetState } from './helpers';

const toast = vi.fn();
vi.mock('../src/ts/utils', async (importOriginal) => {
  const real = await importOriginal<typeof import('../src/ts/utils')>();
  return { ...real, showToast: (...a: unknown[]) => toast(...a) };
});

const $ = <T extends HTMLElement = HTMLElement>(id: string) => document.getElementById(id) as T;
const flush = () => new Promise((r) => setTimeout(r, 10));

function mount(onSave: (e: { read: string; write: string }) => Promise<void> = async () => {}) {
  document.body.innerHTML = '<div id="host"></div>';
  $('host').innerHTML = String(permEditorHtml('t', { read: '', write: '' }));
  wirePermEditor('t', onSave);
}

function values(field: string): string[] {
  ($('t-b-field') as HTMLSelectElement).value = field;
  $('t-b-field').dispatchEvent(new Event('change'));
  return [...($('t-b-value') as HTMLSelectElement).options].map((o) => o.value);
}

beforeEach(() => {
  loadRealIndexHtml();
  resetState(st);
  toast.mockClear();
});

describe('builder suggestions', () => {
  it('offers the fixed env and type lists and tolerates a vault with no categories or tags', () => {
    st.vault = makeVault({ api_keys: [makeEntry({ tags: undefined } as any)] });
    (st.vault as any).user_categories = undefined;
    mount();
    expect(values('env')).toContain('staging');
    expect(values('type')).toContain('api_key');
    expect(values('category')).toEqual(['*']);
    expect(values('tag')).toEqual(['*']);
    expect(values('project')[0]).toBe('*');
  });
});

describe('wiring', () => {
  it('reports a save that throws', async () => {
    mount(async () => {
      throw new Error('disk full');
    });
    $('t-expr-save').click();
    await flush();
    expect(toast).toHaveBeenCalledWith(
      expect.stringContaining('Save failed: disk full'),
      'err',
      4000,
    );
  });

  it('refuses to save an expression that does not parse', async () => {
    mount();
    ($('t-expr-read') as HTMLTextAreaElement).value = 'nonsense:::(';
    $('t-expr-save').click();
    await flush();
    expect(toast).toHaveBeenCalledWith('Fix the expression before saving', 'err');
  });

  it('saves and confirms', async () => {
    const onSave = vi.fn(async () => {});
    mount(onSave);
    $('t-expr-save').click();
    await flush();
    expect(onSave).toHaveBeenCalled();
    expect(toast).toHaveBeenCalledWith('Permissions saved ✓', 'ok');
  });
});
