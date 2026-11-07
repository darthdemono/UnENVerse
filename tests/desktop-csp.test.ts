// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
// R15: the desktop window may not open its own connections to any host. Every http and https
// request the page makes goes through the Rust `remote_request` command (which pins the
// certificate when there is one), so the webview's `connect-src` allows only itself and IPC.
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';

const conf = JSON.parse(
  readFileSync(join(process.cwd(), 'src-tauri', 'tauri.conf.json'), 'utf8'),
) as { app: { security: { csp: string } } };
const directive = (name: string) =>
  conf.app.security.csp
    .split(';')
    .map((d) => d.trim())
    .find((d) => d.startsWith(`${name} `))
    ?.split(/\s+/)
    .slice(1) ?? [];

describe('desktop content security policy', () => {
  it('lets the page connect to its own origin and IPC, and to no host', () => {
    const connect = directive('connect-src');
    expect(connect).toEqual(expect.arrayContaining(["'self'", 'ipc:']));
    expect(connect).not.toContain('http:');
    expect(connect).not.toContain('https:');
    expect(connect).not.toContain('*');
  });

  it('does not hand the page a global Tauri object', () => {
    // The bridge is reached through the internals handle instead (src/ts/tauri.ts).
    const all = JSON.parse(
      readFileSync(join(process.cwd(), 'src-tauri', 'tauri.conf.json'), 'utf8'),
    ) as { app: { withGlobalTauri?: boolean } };
    expect(all.app.withGlobalTauri).toBe(false);
  });

  it('keeps scripts to the bundled files', () => {
    expect(directive('script-src')).toEqual(["'self'"]);
  });

  it('sends every http and https request through the Rust proxy', () => {
    // The reason the allowances could go: RemoteVaultStore uses remote_request for both.
    const state = readFileSync(join(process.cwd(), 'src', 'ts', 'state.ts'), 'utf8');
    expect(state).toContain('isTauri() && /^https?:');
    expect(state).toContain("'remote_request'");
  });
});
