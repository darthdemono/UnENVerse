// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
// The recovery codes list in the entry form: rows, used ticks and times, pasting, and the way the
// form carries `codes` and `phrase` on any entry (a password card holds its own).
import { beforeEach, describe, expect, it } from 'vitest';
import {
  addPasted,
  fillCodes,
  initCodesEditor,
  readCodes,
  toLocalInput,
} from '../src/ts/codes-editor';
import { fillForm, formToEntry } from '../src/ts/modals';
import { st } from '../src/ts/state';
import { loadRealIndexHtml, resetState } from './helpers';

const $ = <T extends HTMLElement = HTMLElement>(id: string) => document.getElementById(id) as T;
const rows = () => [...document.querySelectorAll<HTMLElement>('#f-codes-list .code-row')];
const tick = () => new Promise((r) => setTimeout(r, 0));

beforeEach(() => {
  loadRealIndexHtml();
  resetState(st);
  delete (window as unknown as { __TAURI__?: unknown }).__TAURI__;
});

describe('rows', () => {
  it('reads back exactly what it was given, an untouched used time included', () => {
    const text = 'aaaa-1111\nbbbb-2222\tUSED 2026-11-01\ncccc-3333\tUSED 2026-11-02T10:30:00.000Z';
    fillCodes(text);
    expect(rows()).toHaveLength(3);
    expect(rows()[0].querySelector<HTMLInputElement>('.code-used')!.checked).toBe(false);
    expect(rows()[0].querySelector<HTMLInputElement>('.code-when')!.hidden).toBe(true);
    expect(rows()[1].querySelector<HTMLInputElement>('.code-used')!.checked).toBe(true);
    expect(readCodes()).toBe(text);
  });

  it('writes a changed time as a full ISO timestamp', () => {
    fillCodes('aaaa-1111\tUSED 2026-11-01');
    rows()[0].querySelector<HTMLInputElement>('.code-when')!.value = '2026-11-05T08:15';
    expect(readCodes()).toBe(`aaaa-1111\tUSED ${new Date('2026-11-05T08:15').toISOString()}`);
  });

  it('stamps now when a code is ticked used, and hides the time again when unticked', () => {
    fillCodes('aaaa-1111');
    const tickBox = rows()[0].querySelector<HTMLInputElement>('.code-used')!;
    tickBox.checked = true;
    tickBox.onchange!(new Event('change'));
    const when = rows()[0].querySelector<HTMLInputElement>('.code-when')!;
    expect(when.hidden).toBe(false);
    expect(when.value).not.toBe('');
    expect(readCodes()).toMatch(/^aaaa-1111\tUSED \d{4}-\d\d-\d\dT[\d:.]+Z$/);
    // A second tick on a row that already has a time keeps that time.
    when.value = '2026-01-02T03:04';
    tickBox.onchange!(new Event('change'));
    expect(when.value).toBe('2026-01-02T03:04');
    tickBox.checked = false;
    tickBox.onchange!(new Event('change'));
    expect(when.hidden).toBe(true);
    expect(readCodes()).toBe('aaaa-1111');
  });

  it('stamps now when a used row has lost its time', () => {
    fillCodes('aaaa-1111\tUSED 2026-11-01');
    rows()[0].querySelector<HTMLInputElement>('.code-when')!.value = '';
    expect(readCodes()).toMatch(/^aaaa-1111\tUSED \d{4}-/);
  });

  it('drops empty rows, squeezes whitespace and renumbers after a removal', () => {
    initCodesEditor(() => {});
    fillCodes('aaaa 1111\n\nbbbb-2222');
    $('f-codes-add').click();
    rows()[2].querySelector<HTMLInputElement>('.code-text')!.value = '   ';
    rows()[0].querySelector<HTMLInputElement>('.code-text')!.value = ' aaaa \t 1111 ';
    expect(readCodes()).toBe('aaaa 1111\nbbbb-2222');
    rows()[0].querySelector<HTMLElement>('.code-remove')!.click();
    expect(rows().map((r) => r.querySelector('.code-n')!.textContent)).toEqual(['1', '2']);
    expect(rows()[0].querySelector('.code-text')!.getAttribute('aria-label')).toBe('Code 1');
  });

  it('turns an unreadable time into nothing', () => {
    expect(toLocalInput('not a time')).toBe('');
    expect(toLocalInput('2026-11-05T08:15:00')).toBe('2026-11-05T08:15');
  });

  it('does nothing when the form is not on the page', async () => {
    document.body.replaceChildren();
    fillCodes('aaaa-1111');
    expect(readCodes()).toBe('');
    expect(await addPasted('x-1')).toBe(1); // no list to compare with, nothing to skip
    initCodesEditor(() => {});
  });
});

describe('pasting', () => {
  it('splits on spaces, commas and lines in the browser, and skips what is there', async () => {
    fillCodes('aaaa-1111');
    expect(await addPasted('aaaa-1111, bbbb-2222\ncccc-3333;bbbb-2222')).toBe(2);
    expect(readCodes()).toBe('aaaa-1111\nbbbb-2222\ncccc-3333');
  });

  it('asks Rust to parse the text in the desktop app', async () => {
    let asked = '';
    (window as unknown as { __TAURI__: unknown }).__TAURI__ = {
      core: {
        invoke: async (_c: string, a: { text: string }) => {
          asked = a.text;
          return ['x-1', 'x-2'];
        },
      },
    };
    expect(await addPasted('1. x-1\n2. x-2')).toBe(2);
    expect(asked).toBe('1. x-1\n2. x-2');
  });

  it('wires the buttons: add row, add pasted, a failed parse, show phrase', async () => {
    const errors: unknown[] = [];
    initCodesEditor((e) => errors.push(e));
    $('f-codes-add').click();
    expect(rows()).toHaveLength(1);
    $<HTMLTextAreaElement>('f-codes-paste').value = 'dddd-4444';
    $('f-codes-paste-add').click();
    await tick();
    expect($<HTMLTextAreaElement>('f-codes-paste').value).toBe('');
    expect(rows()).toHaveLength(2);
    (window as unknown as { __TAURI__: unknown }).__TAURI__ = {
      core: {
        invoke: async () => {
          throw new Error('parse failed');
        },
      },
    };
    $<HTMLTextAreaElement>('f-codes-paste').value = 'eeee-5555';
    $('f-codes-paste-add').click();
    await tick();
    expect(errors).toHaveLength(1);
    const show = $<HTMLInputElement>('f-phrase-show');
    show.checked = true;
    show.onchange!(new Event('change'));
    expect($<HTMLInputElement>('f-phrase').type).toBe('text');
    show.checked = false;
    show.onchange!(new Event('change'));
    expect($<HTMLInputElement>('f-phrase').type).toBe('password');
  });
});

describe('the entry form', () => {
  it('carries codes and a phrase on a password card, beside its 2FA seed and other values', () => {
    fillForm({
      provider: 'Bank',
      secretType: 'password',
      api_key: 'pw-value',
      totp_secret: 'JBSWY3DPEHPK3PXP',
      extra_vars: [
        { key: 'pin', value: '1234', secret: true },
        { key: 'codes', value: 'aaaa-1111\nbbbb-2222\tUSED 2026-11-01', secret: true },
        { key: 'phrase', value: 'blue cat green dog', secret: true },
      ],
    });
    expect(rows()).toHaveLength(2);
    expect($<HTMLInputElement>('f-phrase').value).toBe('blue cat green dog');
    expect($<HTMLDetailsElement>('f-more').open).toBe(true);
    // The generic list holds only the other value.
    expect(document.querySelectorAll('#f-extra-vars-list .extra-var-row')).toHaveLength(1);
    const back = formToEntry();
    const vars = Object.fromEntries((back.extra_vars ?? []).map((v) => [v.key, v]));
    expect(vars.pin.value).toBe('1234');
    expect(vars.codes.value).toBe('aaaa-1111\nbbbb-2222\tUSED 2026-11-01');
    expect(vars.codes.secret).toBe(true);
    expect(vars.phrase.value).toBe('blue cat green dog');
    expect(back.totp_secret).toBeTruthy();
  });

  it('saves no codes or phrase variable when both are empty', () => {
    fillForm({ provider: 'Plain', secretType: 'password', api_key: 'pw-value' });
    expect(rows()).toHaveLength(0);
    expect($<HTMLDetailsElement>('f-more').open).toBe(false);
    expect(formToEntry().extra_vars).toBeUndefined();
  });
});
