/**
 * The Generate button on the add/edit form: what it produces depends on the
 * kind of secret being entered.
 */
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { quickGenerate, fillForm } from '../src/ts/modals';
import { st } from '../src/ts/state';
import { loadRealIndexHtml, resetState } from './helpers';

const toasts: [string, string?][] = [];
vi.mock('../src/ts/utils', async (importOriginal) => {
  const real = await importOriginal<typeof import('../src/ts/utils')>();
  return { ...real, showToast: (m: string, k?: string) => toasts.push([m, k]) };
});

const $ = <T extends HTMLElement = HTMLElement>(id: string) => document.getElementById(id) as T;
const setType = (t: string) => {
  $<HTMLSelectElement>('f-secret-type').value = t;
  $('f-secret-type').dispatchEvent(new Event('change'));
};

beforeEach(() => {
  loadRealIndexHtml();
  resetState(st);
  toasts.length = 0;
  fillForm({} as never);
  delete (window as any).__TAURI__;
});
afterEach(() => {
  delete (window as any).__TAURI__;
});

describe('Generate', () => {
  it('an API key or any other type gets 32 random bytes as hex, different each time', async () => {
    setType('api_key');
    await quickGenerate();
    const first = $<HTMLInputElement>('f-key').value;
    expect(first).toMatch(/^[0-9a-f]{64}$/);
    expect(document.activeElement).toBe($('f-key'));
    await quickGenerate();
    expect($<HTMLInputElement>('f-key').value).not.toBe(first);
    expect(toasts).toContainEqual(['Value generated', 'ok']);
  });

  it('a password gets 20 characters of mixed classes', async () => {
    setType('password');
    for (let i = 0; i < 5; i++) {
      await quickGenerate();
      expect($<HTMLInputElement>('f-key').value).toHaveLength(20);
    }
    const seen = new Set<string>();
    for (let i = 0; i < 40; i++) {
      await quickGenerate();
      for (const c of $<HTMLInputElement>('f-key').value) seen.add(c);
    }
    expect([...seen].some((c) => /[A-Z]/.test(c))).toBe(true);
    expect([...seen].some((c) => /[a-z]/.test(c))).toBe(true);
    expect([...seen].some((c) => /\d/.test(c))).toBe(true);
    expect([...seen].some((c) => /[^A-Za-z0-9]/.test(c))).toBe(true);
    expect(toasts).toContainEqual(['Password generated', 'ok']);
  });

  it('an SSH key needs the desktop app, then puts the private key in the field', async () => {
    setType('ssh_key');
    await quickGenerate();
    expect(toasts).toContainEqual(['Tauri not available', 'err']);
    expect($<HTMLInputElement>('f-key').value).toBe('');
    (window as any).__TAURI__ = {
      core: {
        invoke: async () => ({
          public_key: 'ssh-ed25519 AAA',
          private_key: '-----BEGIN OPENSSH PRIVATE KEY-----',
        }),
      },
    };
    await quickGenerate();
    expect($<HTMLInputElement>('f-key').value).toBe('-----BEGIN OPENSSH PRIVATE KEY-----');
    expect(toasts).toContainEqual(['SSH key pair generated (private key inserted)', 'ok']);
    (window as any).__TAURI__ = {
      core: { invoke: async () => Promise.reject(new Error('no rng')) },
    };
    await quickGenerate();
    expect(toasts.some(([m, k]) => m.includes('no rng') && k === 'err')).toBe(true);
  });

  it('a certificate fills both the certificate and its key', async () => {
    setType('certificate');
    await quickGenerate();
    expect(toasts).toContainEqual(['Tauri not available', 'err']);
    let args: unknown;
    (window as any).__TAURI__ = {
      core: {
        invoke: async (c: string, a: unknown) => {
          args = a;
          return c === 'generate_certificate' ? { cert_pem: 'CERT', key_pem: 'KEY' } : null;
        },
      },
    };
    await quickGenerate();
    expect($<HTMLTextAreaElement>('f-cert').value).toBe('CERT');
    expect($<HTMLTextAreaElement>('f-cert-key').value).toBe('KEY');
    expect(args).toEqual({ commonName: 'localhost', validityDays: 365 });
    (window as any).__TAURI__ = {
      core: { invoke: async () => Promise.reject(new Error('openssl missing')) },
    };
    await quickGenerate();
    expect(toasts.some(([m, k]) => m.includes('openssl missing') && k === 'err')).toBe(true);
  });
});
