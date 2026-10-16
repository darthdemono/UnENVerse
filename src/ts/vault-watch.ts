/**
 * @file
 * Notice when something *else* writes the vault, and pick the change up.
 *
 * The desktop app reads the vault once at unlock and then holds it in memory.
 * Nothing told it that `unv entry set` in a terminal, `unv totp advance`, or a
 * LAN peer had written to the same database — so the window went on showing the
 * state it read at unlock, and the next save either overwrote the other writer's
 * work or (since the compare-and-swap landed) failed with a conflict the user
 * had no way to have predicted.
 *
 * ## How it detects a change
 *
 * `vault_meta.data_hash` is written in the same transaction as the data, so it
 * is by construction the hash of exactly the bytes on disk — and it is already
 * what the compare-and-swap compares. The store keeps the version it last read
 * or wrote; this polls the stored one and reloads when the two differ. No file
 * watcher, no mtime heuristics, and nothing new that could disagree with the
 * CAS about what "the current version" means.
 *
 * A poll rather than a pushed event because the answer is one indexed `SELECT`
 * against a table with a handful of rows, and because a Rust-side watcher would
 * be a second definition of "changed" to keep in step with the first.
 *
 * ## What it will not do
 *
 * **It never reloads over an open editor.** A reload replaces `st.vault`, and
 * doing that under a half-typed entry throws the typing away — which is worse
 * than being briefly out of date. While any overlay is open the watcher waits
 * and tries again on the next tick.
 *
 * **It does not touch view state.** This is the same vault with new contents,
 * not a different vault, so `resetViewState()` — which is for when the data is
 * *replaced* — would wrongly drop the user's filters and expanded cards
 * mid-session. Ids that no longer resolve are already handled by the renderers
 * (invariant 1) and by `restoreViewState`'s validation (invariant 7).
 *
 * **Local vaults only.** A remote vault's writes come back through its own API
 * and `RemoteVaultStore` has no equivalent marker to poll; watching one would
 * mean a request every few seconds against a server that may be on a phone
 * tether. Written down because an unrecorded gap is indistinguishable from an
 * oversight (invariant 10).
 */

import { st, inTauri } from './state';
import { showToast } from './utils';
import { invokeTauri } from './tauri';

/**
 * How often to ask. One indexed SELECT, so cheap enough for a change made in a
 * terminal to show up in the window within a blink; 3 s read as "slow".
 */
const POLL_MS = 750;

let timer: ReturnType<typeof setInterval> | null = null;
/** Set while a reload is in flight, so a slow load cannot overlap the next tick. */
let reloading = false;

const invoke = invokeTauri;

/** True while any modal is open — see the file header on why that blocks a reload. */
function editorOpen(): boolean {
  return !!document.querySelector('.overlay.open, .modal-overlay.open');
}

/**
 * One check. Exported for the tests, which cannot wait three seconds.
 *
 * Never throws: it runs on a timer, and a rejected promise per tick is a console
 * nobody reads.
 */
export async function checkVaultVersion(): Promise<boolean> {
  if (reloading || !st.vaultOpen || st.store?.isRemote) return false;
  try {
    const stored = (await invoke('vault_version')) as string | null | undefined;
    // `null` means locked. Nothing to compare against, and nothing to reload.
    if (!stored) return false;
    const ours = (st.store as unknown as { lastVersion?: string | null }).lastVersion ?? null;
    if (!ours || stored === ours) return false;
    if (editorOpen()) return false;

    reloading = true;
    const data = await st.store.load();
    if (!data) return false;
    st.vault = data;
    const { triggerRender } = await import('./state');
    triggerRender();
    showToast('Vault updated outside this window — reloaded');
    return true;
  } catch {
    return false;
  } finally {
    reloading = false;
  }
}

/**
 * Start watching. Idempotent by assignment (invariant 9): calling it on every
 * unlock leaves one interval, not one per unlock.
 */
export function startVaultWatch(): void {
  if (timer !== null || !inTauri) return;
  timer = setInterval(() => {
    // A hidden window cannot show the change; the focus hook below catches up.
    if (!document.hidden) void checkVaultVersion();
  }, POLL_MS);
  // Coming back from the terminal that made the change: look at once rather
  // than waiting for the next tick.
  window.addEventListener('focus', onFocus);
}

function onFocus(): void {
  void checkVaultVersion();
}

/** Stop watching. Called on lock: a locked vault has nothing to reload into. */
export function stopVaultWatch(): void {
  if (timer !== null) {
    clearInterval(timer);
    timer = null;
    window.removeEventListener('focus', onFocus);
  }
}
