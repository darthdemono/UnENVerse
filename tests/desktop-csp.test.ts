// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
// R15: the desktop window may not open its own connections to arbitrary https hosts. Every https
// request the page makes goes through the Rust `remote_request` command (which pins the
// certificate), so the webview's `connect-src` has no reason to allow them. Plain http remains,
// because a LAN or localhost remote has no certificate to pin and uses the page's fetch.
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
  it('lets the page connect to its own origin, IPC and plain http, but not to any https host', () => {
    const connect = directive('connect-src');
    expect(connect).toEqual(expect.arrayContaining(["'self'", 'ipc:', 'http:']));
    expect(connect).not.toContain('https:');
    expect(connect).not.toContain('*');
  });

  it('keeps scripts to the bundled files', () => {
    expect(directive('script-src')).toEqual(["'self'"]);
  });

  it('sends every https request through the Rust proxy', () => {
    // The reason the https allowance could go: RemoteVaultStore uses remote_request for https.
    const state = readFileSync(join(process.cwd(), 'src', 'ts', 'state.ts'), 'utf8');
    expect(state).toContain("isTauri() && url.startsWith('https://')");
    expect(state).toContain("'remote_request'");
  });
});
