// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
import { describe, it, expect } from 'vitest';
import { fieldSchema } from '../src/ts/field-schema';

describe('fieldSchema', () => {
  it('calls a browser session URL its site and drops the OAuth and rate fields', () => {
    expect(fieldSchema('cookie')).toMatchObject({
      urlLabel: 'Site URL',
      callback: false,
      scopes: false,
      rate: false,
    });
  });

  it('keeps the whole field set for an API key and an OAuth client', () => {
    for (const t of ['api_key', 'oauth_client']) {
      expect(fieldSchema(t)).toMatchObject({ callback: true, scopes: true, rate: true });
    }
    expect(fieldSchema('api_key').urlLabel).toBe('API URL');
  });

  it('labels the shared URL for what it is in each type', () => {
    expect(fieldSchema('password').urlLabel).toBe('Login URL');
    expect(fieldSchema('database').urlLabel).toBe('Server URL');
    expect(fieldSchema('registry_token')).toMatchObject({ urlLabel: 'Registry URL', scopes: true });
    expect(fieldSchema('local_service')).toMatchObject({ rate: true, scopes: false });
  });

  it('treats a missing or unknown type as an API key', () => {
    expect(fieldSchema(undefined).urlLabel).toBe('API URL');
    expect(fieldSchema('from-the-future').callback).toBe(true);
  });
});
