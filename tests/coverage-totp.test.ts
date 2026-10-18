// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
/** Branch coverage for totp.ts: URI edge cases, builders, live codes and the ticker. */
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';

const SEED = 'JBSWY3DPEHPK3PXP';
const e = (o: Record<string, unknown>) => ({ id: 'e1', provider: 'P', ...o }) as any;

async function fresh(invoke?: (cmd: string, args: any) => Promise<unknown>) {
  vi.resetModules();
  if (invoke) (window as any).__TAURI__ = { core: { invoke: vi.fn(invoke) } };
  else delete (window as any).__TAURI__;
  const totp = await import('../src/ts/totp');
  const { st } = await import('../src/ts/state');
  return { totp, st };
}

afterEach(() => {
  delete (window as any).__TAURI__;
  vi.useRealTimers();
});

describe('parsing', () => {
  it('rejects bad input with a reason', async () => {
    const { totp } = await fresh();
    expect(() => totp.parseOtpauth('http://x')).toThrow(/not an otpauth/);
    expect(() => totp.parseOtpauth('otpauth://nope/x?secret=' + SEED)).toThrow(/only otpauth/);
    expect(() => totp.parseOtpauth('otpauth://totp/x')).toThrow(/no secret/);
    expect(() => totp.parseOtpauth('otpauth://totp/x?secret=1')).toThrow(/not usable base32/);
    expect(() => totp.parseOtpauth('otpauth://totp/x?secret=A')).toThrow(/not usable base32/);
    expect(() => totp.parseTotpSeed('  ')).toThrow(/empty/);
    expect(() => totp.parseTotpSeed('1')).toThrow(/not usable/);
    expect(() => totp.parseTotpSeed('- -')).toThrow(/not usable/);
  });

  it('reads every parameter, ignoring out-of-range ones', async () => {
    const { totp } = await fresh();
    const s = totp.parseOtpauth(
      `otpauth://hotp/Iss%3AAcct?secret=${SEED}&issuer=Param&algorithm=SHA256&digits=8&period=45&counter=7&flag&bogus=1`,
    );
    expect(s).toMatchObject({
      kind: 'hotp',
      issuer: 'Param',
      account: 'Acct',
      algorithm: 'SHA256',
      digits: 8,
      counter: 7,
    });
    const bad = totp.parseOtpauth(
      `otpauth://totp/Iss:Acct?secret=${SEED}&issuer=%20&algorithm=NOPE&digits=99&period=0&counter=-1`,
    );
    expect(bad).toMatchObject({
      issuer: 'Iss',
      account: 'Acct',
      digits: 6,
      period: 30,
      counter: 0,
    });
    const noLabel = totp.parseOtpauth(`otpauth://totp?secret=${SEED}`);
    expect(noLabel).toMatchObject({ issuer: null, account: null });
    const steam = totp.parseOtpauth(`otpauth://totp/x?secret=${SEED}&encoder=STEAM`);
    expect(steam.kind).toBe('steam');
    const notSteam = totp.parseOtpauth(`otpauth://totp/x?secret=${SEED}&encoder=other`);
    expect(notSteam.kind).toBe('totp');
    // A malformed percent escape is left alone rather than throwing.
    expect(totp.parseOtpauth(`otpauth://totp/a%C3?secret=${SEED}`).account).toBe('a%C3');
    expect(totp.parseTotpSeed(`otpauth://totp/x?secret=${SEED}`).secret).toBe(SEED);
    expect(totp.parseTotpSeed('jbsw y3dp-').secret).toBe('JBSWY3DP');
  });
});

describe('building', () => {
  it('writes totp, hotp and steam URIs and escapes punctuation', async () => {
    const { totp } = await fresh();
    const base = {
      secret: SEED,
      algorithm: 'SHA1',
      digits: 6,
      period: 30,
      counter: 4,
      issuer: null,
      account: null,
    } as any;
    expect(totp.buildOtpauthUri({ ...base, kind: 'totp' }, "I(s)'s!*", 'acc')).toContain(
      'I%28s%29%27s%21%2A:acc',
    );
    expect(totp.buildOtpauthUri({ ...base, kind: 'hotp' })).toContain('hotp/');
    expect(totp.buildOtpauthUri({ ...base, kind: 'hotp' })).toContain('&counter=4');
    const steam = totp.buildOtpauthUri({ ...base, kind: 'steam', issuer: 'Steam', account: 'me' });
    expect(steam).toContain('&encoder=steam');
    expect(steam).toContain('totp/Steam:me');
    expect(totp.buildOtpauthUri({ ...base, kind: 'totp', issuer: '', account: 'a/b' })).toContain(
      'totp/a%2Fb?',
    );
  });
});

describe('entry helpers', () => {
  it('fills defaults from untrusted entry data', async () => {
    const { totp } = await fresh();
    expect(totp.totpStoredOf(e({})).secret).toBe('');
    const p = totp.totpParamsOf(
      e({
        totp_kind: 'hotp',
        totp_counter: 3,
        totp_digits: 8,
        totp_period: 60,
        totp_algorithm: 'SHA512',
      }),
    );
    expect(p).toMatchObject({
      kind: 'hotp',
      counter: 3,
      digits: 8,
      period: 60,
      algorithm: 'SHA512',
    });
    const junk = totp.totpParamsOf(
      e({
        totp_kind: 'x',
        totp_counter: -1,
        totp_digits: 'ten',
        totp_period: 0,
        totp_algorithm: 'no',
      }),
    );
    expect(junk).toMatchObject({ kind: 'totp', counter: 0, digits: 6, period: 30 });
    expect(totp.totpParamsOf(e({ totp_kind: 'hotp', totp_counter: -2 })).counter).toBe(0);
    expect(totp.hasTotp(e({ totp_secret: ' ' }))).toBe(false);
    expect(totp.totpRemainingSecs(30, 61)).toBe(29);
    expect(totp.totpRemainingSecs(0, 5)).toBe(1);
    expect(totp.groupCode('12345')).toBe('12345');
    expect(totp.groupCode('123456')).toBe('123 456');
    expect(totp.groupCode('1234567')).toBe('1234 567');
  });
});

describe('live codes', () => {
  const reply = (over: object = {}) => ({
    code: '123456',
    next_code: '654321',
    kind: 'totp',
    counter: 0,
    remaining_secs: 20,
    period: 30,
    digits: 6,
    algorithm: 'SHA1',
    ...over,
  });

  it('returns null without a seed, without the bridge, or while locked', async () => {
    const a = await fresh();
    expect(await a.totp.liveCodeFor(e({ totp_secret: SEED }))).toBeNull();
    expect(a.totp.lastTotpError(e({}))).toBeNull();
    const b = await fresh(async () => reply());
    b.st.vaultOpen = false;
    expect(await b.totp.liveCodeFor(e({ totp_secret: SEED }))).toBeNull();
    expect(await b.totp.liveCodeFor(e({}))).toBeNull();
  });

  it('fetches, caches, serves a hotp from cache, and de-duplicates in-flight calls', async () => {
    let n = 0;
    let release: (v: unknown) => void = () => {};
    const { totp, st } = await fresh(() => {
      n++;
      return new Promise((r) => (release = r));
    });
    st.vaultOpen = true;
    const entry = e({ totp_secret: SEED });
    const first = totp.liveCodeFor(entry);
    expect(await totp.liveCodeFor(entry)).toBeNull();
    release(reply());
    expect((await first)!.code).toBe('123456');
    const cached = await totp.liveCodeFor(entry);
    expect(cached).toMatchObject({ code: '123456', next_code: '654321' });
    expect(n).toBe(1);

    totp.resetTotpCache();
    const h = e({ totp_secret: SEED, totp_kind: 'hotp', totp_counter: 2 });
    const p = totp.liveCodeFor(h);
    release(reply({ kind: 'hotp', counter: 2, next_code: undefined }));
    await p;
    expect(await totp.liveCodeFor(h)).toMatchObject({ remaining_secs: 0, next_code: null });
  });

  it('treats an empty reply as nothing and records a failure reason', async () => {
    let mode: 'empty' | 'error' | 'string' = 'empty';
    const { totp, st } = await fresh(async () => {
      if (mode === 'error') throw new Error('refused');
      if (mode === 'string') throw 'plain' as unknown as Error;
      return undefined;
    });
    st.vaultOpen = true;
    const entry = e({ totp_secret: SEED });
    expect(await totp.liveCodeFor(entry)).toBeNull();
    expect(totp.lastTotpError(entry)).toBeNull();
    mode = 'error';
    expect(await totp.liveCodeFor(entry)).toBeNull();
    expect(totp.lastTotpError(entry)).toBe('refused');
    mode = 'string';
    await totp.liveCodeFor(entry);
    expect(totp.lastTotpError(entry)).toBe('plain');
  });
});

describe('ticker', () => {
  const slot = (id: string, extra = '') =>
    `<div data-totp-for="${id}" ${extra}><span class="totp-code"></span><span class="totp-next"></span><span class="totp-counter"></span><span class="totp-secs"></span><div class="totp-countdown-fill"></div></div>`;
  const bare = (id: string) => `<div data-totp-for="${id}"></div>`;
  const reply = (over: object = {}) => ({
    code: '123456',
    next_code: '654321',
    kind: 'totp',
    counter: 0,
    remaining_secs: 5,
    period: 30,
    digits: 6,
    algorithm: 'SHA1',
    ...over,
  });

  beforeEach(() => {
    document.body.innerHTML = '';
  });

  it('does nothing with no slots, hides slots without a seed', async () => {
    const { totp, st } = await fresh(async () => reply());
    await totp.tickTotp();
    st.vault = { api_keys: [e({ id: 'a', totp_secret: '' })] } as any;
    document.body.innerHTML = slot('a') + slot('missing');
    await totp.tickTotp();
    expect(document.querySelectorAll('[hidden]')).toHaveLength(2);
    st.vault = undefined as any;
    await totp.tickTotp();
  });

  it('shows placeholders in a plain browser', async () => {
    const { totp, st } = await fresh();
    st.vault = { api_keys: [e({ id: 'a', totp_secret: SEED })] } as any;
    document.body.innerHTML = slot('a') + bare('a');
    await totp.tickTotp();
    expect(document.querySelector<HTMLElement>('.totp-code')!.textContent).toBe('— — —');
    expect(document.querySelector<HTMLElement>('[data-totp-for]')!.title).toMatch(/desktop app/);
  });

  it('paints a totp code with ring and warning, and a bare slot without throwing', async () => {
    const { totp, st } = await fresh(async () => reply());
    st.vaultOpen = true;
    st.vault = {
      api_keys: [
        e({ id: 'a', totp_secret: SEED }),
        e({ id: 'b', totp_secret: SEED, totp_period: 30 }),
      ],
    } as any;
    document.body.innerHTML = slot('a', 'data-totp-next="1"') + bare('b');
    await totp.tickTotp();
    expect(document.querySelector('.totp-code')!.textContent).toBe('123 456');
    expect(document.querySelector('.totp-next')!.textContent).toBe('654 321');
    expect(document.querySelector('.totp-countdown-fill')!.classList.contains('expiring')).toBe(
      true,
    );
    expect(document.querySelector<HTMLElement>('.totp-secs')!.textContent).toBe('5');
  });

  it('clears the next-code element when no next code comes back', async () => {
    const { totp, st } = await fresh(async () => reply({ next_code: null, remaining_secs: 25 }));
    st.vaultOpen = true;
    st.vault = { api_keys: [e({ id: 'a', totp_secret: SEED })] } as any;
    document.body.innerHTML = slot('a');
    await totp.tickTotp();
    expect(document.querySelector('.totp-next')!.textContent).toBe('');
    expect(document.querySelector('.totp-countdown-fill')!.classList.contains('expiring')).toBe(
      false,
    );
  });

  it('paints a counter-based code, with and without its elements', async () => {
    const { totp, st } = await fresh(async () => reply({ kind: 'hotp', counter: 9 }));
    st.vaultOpen = true;
    st.vault = {
      api_keys: [
        e({ id: 'a', totp_secret: SEED, totp_kind: 'hotp' }),
        e({ id: 'b', totp_secret: SEED, totp_kind: 'hotp', totp_counter: 1 }),
      ],
    } as any;
    document.body.innerHTML = slot('a') + bare('b');
    await totp.tickTotp();
    expect(document.querySelector('.totp-counter')!.textContent).toBe('#9');
  });

  it('explains a refusal, a lock, and a pending fetch', async () => {
    const { totp, st } = await fresh(async () => {
      throw new Error('nope');
    });
    st.vaultOpen = true;
    st.vault = { api_keys: [e({ id: 'a', totp_secret: SEED })] } as any;
    document.body.innerHTML = slot('a') + bare('a');
    await totp.tickTotp();
    expect(document.querySelector<HTMLElement>('[data-totp-for]')!.title).toContain(
      'Code not available: nope',
    );
    // A lock arriving between the fetch and the check, with no recorded reason.
    totp.resetTotpCache();
    const { totp: t2, st: s2 } = await fresh(async () => {
      s2.vaultOpen = false;
      return undefined;
    });
    s2.vaultOpen = true;
    s2.vault = { api_keys: [e({ id: 'a', totp_secret: SEED })] } as any;
    document.body.innerHTML = slot('a') + bare('a');
    await t2.tickTotp();
    expect(document.querySelector<HTMLElement>('[data-totp-for]')!.title).toBe('Vault is locked.');
    // Still fetching: no reason, vault open, nothing to say.
    const { totp: t3, st: s3 } = await fresh(async () => undefined);
    s3.vaultOpen = true;
    s3.vault = { api_keys: [e({ id: 'a', totp_secret: SEED })] } as any;
    document.body.innerHTML = slot('a');
    await t3.tickTotp();
  });

  it('start is idempotent and stop is safe twice', async () => {
    vi.useFakeTimers();
    const { totp } = await fresh(async () => reply());
    totp.startTotpTicker();
    totp.startTotpTicker();
    expect(vi.getTimerCount()).toBe(1);
    totp.stopTotpTicker();
    totp.stopTotpTicker();
    expect(vi.getTimerCount()).toBe(0);
    totp.startTotpTicker();
    await vi.advanceTimersByTimeAsync(1100);
    totp.stopTotpTicker();
  });
});
