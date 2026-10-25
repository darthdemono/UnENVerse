// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
/**
 * Which optional fields matter for which secret type, and what the shared ones are
 * called there.
 *
 * `api_url`, `callback_url`, `scopes` and `rate_limit` are one set of stored fields used
 * by every type, but they mean different things: for a browser session `api_url` is the
 * *site*, for a database it is the *server*, for a password the *login page*. The stored
 * name stays (renaming it would break every vault and the CLI); what changes is the label
 * the form shows and whether a field is offered at all.
 *
 * A field is only ever hidden while it is empty. An entry that already holds a value in
 * it shows the field whatever its type says (invariant 7: a hidden control is not a gate,
 * and a stored value must stay reachable).
 */
import type { SecretType } from './types';

export interface FieldSchema {
  /** Label of the `api_url` field for this type. */
  urlLabel: string;
  urlPlaceholder: string;
  /** Offer the callback URL (OAuth and webhook flows only). */
  callback: boolean;
  /** Offer scopes / permissions. */
  scopes: boolean;
  /** Offer a rate limit. */
  rate: boolean;
}

const DEFAULT: FieldSchema = {
  urlLabel: 'API URL',
  urlPlaceholder: 'https://api.example.com',
  callback: true,
  scopes: true,
  rate: true,
};

const none = { callback: false, scopes: false, rate: false };

const BY_TYPE: Partial<Record<SecretType, Partial<FieldSchema>>> = {
  api_key: {},
  password: { urlLabel: 'Login URL', urlPlaceholder: 'https://example.com/login', ...none },
  env_var: { urlLabel: 'Related URL', urlPlaceholder: 'https://example.com', ...none },
  connection_string: { urlLabel: 'Server URL', urlPlaceholder: 'db.internal:5432', ...none },
  ssh_key: { urlLabel: 'Host', urlPlaceholder: 'ssh://host.example.com', ...none },
  certificate: { urlLabel: 'Site URL', urlPlaceholder: 'https://example.com', ...none },
  file_blob: { urlLabel: 'Source URL', urlPlaceholder: 'https://example.com/file', ...none },
  cookie: { urlLabel: 'Site URL', urlPlaceholder: 'https://example.com', ...none },
  composite: { urlLabel: 'Base URL', urlPlaceholder: 'https://example.com', ...none },
  bundle: { urlLabel: 'Base URL', urlPlaceholder: 'https://example.com', ...none },
  oauth_client: { urlLabel: 'API base URL' },
  signing_key: {
    urlLabel: 'Verification URL',
    urlPlaceholder: 'https://example.com/jwks.json',
    ...none,
  },
  registry_token: {
    urlLabel: 'Registry URL',
    urlPlaceholder: 'https://registry.npmjs.org',
    callback: false,
    rate: false,
  },
  database: { urlLabel: 'Server URL', urlPlaceholder: 'db.internal:5432', ...none },
  recovery_codes: { urlLabel: 'Site URL', urlPlaceholder: 'https://example.com', ...none },
  gpg_key: { urlLabel: 'Keyserver URL', urlPlaceholder: 'https://keys.openpgp.org', ...none },
  age_key: { urlLabel: 'Related URL', urlPlaceholder: 'https://example.com', ...none },
  local_service: {
    urlLabel: 'Service URL',
    urlPlaceholder: 'http://localhost:8989',
    callback: false,
    scopes: false,
  },
  tracker: { urlLabel: 'Tracker URL', urlPlaceholder: 'https://tracker.example', ...none },
  usenet_server: { urlLabel: 'Server URL', urlPlaceholder: 'news.example.com', ...none },
  wifi: { urlLabel: 'Router admin URL', urlPlaceholder: 'http://192.168.1.1', ...none },
  license_key: { urlLabel: 'Vendor URL', urlPlaceholder: 'https://vendor.example', ...none },
  crypto_wallet: { urlLabel: 'Explorer URL', urlPlaceholder: 'https://explorer.example', ...none },
  passkey: { urlLabel: 'Site URL', urlPlaceholder: 'https://example.com', ...none },
  secure_note: { urlLabel: 'Related URL', urlPlaceholder: 'https://example.com', ...none },
  identity_document: { urlLabel: 'Issuer URL', urlPlaceholder: 'https://issuer.example', ...none },
};

/** The field schema of a type; an unknown type gets the full default set. */
export function fieldSchema(type: string | undefined): FieldSchema {
  return { ...DEFAULT, ...(BY_TYPE[(type || 'api_key') as SecretType] ?? {}) };
}
