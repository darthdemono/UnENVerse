// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
/**
 * Tools, Recovery codes (R36): the single-use codes of every account in one place.
 *
 * Reading never spends a code: it is marked used only by the button, after a question. Pasted text
 * is split into codes by Rust (`codes_parse`, `codes_add`, `codes_unmark` over IPC), the same code
 * `unv codes import|unuse` run; this pane only holds the vault and shows what Rust answers. The
 * list is masked until "Show the codes" is ticked, and the printable sheet is built only when
 * Print is pressed and is emptied afterwards.
 */
import { html, setHtml } from './html';
import { codeStatus, listCodes, markUsed } from './recovery-codes';
import { persist, st } from './state';
import { invokeTauri, isTauri } from './tauri';
import type { VaultEntry } from './types';
import { errorMessage, showConfirm, showToast } from './utils';

const el = <T extends HTMLElement = HTMLElement>(id: string) =>
  document.getElementById(id) as T | null;
let selectedId = '';

const accounts = (): VaultEntry[] =>
  st.vault.api_keys.filter((e) => e.secretType === 'recovery_codes');
/** Looked up again by id each time: a dialog or a save may have replaced the document. */
const current = (): VaultEntry | undefined => accounts().find((e) => e.id === selectedId);

function say(msg: string, kind: 'ok' | 'err' = 'ok'): void {
  const s = el('cod-status');
  if (!s) return;
  s.textContent = msg;
  s.className = `tool-status ${kind}`;
}

/** Writes a new `codes` value into the entry (creating the variable when it is missing). */
function store(entry: VaultEntry, value: string): void {
  const vars = (entry.extra_vars ??= []);
  const slot = vars.find((v) => v.key === 'codes');
  if (slot) slot.value = value;
  else vars.push({ key: 'codes', value, secret: true });
}

export function paintCodes(): void {
  const select = el<HTMLSelectElement>('cod-entry');
  const list = el('cod-list');
  if (!select || !list) return;
  const all = accounts();
  if (!all.some((e) => e.id === selectedId)) selectedId = (all[0]?.id as string) ?? '';
  setHtml(
    select,
    html`${all.map((e) => html`<option value="${e.id}" ${e.id === selectedId ? 'selected' : ''}>${e.provider}</option>`)}`,
  );
  const entry = current();
  if (!entry) {
    setHtml(list, html``);
    say('No recovery codes entry yet. Add one with the type "Recovery Codes".');
    return;
  }
  const { total, remaining } = codeStatus(entry);
  const show = el<HTMLInputElement>('cod-reveal')?.checked ?? false;
  say(`${remaining} of ${total} unused`, remaining <= 2 && total > 0 ? 'err' : 'ok');
  setHtml(
    list,
    html`${listCodes(entry).map(
      (c) => html`<div class="cod-code${c.used === null ? '' : ' used'}">
        <span>${show ? c.code : '•'.repeat(Math.min(c.code.length, 12))}</span>
        ${
          c.used === null
            ? html``
            : html`<span class="rcp-dim">used ${c.used}</span
                ><button class="btn btn-ghost btn-sm" data-action="cod-restore" data-code="${c.code}">Restore</button>`
        }
      </div>`,
    )}`,
  );
  const use = el<HTMLButtonElement>('cod-use');
  if (use) use.disabled = remaining === 0;
}

async function useNext(): Promise<void> {
  const entry = current();
  if (!entry) return;
  if (!(await showConfirm(`Mark the next recovery code as used for "${entry.provider}"?`))) return;
  // The dialog was an await: mark the entry that is in the vault now.
  const live = current();
  if (!live) return;
  try {
    store(live, markUsed(live, null, new Date().toISOString().slice(0, 10)));
  } catch (err) {
    showToast(errorMessage(err), 'err');
    return;
  }
  await persist();
  paintCodes();
  showToast(`${codeStatus(live).remaining} recovery codes left`, 'ok');
}

async function restore(code: string): Promise<void> {
  const entry = current();
  if (!entry) return;
  if (!isTauri()) {
    showToast('Needs the desktop app.', 'err');
    return;
  }
  try {
    store(entry, await invokeTauri<string>('codes_unmark', { entry, code }));
  } catch (err) {
    showToast(errorMessage(err), 'err');
    return;
  }
  await persist();
  paintCodes();
  showToast('Code restored', 'ok');
}

async function importPasted(): Promise<void> {
  const entry = current();
  const box = el<HTMLTextAreaElement>('cod-paste');
  if (!entry || !box) return;
  if (!isTauri()) {
    showToast('Needs the desktop app.', 'err');
    return;
  }
  try {
    const found = await invokeTauri<string[]>('codes_parse', { text: box.value });
    if (found.length === 0) {
      showToast('No recovery codes found in that text', 'err');
      return;
    }
    const done = await invokeTauri<{ value: string; added: number }>('codes_add', {
      entry,
      codes: found,
    });
    store(entry, done.value);
    box.value = '';
    await persist();
    paintCodes();
    showToast(`${done.added} added (${found.length} found)`, 'ok');
  } catch (err) {
    showToast(errorMessage(err), 'err');
  }
}

/** Builds the sheet, prints it, and empties it again. Unused codes only; nothing leaves the page. */
function printSheet(): void {
  const entry = current();
  const sheet = el('cod-sheet');
  if (!entry || !sheet) return;
  const unused = listCodes(entry).filter((c) => c.used === null);
  sheet.textContent = `${entry.provider}\n${unused.map((c, i) => `${i + 1}. ${c.code}`).join('\n')}`;
  document.body.classList.add('printing-codes');
  try {
    window.print();
  } finally {
    document.body.classList.remove('printing-codes');
    sheet.textContent = '';
  }
}

/** Wires the pane once; every later call only repaints. Handlers are assigned, not added. */
export function initCodesPane(): void {
  if (!el('cod-list')) return;
  paintCodes();
  el<HTMLSelectElement>('cod-entry')!.onchange = (e) => {
    selectedId = (e.target as HTMLSelectElement).value;
    paintCodes();
  };
  el('cod-reveal')!.onchange = paintCodes;
  el('cod-use')!.onclick = () => void useNext();
  el('cod-import')!.onclick = () => void importPasted();
  el('cod-print')!.onclick = printSheet;
  el('cod-list')!.onclick = (e) => {
    const t = (e.target as HTMLElement).closest<HTMLElement>('[data-action="cod-restore"]');
    if (t) void restore(t.dataset.code ?? '');
  };
}
