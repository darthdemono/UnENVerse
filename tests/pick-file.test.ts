// SPDX-License-Identifier: PolyForm-Strict-1.0.0
// Copyright Jubair Hasan.
/**
 * The file picker the config view uses. It builds a throwaway file input (a
 * static one is a ghost widget in WebKitGTK), hands the text and the name to the
 * caller, and removes the input however the person leaves it.
 */
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { pickFileText } from '../src/ts/chunk-ops';

const input = () => document.querySelector<HTMLInputElement>('input[type=file]');
const flush = (ms = 40) => new Promise((r) => setTimeout(r, ms));

beforeEach(() => {
  document.body.innerHTML = '';
});

describe('pickFileText', () => {
  it('reads the chosen file as text with its name, then removes the input', async () => {
    const got = vi.fn();
    pickFileText('.txt', got);
    const inp = input()!;
    expect(inp.accept).toBe('.txt');
    Object.defineProperty(inp, 'files', {
      value: [new File(['hello'], 'a.txt')],
      configurable: true,
    });
    inp.dispatchEvent(new Event('change'));
    await flush();
    expect(got).toHaveBeenCalledWith('hello', 'a.txt');
    expect(input()).toBeNull();
  });

  it('choosing nothing removes the input, and a later cancel or focus does no harm', async () => {
    const got = vi.fn();
    vi.useFakeTimers();
    try {
      pickFileText('', got);
      const inp = input()!;
      Object.defineProperty(inp, 'files', { value: [], configurable: true });
      inp.dispatchEvent(new Event('change'));
      expect(input()).toBeNull();
      inp.dispatchEvent(new Event('cancel'));
      await vi.advanceTimersByTimeAsync(350);
      window.dispatchEvent(new Event('focus'));
      expect(got).not.toHaveBeenCalled();
    } finally {
      vi.useRealTimers();
    }
  });

  it('a picker whose input was removed by something else cleans up without error', () => {
    pickFileText('', () => {});
    const inp = input()!;
    inp.remove();
    expect(() => inp.dispatchEvent(new Event('cancel'))).not.toThrow();
  });
});
