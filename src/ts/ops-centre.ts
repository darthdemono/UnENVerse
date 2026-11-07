// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
/**
 * Tools -> Operations (R15): one page for what the app is doing or waiting on. Backup and restore
 * runs, pushes held for a person's approval, what each node last reported for each secret, secrets
 * that were rotated but not yet redeployed, and the leases holding deployment targets. Names and
 * states only: nothing on this page is a secret value, and every cell goes through the escaping
 * `html` template, because names come from the vault.
 */
import { html, setHtml } from './html';
import { showConfirm, showToast } from './utils';
import { invokeTauri, isTauri } from './tauri';

interface Run {
  id: string;
  kind: string;
  state: string;
  outcome?: string | null;
  error_msg?: string | null;
  created_at: string;
  finished_at?: string | null;
}
interface Approval {
  id: string;
  node_id: string;
  target: string;
  sha256: string;
}
interface Deployment {
  entry: string;
  field: string;
  target_id: string;
  generation: number;
  result: string;
  applied_at?: string | null;
  checked_at?: string | null;
}
interface Mismatch {
  entry: string;
  field: string;
  target_id: string;
  deployed_generation: number;
  active_generation: number;
}
interface Lease {
  node_id: string;
  target: string;
  holder: string;
  expires_secs: number;
}
export interface Overview {
  runs: Run[];
  approvals: Approval[];
  deployments: Deployment[];
  mismatches: Mismatch[];
  leases: Lease[];
  nodes: number;
}

/** States after which a run no longer changes. */
export const FINAL_STATES = ['succeeded', 'failed', 'skipped_stale', 'cancelled'];

const $ = (id: string) => document.getElementById(id) as HTMLElement;

function say(msg: string, kind: 'ok' | 'err' = 'ok'): void {
  const el = $('ops-status');
  el.textContent = msg;
  el.className = `tool-status ${kind}`;
}

/** One-line summary above the lists. */
export function summary(o: Overview): string {
  const active = o.runs.filter((r) => !FINAL_STATES.includes(r.state)).length;
  return `${active} running or waiting · ${o.approvals.length} held for approval · ${o.mismatches.length} rotated but not redeployed · ${o.leases.length} lease${o.leases.length === 1 ? '' : 's'} · ${o.nodes} node${o.nodes === 1 ? '' : 's'}`;
}

function runRow(r: Run) {
  const note = r.outcome ?? r.error_msg ?? '';
  const open = !FINAL_STATES.includes(r.state);
  return html`<tr>
    <td class="mono">${r.kind}</td>
    <td><strong>${r.state}</strong>${note ? html` <em>(${note})</em>` : ''}</td>
    <td>${r.created_at}</td>
    <td>
      ${
        open
          ? html`<button class="btn btn-ghost btn-sm" data-ops-cancel="${r.id}">Cancel</button>`
          : ''
      }
    </td>
  </tr>`;
}

/** The page body for an overview. Exported so the markup can be tested without the desktop. */
export function render(o: Overview) {
  const empty = (text: string) => html`<p class="tool-hint">${text}</p>`;
  return html`
    <section aria-labelledby="ops-h-runs">
      <h4 id="ops-h-runs">Runs</h4>
      ${
        o.runs.length
          ? html`<table class="tool-table">
            <caption class="sr-only">
              Backup and restore runs, newest first
            </caption>
            <thead>
              <tr>
                <th scope="col">Kind</th>
                <th scope="col">State</th>
                <th scope="col">Started</th>
                <th scope="col"><span class="sr-only">Actions</span></th>
              </tr>
            </thead>
            <tbody>
              ${o.runs.map(runRow)}
            </tbody>
          </table>`
          : empty('No runs yet.')
      }
    </section>
    <section aria-labelledby="ops-h-approvals">
      <h4 id="ops-h-approvals">Held for your approval</h4>
      ${
        o.approvals.length
          ? html`<ul>
            ${o.approvals.map(
              (a) =>
                html`<li><span class="mono">${a.target}</span> on node ${a.node_id} ·
                  <span class="mono">${a.sha256.slice(0, 12)}</span></li>`,
            )}
          </ul>
          <p class="tool-hint">Review and decide them in Tools, Nodes.</p>`
          : empty('Nothing is waiting for you.')
      }
    </section>
    <section aria-labelledby="ops-h-mismatch">
      <h4 id="ops-h-mismatch">Rotated but not redeployed</h4>
      ${
        o.mismatches.length
          ? html`<ul>
            ${o.mismatches.map(
              (m) =>
                html`<li><strong>${m.entry}</strong> · ${m.field} on
                  <span class="mono">${m.target_id}</span>: deployed generation
                  ${m.deployed_generation}, active ${m.active_generation}</li>`,
            )}
          </ul>`
          : empty('Every reported deployment is on the active generation.')
      }
    </section>
    <section aria-labelledby="ops-h-deploy">
      <h4 id="ops-h-deploy">Last reported by nodes</h4>
      ${
        o.deployments.length
          ? html`<ul>
            ${o.deployments.map(
              (d) =>
                html`<li><strong>${d.entry}</strong> · ${d.field} on
                  <span class="mono">${d.target_id}</span>: ${d.result}${
                    d.checked_at ? html` (checked ${d.checked_at})` : ''
                  }</li>`,
            )}
          </ul>`
          : empty('No node has reported a deployment yet.')
      }
    </section>
    <section aria-labelledby="ops-h-leases">
      <h4 id="ops-h-leases">Leases</h4>
      ${
        o.leases.length
          ? html`<ul>
            ${o.leases.map(
              (l) =>
                html`<li><span class="mono">${l.target}</span> on node ${l.node_id}, held by
                  ${l.holder}</li>`,
            )}
          </ul>`
          : empty('No target is leased.')
      }
    </section>
  `;
}

export async function refreshOperations(): Promise<void> {
  if (!document.getElementById('ops-list')) return;
  if (!isTauri()) {
    say('The Operations centre is in the desktop app. In a terminal: unv ops ls.', 'err');
    setHtml($('ops-list'), '');
    return;
  }
  say('Working…');
  try {
    const o = await invokeTauri<Overview>('operations_overview');
    setHtml($('ops-list'), render(o));
    say(summary(o));
  } catch (e) {
    const msg = e instanceof Error ? e.message : String(e);
    setHtml($('ops-list'), '');
    say(
      msg.includes('locked') ? 'Unlock the vault to see its operations.' : `Could not read: ${msg}`,
      'err',
    );
  }
}

async function cancelRun(id: string): Promise<void> {
  if (!(await showConfirm('Cancel this run? A step already sent to a destination is not undone.')))
    return;
  try {
    await invokeTauri('ops_cancel', { id });
    showToast('Run cancelled', 'ok', 2000);
  } catch (e) {
    showToast(`Not cancelled: ${e instanceof Error ? e.message : String(e)}`, 'err', 4000);
  }
  await refreshOperations();
}

export function initOpsCentre(): void {
  const refresh = document.getElementById('ops-refresh-btn');
  const list = document.getElementById('ops-list');
  if (!refresh || !list) return;
  // Assignment, not addEventListener: panes are initialised again (invariant 9).
  refresh.onclick = () => void refreshOperations();
  list.onclick = (e) => {
    const btn = (e.target as HTMLElement).closest<HTMLElement>('[data-ops-cancel]');
    if (btn?.dataset.opsCancel) void cancelRun(btn.dataset.opsCancel);
  };
}
