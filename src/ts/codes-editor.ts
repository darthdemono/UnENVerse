// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
/**
 * The recovery codes list in the entry form: one row per code with a "used" tick and the time it
 * was used. It reads and writes the same `codes` text as `recovery-codes.ts` (one code per line,
 * a used one followed by a tab and `USED <time>`), so any entry can carry codes beside its password
 * and 2FA seed. A used time that was not touched is written back exactly as it was read.
 */
import { html, setHtml } from './html';
import { invokeTauri, isTauri } from './tauri';

const list = (): HTMLElement | null => document.getElementById('f-codes-list');

/** An ISO time as the local `YYYY-MM-DDTHH:mm` a datetime-local input takes; '' when unreadable. */
export function toLocalInput(iso: string): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return '';
  const p = (n: number) => String(n).padStart(2, '0');
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}T${p(d.getHours())}:${p(d.getMinutes())}`;
}

function renumber(): void {
  list()
    ?.querySelectorAll<HTMLElement>('.code-row')
    .forEach((row, i) => {
      row.querySelector('.code-n')!.textContent = String(i + 1);
      row.querySelector('.code-text')!.setAttribute('aria-label', `Code ${i + 1}`);
      row.querySelector('.code-when')!.setAttribute('aria-label', `Time code ${i + 1} was used`);
    });
}

function addRow(code = '', used: string | null = null): HTMLElement {
  const row = document.createElement('div');
  row.className = 'code-row';
  setHtml(
    row,
    html`<span class="code-n"></span>
      <input class="form-input mono code-text" autocomplete="off" spellcheck="false" value="${code}" />
      <label class="extra-var-secret-label"
        ><input type="checkbox" class="code-used" ${used === null ? '' : 'checked'} /> used</label
      >
      <input type="datetime-local" class="form-input code-when" ${used === null ? 'hidden' : ''} />
      <button type="button" class="icon-btn sm code-remove" aria-label="Remove this code">×</button>`,
  );
  const when = row.querySelector<HTMLInputElement>('.code-when')!;
  const tick = row.querySelector<HTMLInputElement>('.code-used')!;
  if (used !== null) {
    row.dataset.orig = used;
    when.value = toLocalInput(used);
  }
  tick.onchange = () => {
    when.hidden = !tick.checked;
    if (tick.checked && !when.value) when.value = toLocalInput(new Date().toISOString());
  };
  row.querySelector<HTMLElement>('.code-remove')!.onclick = () => {
    row.remove();
    renumber();
  };
  list()?.appendChild(row);
  renumber();
  return row;
}

/** Shows `text` (the stored `codes` value) as rows, replacing what was there. */
export function fillCodes(text: string): void {
  const box = list();
  if (!box) return;
  setHtml(box, '');
  for (const line of text.split('\n')) {
    if (!line.trim()) continue;
    const [code, mark] = line.replace(/\s+$/, '').split('\t');
    addRow(code, mark === undefined ? null : mark.replace(/^USED\s*/, ''));
  }
}

/** The `codes` value the rows hold now; '' when there are none. */
export function readCodes(): string {
  const lines: string[] = [];
  list()
    ?.querySelectorAll<HTMLElement>('.code-row')
    .forEach((row) => {
      const code = row
        .querySelector<HTMLInputElement>('.code-text')!
        .value.replace(/\s+/g, ' ')
        .trim();
      if (!code) return;
      if (!row.querySelector<HTMLInputElement>('.code-used')!.checked) {
        lines.push(code);
        return;
      }
      const when = row.querySelector<HTMLInputElement>('.code-when')!.value;
      const orig = row.dataset.orig;
      const stamp =
        orig && when === toLocalInput(orig)
          ? orig
          : when
            ? new Date(when).toISOString()
            : new Date().toISOString();
      lines.push(`${code}\tUSED ${stamp}`);
    });
  return lines.join('\n');
}

/** Adds pasted codes as unused rows; a code already in the list is skipped. Returns how many. */
export async function addPasted(text: string): Promise<number> {
  const found = isTauri()
    ? await invokeTauri<string[]>('codes_parse', { text })
    : text.split(/[\s,;]+/).filter(Boolean);
  const have = new Set(
    [...(list()?.querySelectorAll<HTMLInputElement>('.code-text') ?? [])].map((i) =>
      i.value.trim(),
    ),
  );
  let added = 0;
  for (const c of found) {
    if (have.has(c)) continue;
    have.add(c);
    addRow(c);
    added++;
  }
  return added;
}

/** Wires the two buttons and the phrase "show" tick. Handlers are assigned, not added. */
export function initCodesEditor(onError: (err: unknown) => void): void {
  const add = document.getElementById('f-codes-add');
  if (add) add.onclick = () => addRow().querySelector<HTMLInputElement>('.code-text')!.focus();
  const paste = document.getElementById('f-codes-paste-add');
  if (paste)
    paste.onclick = () => {
      const box = document.getElementById('f-codes-paste') as HTMLTextAreaElement;
      void addPasted(box.value).then(() => (box.value = ''), onError);
    };
  const show = document.getElementById('f-phrase-show') as HTMLInputElement | null;
  if (show)
    show.onchange = () => {
      (document.getElementById('f-phrase') as HTMLInputElement).type = show.checked
        ? 'text'
        : 'password';
    };
}
