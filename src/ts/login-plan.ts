// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
/**
 * The login steps of a bundle on its card (R36): password, 2FA code, security phrase, recovery code,
 * in the one order `vault_core::login` decides (reached over IPC; there is no twin of it here), and
 * the bundle's `terminate_codes` choice for what happens to a recovery code once it has been filled.
 * The plan names steps and never carries a value. The section is added only when the desktop can
 * answer, and removes itself when the bundle has fewer than two steps.
 */
import { html, setHtml } from './html';
import { persist, st } from './state';
import { invokeTauri, isTauri } from './tauri';
import type { VaultEntry } from './types';
import { errorMessage, showToast } from './utils';

interface Plan {
  steps: { step: string; label: string }[];
  terminate: 'ask' | 'auto' | 'never';
}

const CHOICES: [Plan['terminate'], string][] = [
  ['ask', 'Ask me whether to mark it used'],
  ['auto', 'Mark it used straight away'],
  ['never', 'Leave it for me to mark'],
];

export function loginPlanSection(bundle: VaultEntry, members: VaultEntry[]): HTMLElement | null {
  if (!isTauri() || members.length < 2) return null;
  const section = document.createElement('section');
  section.className = 'bundle-login-plan';
  section.setAttribute('aria-label', 'Login steps');
  void fill(section, bundle, members);
  return section;
}

async function fill(
  section: HTMLElement,
  bundle: VaultEntry,
  members: VaultEntry[],
): Promise<void> {
  let plan: Plan;
  try {
    plan = await invokeTauri<Plan>('login_plan', { bundle, entries: members });
  } catch {
    section.remove();
    return;
  }
  if (plan.steps.length < 2) {
    section.remove();
    return;
  }
  const hasCode = plan.steps.some((s) => s.step === 'recovery_code');
  const selectId = `login-terminate-${String(bundle.id)}`;
  setHtml(
    section,
    html`<div class="key-row">
        <div class="key-label">LOGIN</div>
        <div class="key-value revealed">${plan.steps.map((s) => s.label).join(' → ')}</div>
      </div>${
        hasCode
          ? html`<div class="key-row">
              <label class="key-label" for="${selectId}">CODE USED</label>
              <select id="${selectId}" class="form-input">${CHOICES.map(
                ([v, label]) =>
                  html`<option value="${v}" ${v === plan.terminate ? 'selected' : ''}>${label}</option>`,
              )}</select>
            </div>`
          : ''
      }`,
  );
  const select = section.querySelector<HTMLSelectElement>('select');
  if (!select) return;
  select.onchange = () => {
    void (async () => {
      // Look the bundle up again by id: the document may have been replaced since the card was drawn.
      const live = st.vault.api_keys.find((e) => e.id === bundle.id);
      if (!live) return;
      try {
        live.terminate_codes = select.value as Plan['terminate'];
        await persist();
        showToast('Saved', 'ok');
      } catch (err) {
        showToast(errorMessage(err), 'err');
      }
    })();
  };
}
