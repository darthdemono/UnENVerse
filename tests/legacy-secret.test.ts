// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
import { describe, it, expect } from 'vitest';
import { migrateLegacySecret } from '../src/ts/state';
import { createdFromForm } from '../src/ts/modals';
import { getEntryFieldValue } from '../src/ts/chunk-ops';
import type { VaultEntry } from '../src/ts/types';

const e = (o: Partial<VaultEntry>) => ({ provider: 'Twilio', api_key: 'k', ...o }) as VaultEntry;

describe('the old API Secret becomes an additional value', () => {
  it('keeps its role as the value key, SECRET when there is none', () => {
    const a = e({ api_secret: 's1', secret_role: 'auth_token' });
    expect(migrateLegacySecret(a)).toBe(true);
    expect(a.extra_vars).toEqual([{ key: 'auth_token', value: 's1', secret: true }]);
    expect(a.api_secret).toBeUndefined();
    const b = e({ api_secret: 's2', secret_public: true });
    migrateLegacySecret(b);
    expect(b.extra_vars).toEqual([{ key: 'SECRET', value: 's2', secret: false, public: true }]);
  });

  it('never overwrites an additional value that already has the name', () => {
    const a = e({ api_secret: 's', extra_vars: [{ key: 'secret', value: 'other' }] });
    expect(migrateLegacySecret(a)).toBe(false);
    expect(a.api_secret).toBe('s');
  });

  it('does nothing without a secret, and is idempotent', () => {
    const a = e({ api_secret: 's' });
    expect(migrateLegacySecret(e({}))).toBe(false);
    migrateLegacySecret(a);
    expect(migrateLegacySecret(a)).toBe(false);
  });

  it('keeps ${Provider/SECRET} and its aliases resolving after the move', () => {
    const a = e({ api_secret: 's3' });
    migrateLegacySecret(a);
    for (const f of ['SECRET', 'API_SECRET', 'CLIENT_SECRET', 'api_secret'])
      expect(getEntryFieldValue(a, f)).toBe('s3');
  });
});

describe('the Created date', () => {
  it('keeps the stored time when the date is untouched, replaces it when changed', () => {
    expect(createdFromForm('2026-10-01', '2026-10-01T13:14:15.000Z')).toBe(
      '2026-10-01T13:14:15.000Z',
    );
    expect(createdFromForm('2025-01-02', '2026-10-01T13:14:15.000Z')).toBe(
      '2025-01-02T00:00:00.000Z',
    );
    expect(createdFromForm('', '2026-10-01T13:14:15.000Z')).toBeUndefined();
    expect(createdFromForm('junk', '2026-10-01T13:14:15.000Z')).toBe('2026-10-01T13:14:15.000Z');
  });
});
