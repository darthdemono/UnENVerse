/** Secret-free entry lifecycle viewer shared by the local and remote app. */
import type { VaultEntry } from './types';
import { RemoteVaultStore, st } from './state';
import { invokeTauri, isTauri } from './tauri';
import { showPromptLarge, showToast } from './utils';

type TimelineEvent = {
  kind: string;
  at: string;
  at_precision: string;
  source: string;
  detail: Record<string, unknown>;
};

function display(events: TimelineEvent[]): string {
  if (!events.length) return 'No lifecycle events yet.';
  return events
    .map((event) => {
      const detail = Object.entries(event.detail)
        .map(([key, value]) => `${key}=${String(value)}`)
        .join(' ');
      return `${event.at}  ${event.kind}  ${event.source}${detail ? `  ${detail}` : ''}`;
    })
    .join('\n');
}

/** Opens a read-only-in-practice detail view. The textarea's edits are discarded. */
export async function showEntryTimeline(entry: VaultEntry): Promise<void> {
  if (!entry.id) {
    showToast('Save this entry once before viewing its lifecycle timeline.', 'info');
    return;
  }
  try {
    let events: TimelineEvent[] | null = null;
    if (st.store instanceof RemoteVaultStore) {
      events = await st.store.api<TimelineEvent[]>(
        `/api/vault/entries/${encodeURIComponent(entry.id)}/timeline`,
      );
    } else if (isTauri()) {
      events = await invokeTauri<TimelineEvent[]>('entry_timeline', { entryId: entry.id });
    } else {
      showToast(
        'The lifecycle timeline is available in the desktop app or a remote vault.',
        'info',
      );
      return;
    }
    await showPromptLarge(`${entry.provider} lifecycle timeline`, display(events ?? []));
  } catch (error) {
    showToast(error instanceof Error ? error.message : String(error), 'err');
  }
}
