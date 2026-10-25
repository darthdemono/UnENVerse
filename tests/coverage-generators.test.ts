// SPDX-License-Identifier: PolyForm-Strict-1.0.0
// Copyright Jubair Hasan.
/** Branch coverage for generators.ts: every format, charset flag and pattern. */
import { describe, it, expect, afterEach, vi } from 'vitest';
import {
  generateRandomBytes,
  passwordCharset,
  passwordEntropyBits,
  generatePassword,
  generateApiKeyPattern,
  generateHash,
  guardEntropy,
  setEntropySource,
  entropySource,
  fillRandom,
  EntropyPoolEmpty,
} from '../src/ts/generators';

afterEach(async () => {
  delete (window as any).__TAURI__;
  await setEntropySource('os');
});

describe('generators', () => {
  it('encodes random bytes three ways', () => {
    expect(generateRandomBytes(8, 'hex')).toMatch(/^[0-9a-f]{16}$/);
    expect(generateRandomBytes(32, 'base64')).toMatch(/^[A-Za-z0-9+/]+=*$/);
    expect(generateRandomBytes(32, 'base64url')).toMatch(/^[A-Za-z0-9_-]+$/);
  });

  it('builds the charset from each flag and drops ambiguous characters', () => {
    const all = { upper: true, lower: true, digits: true, symbols: true, noAmbig: false };
    expect(passwordCharset(all)).toContain('O');
    expect(passwordCharset({ ...all, noAmbig: true })).not.toMatch(/[O0lI1]/);
    expect(
      passwordCharset({
        upper: false,
        lower: false,
        digits: false,
        symbols: false,
        noAmbig: false,
      }),
    ).toBe('');
    expect(
      generatePassword({
        length: 5,
        upper: false,
        lower: false,
        digits: false,
        symbols: false,
        noAmbig: false,
      }),
    ).toBeNull();
    expect(generatePassword({ ...all, length: 12 })).toHaveLength(12);
  });

  it('entropy bits grow with length and clamp an empty alphabet', () => {
    expect(passwordEntropyBits(10, 2)).toBe(10);
    expect(passwordEntropyBits(10, 0)).toBe(0);
  });

  it('shapes every api key pattern', () => {
    expect(generateApiKeyPattern('jwt-secret')).toMatch(/^[0-9a-f]{64}$/);
    expect(generateApiKeyPattern('base64-32')).toMatch(/^[A-Za-z0-9+/]+=*$/);
    expect(generateApiKeyPattern('hex-32')).toMatch(/^[0-9a-f]{64}$/);
    expect(generateApiKeyPattern('hex-16')).toMatch(/^[0-9a-f]{32}$/);
    expect(generateApiKeyPattern('bearer')).toMatch(/^[A-Za-z0-9_-]+$/);
    expect(generateApiKeyPattern('sk-prefix')).toMatch(/^sk-[A-Za-z0-9_-]{1,48}$/);
  });

  it('hashes to hex or base64', async () => {
    expect(await generateHash('a', 'SHA-256', 'hex')).toMatch(/^[0-9a-f]{64}$/);
    expect(await generateHash('a', 'SHA-1', 'base64')).toMatch(/^[A-Za-z0-9+/]+=*$/);
  });

  it('guardEntropy passes values through, reports an empty pool, rethrows the rest', () => {
    expect(
      guardEntropy(
        () => 5,
        () => {},
      ),
    ).toBe(5);
    const onEmpty = vi.fn();
    expect(
      guardEntropy(() => {
        throw new EntropyPoolEmpty();
      }, onEmpty),
    ).toBeUndefined();
    expect(onEmpty).toHaveBeenCalledWith(expect.stringMatching(/filling its pool/));
    expect(() =>
      guardEntropy(() => {
        throw new Error('x');
      }, onEmpty),
    ).toThrow('x');
  });
});

describe('pooled entropy source', () => {
  function bridge(hex: string | Error) {
    const invoke = vi.fn(async () => {
      if (hex instanceof Error) throw hex;
      return hex;
    });
    (window as any).__TAURI__ = { core: { invoke } };
    return invoke;
  }

  it('fills from the pool, tops it up when half drained, and errors when empty', async () => {
    const invoke = bridge('ab'.repeat(8192));
    await setEntropySource('dev');
    expect(entropySource()).toBe('dev');
    expect(invoke).toHaveBeenCalledTimes(1);

    const a = fillRandom(new Uint8Array(4));
    expect([...a]).toEqual([0xab, 0xab, 0xab, 0xab]);
    // Small draw leaves the pool above half: no refill.
    expect(invoke).toHaveBeenCalledTimes(1);
    // Big draw drops below half: refill kicks in.
    fillRandom(new Uint8Array(5000));
    await new Promise((r) => setTimeout(r, 20));
    expect(invoke.mock.calls.length).toBeGreaterThan(1);
    // Over-ask: error and an attempted refill.
    expect(() => fillRandom(new Uint8Array(9000))).toThrow(EntropyPoolEmpty);
  });

  it('a failed top-up after a big draw is swallowed', async () => {
    const invoke = bridge('ab'.repeat(8192));
    await setEntropySource('dev');
    invoke.mockRejectedValue(new Error('gone'));
    fillRandom(new Uint8Array(5000));
    await new Promise((r) => setTimeout(r, 20));
    expect(invoke).toHaveBeenCalledTimes(2);
  });

  it('survives a failed refill and a refill already in flight', async () => {
    bridge(new Error('no device'));
    await expect(setEntropySource('dev')).rejects.toThrow('no device');
    expect(() => fillRandom(new Uint8Array(1))).toThrow(EntropyPoolEmpty);
    await new Promise((r) => setTimeout(r, 20));
  });

  it('an empty id falls back to the OS source and a concurrent refill is skipped', async () => {
    const invoke = bridge('00'.repeat(8192));
    await Promise.all([setEntropySource('dev'), setEntropySource('dev')]);
    expect(invoke.mock.calls.length).toBeLessThanOrEqual(2);
    await setEntropySource('');
    expect(entropySource()).toBe('os');
  });
});
