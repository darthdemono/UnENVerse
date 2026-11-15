// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
// The glossary inside the app (R44): the lookup, the form-label tooltips and the Tools pane. All three
// read glossary/terms.json, the file the website and `unv glossary` use.
import { beforeEach, describe, expect, it } from 'vitest';
import {
  applyGlossaryTooltips,
  categories,
  definitionFor,
  findTerm,
  searchTerms,
} from '../src/ts/glossary';
import { initGlossaryPane } from '../src/ts/glossary-pane';
import { loadRealIndexHtml } from './helpers';
import { mountToolsPanes } from '../src/ts/tools-markup';

describe('glossary lookup', () => {
  it('finds a term by its id, its title or a stored field name, however it is written', () => {
    expect(findTerm('api_key')?.id).toBe('api-key');
    expect(findTerm('API-KEY')?.id).toBe('api-key');
    expect(findTerm('Key Pool')?.id).toBe('pool');
    expect(findTerm('')).toBeUndefined();
    expect(findTerm('no such term at all')).toBeUndefined();
    expect(definitionFor('app_org')).toContain('organisation');
  });

  it('searches titles first, then names, then definitions, and needs every word', () => {
    const r = searchTerms('rate limit');
    expect(r[0].id).toBe('rate-limit');
    expect(searchTerms('rate limit zzzzqqq')).toEqual([]);
    const only = searchTerms('', 'secret type');
    expect(only.length).toBeGreaterThanOrEqual(26);
    expect(only.every((t) => t.category === 'secret type')).toBe(true);
    expect(categories()).toContain('command');
  });
});

describe('form label tooltips', () => {
  it('puts the definition on a label that names a known field and leaves others alone', () => {
    document.body.innerHTML =
      '<label id="a" class="form-label" data-field="api_secret" title="old"></label>' +
      '<label id="b" class="form-label" data-field="nothing_like_this" title="keep"></label>' +
      '<label id="c" class="not-a-form-label" data-field="api_secret"></label>';
    applyGlossaryTooltips();
    expect(document.getElementById('a')!.title).toBe(definitionFor('api_secret'));
    expect(document.getElementById('b')!.title).toBe('keep');
    expect(document.getElementById('c')!.title).toBe('');
  });
});

describe('the Tools glossary pane', () => {
  beforeEach(() => {
    loadRealIndexHtml();
    mountToolsPanes();
  });

  it('has a nav button, a labelled search box and a category picker', () => {
    expect(document.querySelector('.tool-nav-btn[data-tool="glossary"]')).not.toBeNull();
    expect(document.getElementById('tool-glossary')).not.toBeNull();
    expect(document.getElementById('glossary-q')!.getAttribute('aria-label')).toBeTruthy();
    expect(document.getElementById('glossary-cat')!.getAttribute('aria-label')).toBeTruthy();
  });

  it('lists terms, filters as you type and says when nothing matches', () => {
    initGlossaryPane();
    const list = document.getElementById('glossary-list')!;
    const status = document.getElementById('glossary-status')!;
    expect(list.querySelectorAll('.gloss-item').length).toBeGreaterThan(50);
    expect(status.textContent).toContain('Showing the first');
    const q = document.getElementById('glossary-q') as HTMLInputElement;
    q.value = 'key pool';
    q.dispatchEvent(new Event('input'));
    expect(list.querySelector('.gloss-item strong')!.textContent).toBe('Key Pool');
    q.value = 'zzzzqqq';
    q.dispatchEvent(new Event('input'));
    expect(list.querySelectorAll('.gloss-item')).toHaveLength(0);
    expect(status.textContent).toContain('No term matches');
    const cat = document.getElementById('glossary-cat') as HTMLSelectElement;
    expect([...cat.options].map((o) => o.value)).toContain('secret type');
  });

  it('escapes what it shows', () => {
    initGlossaryPane();
    const q = document.getElementById('glossary-q') as HTMLInputElement;
    q.value = '<img src=x onerror=alert(1)>';
    q.dispatchEvent(new Event('input'));
    expect(document.querySelector('#glossary-list img')).toBeNull();
  });
});
