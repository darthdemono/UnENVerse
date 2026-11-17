// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
/**
 * Tools, Site families (R40): sets of hosts that share one sign-in, such as MusicBrainz and its
 * sister sites. An entry that names a family is offered on every host in it and on subdomains,
 * never on a look-alike. The hosts are normalised by Rust (`site_family_normalise`, the parser
 * `unv site family add` uses); this pane only stores what Rust answers. The shipped families are
 * read-only and a family of yours with the same name replaces the shipped one.
 */
import { html, setHtml } from './html';
import { persist, st } from './state';
import { invokeTauri, isTauri } from './tauri';
import shipped from '../../vault-core/data/site-families.json';
import { errorMessage, showConfirm, showToast } from './utils';

interface Family {
  name: string;
  hosts: string[];
}

const el = <T extends HTMLElement = HTMLElement>(id: string) =>
  document.getElementById(id) as T | null;
const doc = () => st.vault as unknown as { site_families?: Family[] };

function say(msg: string, kind: 'ok' | 'err' = 'ok'): void {
  const s = el('fam-status');
  if (!s) return;
  s.textContent = msg;
  s.className = `tool-status ${kind}`;
}

export function paintFamilies(): void {
  const box = el('fam-list');
  if (!box) return;
  const mine = doc().site_families ?? [];
  const names = new Set(mine.map((f) => f.name));
  say(`${mine.length} of your own, ${shipped.families.length} shipped`);
  setHtml(
    box,
    html`${mine.map(
      (f) => html`<div class="rcp-row">
        <strong>${f.name}</strong>
        ${f.hosts.map(
          (h) => html`<span class="rcp-dim">${h}</span>
            <button class="btn btn-ghost btn-sm" data-action="fam-host-rm" data-name="${f.name}" data-host="${h}" aria-label="${`Remove ${h} from ${f.name}`}">Remove</button>`,
        )}
        <button class="btn btn-ghost btn-sm" data-action="fam-rm" data-name="${f.name}" aria-label="${`Remove family ${f.name}`}">Remove family</button>
      </div>`,
    )}${shipped.families.map(
      (f) => html`<div class="rcp-row">
        <strong>${f.name}</strong>
        <span class="rcp-dim">${f.hosts.join(', ')}</span>
        <span class="rcp-dim">${names.has(f.name) ? 'shipped, replaced by yours' : 'shipped'}</span>
      </div>`,
    )}`,
  );
}

async function add(): Promise<void> {
  const name = (el<HTMLInputElement>('fam-name')?.value ?? '').trim();
  const hosts = (el<HTMLTextAreaElement>('fam-hosts')?.value ?? '').split(/[\s,]+/).filter(Boolean);
  if (!isTauri()) {
    showToast('Needs the desktop app.', 'err');
    return;
  }
  try {
    const made = await invokeTauri<Family>('site_family_normalise', { name, hosts });
    const d = doc();
    const list = (d.site_families ??= []);
    const have = list.find((f) => f.name === made.name);
    if (have) have.hosts = [...new Set([...have.hosts, ...made.hosts])];
    else list.push(made);
    await persist();
    el<HTMLInputElement>('fam-name')!.value = '';
    el<HTMLTextAreaElement>('fam-hosts')!.value = '';
    paintFamilies();
    showToast(have ? 'Hosts added to the family' : 'Family saved', 'ok');
  } catch (err) {
    showToast(errorMessage(err), 'err');
  }
}

async function remove(action: string, name: string, host: string): Promise<void> {
  const d = doc();
  const list = d.site_families ?? [];
  const fam = list.find((f) => f.name === name);
  if (!fam) return;
  if (action === 'fam-rm') {
    if (!(await showConfirm(`Remove the family "${name}"? Entries that name it stop matching.`)))
      return;
    d.site_families = list.filter((f) => f.name !== name);
  } else {
    fam.hosts = fam.hosts.filter((h) => h !== host);
    if (fam.hosts.length === 0) d.site_families = list.filter((f) => f.name !== name);
  }
  await persist();
  paintFamilies();
  showToast('Removed', 'ok');
}

/** Wires the pane once; every later call only repaints. Handlers are assigned, not added. */
export function initFamiliesPane(): void {
  if (!el('fam-list')) return;
  paintFamilies();
  el('fam-add')!.onclick = () => void add();
  el('fam-list')!.onclick = (e) => {
    const t = (e.target as HTMLElement).closest<HTMLElement>('[data-action^="fam-"]');
    if (t) void remove(t.dataset.action ?? '', t.dataset.name ?? '', t.dataset.host ?? '');
  };
}
