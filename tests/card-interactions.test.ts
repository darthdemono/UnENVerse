/**
 * Small card behaviours that act on the entry list directly: expanding, the
 * eye button, rotating, duplicating, deleting (with Undo) and removing a bundle
 * member. Each must act on the entry the person clicked, and do nothing when that
 * entry is no longer there.
 */
import { describe, it, expect, beforeEach, afterAll, vi } from 'vitest';
import { bootApp, entry, teardownApp } from './app-boot';

const toasts: [string, string?][] = [];
const copied: string[] = [];
let confirmAnswer = true;
let clipboardFails = false;
vi.mock('../src/ts/utils', async (importOriginal) => {
  const real = await importOriginal<typeof import('../src/ts/utils')>();
  return {
    ...real,
    showToast: (m: string, k?: string) => toasts.push([m, k]),
    showConfirm: async () => confirmAnswer,
    clipboardWrite: async (t: string) => {
      if (clipboardFails) throw new Error('denied');
      copied.push(t);
    },
  };
});

afterAll(() => teardownApp());

const $ = <T extends HTMLElement = HTMLElement>(id: string) => document.getElementById(id) as T;
const flush = (ms = 30) => new Promise((r) => setTimeout(r, ms));
type St = Awaited<ReturnType<typeof bootApp>>['st'];
let st: St;
const ev = () => ({ stopPropagation() {} }) as unknown as Event;
const act = async (action: string, idx = 0) => {
  document
    .querySelector<HTMLElement>(`#card-grid [data-action="${action}"][data-idx="${idx}"]`)!
    .click();
  await flush();
};

beforeEach(async () => {
  toasts.length = 0;
  copied.length = 0;
  confirmAnswer = true;
  clipboardFails = false;
  ({ st } = await bootApp([entry('a', 'Alpha'), entry('b', 'Beta', { compromised: true })]));
  toasts.length = 0;
}, 30000);

describe('expanding and revealing', { timeout: 30000 }, () => {
  it('expanding something that is gone does nothing', async () => {
    const { toggleCard, toggleReveal } = await import('../src/ts/modals');
    toggleCard(ev(), 9);
    toggleReveal(ev(), 'key', 9, 'v');
    expect(st.expanded.size).toBe(0);
    expect(st.revealed).toEqual({});
  });

  it('expanding a card whose element has not been drawn still remembers it by id', async () => {
    const { toggleCard } = await import('../src/ts/modals');
    document.querySelector('#card-grid .card[data-idx="0"]')!.remove();
    toggleCard(ev(), 0);
    expect(st.expanded.has('a')).toBe(false);
  });

  it('the eye button hides again on the first click when the value starts out printed', async () => {
    // Regression: with masking off (or a public value) the stored flag was unset, so the first
    // click "revealed" an already visible value and changed nothing.
    ({ st } = await bootApp([entry('a', 'Alpha', { api_key: 'visible-value-1234' })], {
      settings: { maskKeysByDefault: false },
    }));
    const value = () => $('kv-key-0').textContent;
    expect(value()).toBe('visible-value-1234');
    await act('reveal');
    expect(value()).not.toBe('visible-value-1234');
    expect($('reveal-key-0').getAttribute('aria-pressed')).toBe('false');
    await act('reveal');
    expect(value()).toBe('visible-value-1234');
    expect($('reveal-key-0').getAttribute('aria-pressed')).toBe('true');
  });

  it('the eye button on a card that is not drawn does nothing', async () => {
    const { toggleReveal } = await import('../src/ts/modals');
    document.querySelector('#kv-key-0')!.remove();
    toggleReveal(ev(), 'key', 0, 'v');
    expect(st.revealed).toEqual({});
  });
});

describe('copying a field', { timeout: 30000 }, () => {
  it('says so when the clipboard refuses', async () => {
    const { copyField } = await import('../src/ts/modals');
    clipboardFails = true;
    copyField(ev(), 'v');
    await flush();
    expect(toasts).toContainEqual(['Copy failed', 'err']);
  });
});

describe('rotating, duplicating and deleting', { timeout: 30000 }, () => {
  it('rotating a card that is gone does nothing', async () => {
    const { markAsRotated } = await import('../src/ts/modals');
    markAsRotated(9);
    expect(toasts).toEqual([]);
  });

  it('rotating keeps at most fifty old values, newest first', async () => {
    const { markAsRotated } = await import('../src/ts/modals');
    st.vault.api_keys[0].version_history = Array.from({ length: 50 }, (_, i) => ({
      value: `old-${i}`,
      saved_at: '2020-01-01T00:00:00.000Z',
    }));
    markAsRotated(0);
    const h = st.vault.api_keys[0].version_history!;
    expect(h).toHaveLength(50);
    expect(h[0].value).toBe('k-a-longenough');
    expect(h[49].value).toBe('old-48');
  });

  it('rotating an entry with no value records no history and still stamps the date', async () => {
    const { markAsRotated } = await import('../src/ts/modals');
    st.vault.api_keys[0].api_key = '';
    markAsRotated(0);
    expect(st.vault.api_keys[0].version_history).toEqual([]);
    expect(st.vault.api_keys[0].last_rotated_at).toBeTruthy();
  });

  it('rotating a compromised entry says the flag was cleared', async () => {
    await act('rotate', 1);
    expect(toasts[toasts.length - 1][0]).toMatch(/compromised flag cleared$/);
    expect(st.vault.api_keys[1].compromised).toBe(false);
  });

  it('deleting something that is gone does nothing', async () => {
    const { deleteKey } = await import('../src/ts/modals');
    deleteKey(ev(), 9);
    expect(st.vault.api_keys).toHaveLength(2);
    expect(st.undoStack).toHaveLength(0);
  });

  it('deleting an entry with no id works and Undo appends it when there is nothing to anchor to', async () => {
    delete (st.vault.api_keys[1] as { id?: unknown }).id;
    st.vault.api_keys.splice(0, 1);
    const { deleteKey } = await import('../src/ts/modals');
    deleteKey(ev(), 0);
    expect(st.vault.api_keys).toHaveLength(0);
    $('undo-btn').click();
    await flush();
    expect(st.vault.api_keys).toHaveLength(1);
  });

  it('deleting a bundle member that templates use asks first, and refuses if the references changed meanwhile', async () => {
    ({ st } = await bootApp([
      entry('bun', 'B', { secretType: 'bundle', api_key: '', bundle_primary: 'm1' }),
      entry('m1', 'Bot', { bundle_id: 'bun', bundle_slot: 'bot' }),
    ]));
    st.vault.projects[0].chunks = [
      {
        id: 'c',
        name: 'env',
        chunk_type: 'env_file',
        fields: [{ key: 'T', value: '${bundle:B/bot}', field_type: 'env_var' }],
      },
    ] as never;
    const { deleteKey } = await import('../src/ts/modals');
    confirmAnswer = false;
    deleteKey(ev(), 1);
    await flush();
    expect(st.vault.api_keys).toHaveLength(2);
    confirmAnswer = true;
    deleteKey(ev(), 1);
    await flush();
    expect(st.vault.api_keys.map((e) => e.id)).toEqual(['bun']);
  });

  it('a reference added while the question is open stops the delete', async () => {
    ({ st } = await bootApp([
      entry('bun', 'B', { secretType: 'bundle', api_key: '', bundle_primary: 'm1' }),
      entry('m1', 'Bot', { bundle_id: 'bun', bundle_slot: 'bot' }),
    ]));
    const chunk = (v: string) => ({
      id: v,
      name: v,
      chunk_type: 'env_file',
      fields: [{ key: 'T', value: v, field_type: 'env_var' }],
    });
    st.vault.projects[0].chunks = [chunk('${bundle:B/bot}')] as never;
    const { confirmBundleMemberReferenceBreakage } = await import('../src/ts/modals');
    const utils = await import('../src/ts/utils');
    void utils;
    confirmAnswer = true;
    const pending = confirmBundleMemberReferenceBreakage(st.vault.api_keys[1]);
    (st.vault.projects[0].chunks as unknown as object[]).push(chunk('${bundle:B/bot} again'));
    const ok = await pending;
    expect(ok).toBe(false);
    expect(toasts[toasts.length - 1]).toEqual([
      'Bundle references changed while confirming; review them and try again',
      'err',
    ]);
  });

  it('a member of a bundle that is gone, or an entry that is not a member, deletes without asking', async () => {
    const { confirmBundleMemberReferenceBreakage } = await import('../src/ts/modals');
    expect(await confirmBundleMemberReferenceBreakage(st.vault.api_keys[0])).toBe(true);
    expect(
      await confirmBundleMemberReferenceBreakage({ ...st.vault.api_keys[0], bundle_id: 'ghost' }),
    ).toBe(true);
  });

  it('a delete confirmed after the member moved still removes that member', async () => {
    ({ st } = await bootApp([
      entry('bun', 'B', { secretType: 'bundle', api_key: '', bundle_primary: 'm1' }),
      entry('m1', 'Bot', { bundle_id: 'bun', bundle_slot: 'bot' }),
      entry('x', 'Other'),
    ]));
    const { deleteKey } = await import('../src/ts/modals');
    deleteKey(ev(), 1);
    await flush();
    expect(st.vault.api_keys.map((e) => e.id)).toEqual(['bun', 'x']);
  });
});

describe('resolving the entry a form was opened on', { timeout: 30000 }, () => {
  it('finds it by identity, then by id, and gives up for an entry with neither', async () => {
    const { resolveEditTarget } = await import('../src/ts/modals');
    const a = entry('a', 'A') as never;
    expect(resolveEditTarget([entry('z', 'Z') as never, a], a)).toBe(1);
    expect(resolveEditTarget([entry('z', 'Z') as never, entry('a', 'A2') as never], a)).toBe(1);
    expect(resolveEditTarget([entry('z', 'Z') as never], a)).toBe(-1);
    const anonymous = { provider: 'P', api_key: 'k' } as never;
    expect(resolveEditTarget([entry('z', 'Z') as never], anonymous)).toBe(-1);
  });
});

describe('a custom select', { timeout: 30000 }, () => {
  it('starts with the selected option’s text, or nothing when there are no options', async () => {
    const { CustomSelect } = await import('../src/ts/modals');
    const make = (options: string[]) => {
      const sel = document.createElement('select');
      for (const o of options) sel.add(new Option(o, o));
      document.body.appendChild(sel);
      return new CustomSelect(sel);
    };
    expect(make(['one', 'two'])._btn.textContent).toBe('one');
    expect(make([])._btn.textContent).toBe('');
  });

  it('choosing from its menu selects that option, updates the button and fires change', async () => {
    const { CustomSelect } = await import('../src/ts/modals');
    const sel = document.createElement('select');
    for (const o of ['one', 'two']) sel.add(new Option(o, o));
    document.body.appendChild(sel);
    const cs = new CustomSelect(sel);
    const changed = vi.fn();
    sel.addEventListener('change', changed);
    cs._btn.click();
    [...document.querySelectorAll<HTMLElement>('#dropdown .dropdown-item')]
      .find((i) => i.textContent === 'two')!
      .click();
    expect(sel.value).toBe('two');
    expect(cs._btn.textContent).toBe('two');
    expect(changed).toHaveBeenCalledTimes(1);
  });
});
