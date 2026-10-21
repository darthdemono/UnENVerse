// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
import { describe, it, expect } from 'vitest';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

const read = (p: string) => readFileSync(resolve(__dirname, '..', p), 'utf8');

describe('AMOLED theme', () => {
  it('is offered as a swatch and defines a true-black page background', () => {
    expect(read('src/ts/settings-panel.ts')).toContain("id: 'amoled'");
    const css = read('src/css/base.css');
    const block = /\[data-theme="amoled"\]\s*\{([^}]*)\}/.exec(css)?.[1] ?? '';
    expect(block).toMatch(/--bg:\s*#000\b/);
    expect(block).toContain('color-scheme: dark');
  });
});
