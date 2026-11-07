// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
// The browser paste guard (R35 slice 3). The extension is plain scripts with no build, so these
// tests load the same files the browser does. What they pin: the permission policy, the rewrite
// of a paste by position (never by value), and that a paste is held until the host answers.
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { beforeEach, describe, expect, it, vi } from 'vitest';

const dir = join(process.cwd(), 'browser-extension');
const manifest = JSON.parse(readFileSync(join(dir, 'manifest.json'), 'utf8')) as {
  permissions: string[];
  host_permissions?: string[];
  optional_permissions?: string[];
  optional_host_permissions?: string[];
  externally_connectable?: unknown;
  web_accessible_resources?: unknown;
  content_security_policy?: unknown;
  background: { service_worker: string; scripts: string[] };
  content_scripts: { matches: string[]; js: string[] }[];
};

describe('manifest policy', () => {
  it('asks for native messaging and nothing else', () => {
    expect(manifest.permissions).toEqual(['nativeMessaging']);
    expect(manifest.host_permissions).toBeUndefined();
    expect(manifest.optional_permissions).toBeUndefined();
    expect(manifest.optional_host_permissions).toBeUndefined();
  });

  it('exposes nothing to web pages and keeps the default content security policy', () => {
    expect(manifest.externally_connectable).toBeUndefined();
    expect(manifest.web_accessible_resources).toBeUndefined();
    expect(manifest.content_security_policy).toBeUndefined();
  });

  it('runs only on named https chat sites, never on a wildcard host', () => {
    const matches = manifest.content_scripts.flatMap((c) => c.matches);
    expect(matches.length).toBeGreaterThan(0);
    for (const m of matches) {
      expect(m, m).toMatch(/^https:\/\/[a-z0-9.-]+\/\*$/);
      expect(m).not.toContain('*://');
      expect(m).not.toContain('<all_urls>');
      expect(m.split('/')[2]).not.toContain('*');
    }
  });

  it('loads the core before the content script, and every named file exists', () => {
    const js = manifest.content_scripts[0].js;
    expect(js).toEqual(['guard-core.js', 'content.js']);
    for (const f of [...js, manifest.background.service_worker, ...manifest.background.scripts]) {
      expect(() => readFileSync(join(dir, f), 'utf8'), f).not.toThrow();
    }
  });

  it('makes no network request anywhere in the extension', () => {
    for (const f of ['background.js', 'content.js', 'guard-core.js']) {
      const src = readFileSync(join(dir, f), 'utf8');
      expect(src, f).not.toMatch(/\bfetch\s*\(|XMLHttpRequest|WebSocket|sendBeacon|EventSource/);
    }
  });
});

interface Core {
  MIN_CHARS: number;
  shouldCheck: (text: unknown, files: number) => boolean;
  rewrite: (text: string, spans: unknown[]) => string;
  names: (answer: unknown) => string[];
  decide: (answer: unknown) => {
    action: string;
    names?: string[];
    spans?: unknown[];
    reason?: string;
  };
}

async function loadCore(): Promise<Core> {
  // @ts-expect-error plain browser script without types
  await import('../browser-extension/guard-core.js');
  return (globalThis as unknown as { UnvGuardCore: Core }).UnvGuardCore;
}

describe('guard core', () => {
  let core: Core;
  beforeEach(async () => {
    core = await loadCore();
  });

  it('checks only a text paste of useful length with no files', () => {
    expect(core.shouldCheck('x'.repeat(core.MIN_CHARS), 0)).toBe(true);
    expect(core.shouldCheck('x'.repeat(core.MIN_CHARS - 1), 0)).toBe(false);
    expect(core.shouldCheck('x'.repeat(40), 1)).toBe(false);
    expect(core.shouldCheck(undefined, 0)).toBe(false);
  });

  it('rewrites by position, right to left, with a reference or a visible marker', () => {
    const text = 'key=AAAAAA and BBBBBB end';
    const out = core.rewrite(text, [
      { start: 4, end: 10, ref: '${GitHub}' },
      { start: 15, end: 21 },
    ]);
    expect(out).toBe('key=${GitHub} and [secret removed] end');
  });

  it('ignores spans that are out of range, empty, or overlap one already replaced', () => {
    const text = '0123456789';
    expect(core.rewrite(text, [{ start: -1, end: 3 }])).toBe(text);
    expect(core.rewrite(text, [{ start: 4, end: 4 }])).toBe(text);
    expect(core.rewrite(text, [{ start: 8, end: 99 }])).toBe(text);
    expect(core.rewrite(text, [{ start: 'a', end: 3 }])).toBe(text);
    // Of two overlapping spans only the right-most is applied; the other is dropped, not guessed at.
    expect(
      core.rewrite(text, [
        { start: 2, end: 6, ref: 'X' },
        { start: 4, end: 8, ref: 'Y' },
      ]),
    ).toBe('0123Y89');
    expect(core.rewrite(text, undefined as unknown as unknown[])).toBe(text);
  });

  it('names each entry once and skips short matches', () => {
    expect(
      core.names({
        hits: [
          { entry: 'A', short: false },
          { entry: 'A', short: false },
          { entry: 'B', short: true },
          { entry: 'C' },
        ],
      }),
    ).toEqual(['A', 'C']);
    expect(core.names(null)).toEqual([]);
  });

  it('decides: paste when clean, warn when found, offline when the host did not answer', () => {
    expect(core.decide({ ok: true, hits: [] })).toEqual({ action: 'paste' });
    const w = core.decide({ ok: true, hits: [{ entry: 'GitHub' }], spans: [{ start: 0, end: 1 }] });
    expect(w).toEqual({ action: 'warn', names: ['GitHub'], spans: [{ start: 0, end: 1 }] });
    expect(core.decide({ ok: false, error: 'unavailable' })).toEqual({
      action: 'offline',
      reason: 'unavailable',
    });
    expect(core.decide(undefined).action).toBe('offline');
    expect(core.decide({ ok: false }).reason).toBe('no answer');
  });
});

interface Content {
  install: (
    doc: Document,
    send: (t: string) => Promise<unknown>,
    hooks?: {
      onWarn?: (w: { names: string[]; choices: Record<string, () => void> }) => void;
      onNotice?: (text: string) => void;
    },
  ) => () => void;
}

function paste(doc: Document, text: string, files = 0): Event {
  const ev = new Event('paste', { bubbles: true, cancelable: true });
  Object.defineProperty(ev, 'clipboardData', {
    value: { getData: () => text, files: { length: files } },
  });
  doc.body.dispatchEvent(ev);
  return ev;
}

describe('content script', () => {
  let install: Content['install'];
  let exec: ReturnType<typeof vi.fn>;
  beforeEach(async () => {
    document.body.innerHTML = '<textarea id="t"></textarea>';
    exec = vi.fn(() => true);
    (document as unknown as { execCommand: unknown }).execCommand = exec;
    await loadCore();
    // @ts-expect-error plain browser script without types
    await import('../browser-extension/content.js');
    install = (globalThis as unknown as { UnvGuardContent: Content }).UnvGuardContent.install;
  });
  const tick = () => new Promise((r) => setTimeout(r, 0));

  it('holds the paste, asks the host, and inserts the same text when it is clean', async () => {
    const send = vi.fn(async () => ({ ok: true, hits: [] }));
    const stop = install(document, send);
    const ev = paste(document, 'nothing secret in here');
    expect(ev.defaultPrevented).toBe(true);
    expect(exec).not.toHaveBeenCalled();
    await tick();
    expect(send).toHaveBeenCalledWith('nothing secret in here');
    expect(exec).toHaveBeenCalledWith('insertText', false, 'nothing secret in here');
    stop();
  });

  it('leaves short text and file pastes to the page', async () => {
    const send = vi.fn(async () => ({ ok: true, hits: [] }));
    const stop = install(document, send);
    expect(paste(document, 'short').defaultPrevented).toBe(false);
    expect(paste(document, 'a long enough piece of text', 1).defaultPrevented).toBe(false);
    await tick();
    expect(send).not.toHaveBeenCalled();
    stop();
  });

  it('offers the choices when a secret is found and inserts only what the person picks', async () => {
    const text = 'token=plain-value-123456 please debug';
    const answer = {
      ok: true,
      hits: [{ entry: 'GitHub', short: false }],
      spans: [{ start: 6, end: 24, ref: '${GitHub}' }],
    };
    let warning: { names: string[]; choices: Record<string, () => void> } | undefined;
    const stop = install(document, async () => answer, { onWarn: (w) => (warning = w) });
    paste(document, text);
    await tick();
    expect(warning?.names).toEqual(['GitHub']);
    // Nothing reached the page while the question was open, and cancel inserts nothing.
    expect(exec).not.toHaveBeenCalled();
    warning!.choices.cancel();
    expect(exec).not.toHaveBeenCalled();
    warning!.choices.replace();
    expect(exec).toHaveBeenLastCalledWith('insertText', false, 'token=${GitHub} please debug');
    warning!.choices.anyway();
    expect(exec).toHaveBeenLastCalledWith('insertText', false, text);
    stop();
  });

  it('lets the paste through with a notice when the host cannot be reached', async () => {
    let notice = '';
    const stop = install(
      document,
      async () => {
        throw new Error('no host');
      },
      { onNotice: (t) => (notice = t) },
    );
    paste(document, 'some text that is long enough');
    await tick();
    await tick();
    expect(exec).toHaveBeenCalledWith('insertText', false, 'some text that is long enough');
    // The notice lives in a closed shadow root, so it is read through the hook, not the page.
    expect(notice).toContain('could not check this paste');
    stop();
  });

  it('falls back to setRangeText when the editor refuses insertText', async () => {
    exec.mockReturnValue(false);
    const ta = document.getElementById('t') as HTMLTextAreaElement;
    ta.focus();
    const stop = install(document, async () => ({ ok: true, hits: [] }));
    paste(document, 'plain text for the box');
    await tick();
    expect(ta.value).toBe('plain text for the box');
    stop();
  });
});
