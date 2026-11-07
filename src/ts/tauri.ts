// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
/** Minimal typed boundary for Tauri's runtime-injected IPC bridge. */
type Invoke = (command: string, args?: unknown) => Promise<unknown>;

type TauriRuntime = { core?: { invoke?: Invoke } };
type TauriWindow = Window & { __TAURI__?: TauriRuntime; __TAURI_INTERNALS__?: { invoke?: Invoke } };

/**
 * `window.__TAURI__` exists only while `withGlobalTauri` is on, which the desktop build turns off
 * (R15: no global handle for a script to find). The IPC bridge Tauri always injects,
 * `__TAURI_INTERNALS__.invoke`, is what `@tauri-apps/api` itself calls, so it is the fallback.
 */
function currentInvoke(): Invoke | undefined {
  const w = window as TauriWindow;
  return w.__TAURI__?.core?.invoke ?? w.__TAURI_INTERNALS__?.invoke;
}

/** Read the runtime at call time; tests, hot reload and late webview injection can replace it. */
export function isTauri(): boolean {
  return currentInvoke() !== undefined;
}

export const inTauri = isTauri();

export async function invokeTauri<T>(command: string, args?: unknown): Promise<T> {
  const invoke = currentInvoke();
  if (!invoke) throw new Error('Tauri IPC is unavailable');
  return invoke(command, args) as Promise<T>;
}
