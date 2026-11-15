// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
/**
 * The Tools, Glossary pane: search what every term in the app means. The text is the same file the
 * website glossary and `unv glossary` read, so the three cannot disagree.
 */
import { html, setHtml } from './html';
import { categories, searchTerms, type GlossaryTerm } from './glossary';

const SHOWN = 100;

function entry(t: GlossaryTerm) {
  return html`<div class="gloss-item" id="gloss-${t.id}">
    <div class="gloss-head">
      <strong>${t.term}</strong> <span class="gloss-cat">${t.category}</span>
    </div>
    <p>${t.definition}</p>
    ${t.example ? html`<p class="gloss-small">Example: <code>${t.example}</code></p>` : ''}
    ${
      t.aliases?.length
        ? html`<p class="gloss-small">Also written: ${t.aliases.join(', ')}</p>`
        : ''
    }
  </div>`;
}

/** Renders the list for what the search box and category picker say now. */
export function renderGlossary(): void {
  const q = document.getElementById('glossary-q') as HTMLInputElement | null;
  const cat = document.getElementById('glossary-cat') as HTMLSelectElement | null;
  const list = document.getElementById('glossary-list');
  const status = document.getElementById('glossary-status');
  if (!q || !cat || !list || !status) return;
  const found = searchTerms(q.value, cat.value, SHOWN + 1);
  const shown = found.slice(0, SHOWN);
  setHtml(list, html`${shown.map(entry)}`);
  status.textContent = !found.length
    ? 'No term matches. Try a field name such as api_key, or a word from what you are looking for.'
    : found.length > SHOWN
      ? `Showing the first ${SHOWN}. Type more to narrow the list.`
      : `${found.length} term${found.length === 1 ? '' : 's'}.`;
}

/** Wires the pane once; every later call only repaints. Handlers are assigned, not added. */
export function initGlossaryPane(): void {
  const q = document.getElementById('glossary-q') as HTMLInputElement | null;
  const cat = document.getElementById('glossary-cat') as HTMLSelectElement | null;
  if (!q || !cat) return;
  if (cat.options.length <= 1) {
    for (const c of categories()) cat.add(new Option(c, c));
  }
  q.oninput = renderGlossary;
  cat.onchange = renderGlossary;
  renderGlossary();
}
