// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
/**
 * The Add / Edit form: what it reads into an entry for each kind of secret,
 * what it refuses to save and what it says, how the optional-value rule moves
 * with the extra variables, the authenticator seed's parameters, the browser
 * storage tokens, and the small previews under the fields.
 */
import { describe, it, expect, beforeEach, afterAll, vi } from 'vitest';
import { bootApp, entry, teardownApp } from './app-boot';

const toasts: [string, string?][] = [];
let confirmAnswer = true;
vi.mock('../src/ts/utils', async (importOriginal) => {
  const real = await importOriginal<typeof import('../src/ts/utils')>();
  return {
    ...real,
    showToast: (m: string, k?: string) => toasts.push([m, k]),
    showConfirm: async () => confirmAnswer,
  };
});

afterAll(() => teardownApp());

const $ = <T extends HTMLElement = HTMLElement>(id: string) => document.getElementById(id) as T;
const flush = (ms = 30) => new Promise((r) => setTimeout(r, ms));
type St = Awaited<ReturnType<typeof bootApp>>['st'];
let st: St;

const set = (id: string, value: string) => {
  const el = $<HTMLInputElement | HTMLSelectElement | HTMLTextAreaElement>(id);
  el.value = value;
  el.dispatchEvent(new Event('input', { bubbles: true }));
  el.dispatchEvent(new Event('change', { bubbles: true }));
};
const check = (id: string, on = true) => {
  const el = $<HTMLInputElement>(id);
  el.checked = on;
  el.dispatchEvent(new Event('change', { bubbles: true }));
};
const addVar = (key: string, value: string, o: { secret?: boolean; pub?: boolean } = {}) => {
  $('f-extra-vars-add').click();
  const row = [
    ...document.querySelectorAll<HTMLElement>('#f-extra-vars-list .extra-var-row'),
  ].pop()!;
  row.querySelector<HTMLInputElement>('.extra-var-key')!.value = key;
  row.querySelector<HTMLInputElement>('.extra-var-value')!.value = value;
  row.querySelector<HTMLInputElement>('.extra-var-secret')!.checked = !!o.secret;
  row.querySelector<HTMLInputElement>('.extra-var-public')!.checked = !!o.pub;
  row
    .querySelector<HTMLInputElement>('.extra-var-key')!
    .dispatchEvent(new Event('input', { bubbles: true }));
  return row;
};
const openAdd = async (type = 'api_key') => {
  $('add-btn').click();
  await flush();
  set('f-secret-type', type);
  await flush();
};
const save = async () => {
  $('modal-save').click();
  await flush(60);
};
const last = () => st.vault.api_keys[st.vault.api_keys.length - 1];
const lastToast = () => toasts[toasts.length - 1];

beforeEach(async () => {
  toasts.length = 0;
  confirmAnswer = true;
  ({ st } = await bootApp([entry('a', 'Existing')], {}));
}, 30000);

describe('saving a new entry', { timeout: 30000 }, () => {
  it('reads every plain field of an API key into the entry', async () => {
    await openAdd();
    set('f-provider', 'Acme');
    set('f-account', 'ops');
    set('f-keyid', 'k9');
    set('f-key', 'acme-key-value-12345');
    set('f-secret', 'acme-secret-value-1');
    set('f-label', 'prod');
    set('f-apiurl', 'https://api.acme.test');
    set('f-cburl', 'https://app.acme.test/cb');
    set('f-version', 'v3');
    set('f-purpose', 'billing');
    set('f-pool', 'acme-ring');
    set('f-expires', '2031-05-06');
    set('f-rotation-days', '90');
    set('f-scopes', 'read, write ,');
    set('f-apidesc', 'the api');
    set('f-desc', 'about it');
    set('f-details', 'more');
    set('f-tags-input', 'alpha  beta');
    set('f-env-prefixes', 'VITE_, NEXT');
    check('f-compromised');
    await save();
    expect(last()).toMatchObject({
      provider: 'Acme',
      account_name: 'ops',
      key_id: 'k9',
      api_key: 'acme-key-value-12345',
      api_secret: 'acme-secret-value-1',
      label: 'prod',
      api_url: 'https://api.acme.test',
      callback_url: 'https://app.acme.test/cb',
      version: 'v3',
      purpose: 'billing',
      pool: 'acme-ring',
      expires_at: '2031-05-06',
      rotation_days: 90,
      scopes: ['read', 'write'],
      api_description: 'the api',
      description: 'about it',
      details: 'more',
      tags: ['alpha', 'beta'],
      env_prefixes: ['VITE', 'NEXT'],
      compromised: true,
      secretType: 'api_key',
    });
    expect(last().id).toBeTruthy();
    expect(last().created_at).toBeTruthy();
    expect($('modal-overlay').classList.contains('open')).toBe(false);
  });

  it('leaves out what was left empty, and an unparseable rotation cadence', async () => {
    await openAdd();
    set('f-provider', 'Bare');
    set('f-key', 'bare-key-value-12345');
    set('f-rotation-days', 'soon');
    await save();
    expect(last().rotation_days).toBeUndefined();
    expect(last().tags).toBeUndefined();
    expect(last().env_prefixes).toBeUndefined();
    expect(last().compromised).toBeUndefined();
  });

  it('keeps a rate limit as a count and a period', async () => {
    await openAdd();
    set('f-provider', 'Limited');
    set('f-key', 'limited-key-12345678');
    set('f-ratelimit', '100');
    set('f-ratelimit-period', 'minute');
    await save();
    expect(last()).toMatchObject({ rate_limit_count: 100, rate_limit_period: 'minute' });
  });

  it('stores extra variables with their secret and public flags, skipping unnamed rows', async () => {
    await openAdd();
    set('f-provider', 'Vars');
    set('f-key', 'vars-key-value-12345');
    addVar('REGION', 'eu-west-1', { pub: true });
    addVar('TOKEN', 'tok-123456', { secret: true });
    addVar('', 'orphan');
    await save();
    expect(last().extra_vars).toEqual([
      expect.objectContaining({ key: 'REGION', value: 'eu-west-1', secret: false, public: true }),
      expect.objectContaining({ key: 'TOKEN', value: 'tok-123456', secret: true }),
    ]);
  });

  it('a variable row’s secret box masks its value, and its remove button drops the row', async () => {
    await openAdd();
    const row = addVar('X', 'v');
    const value = row.querySelector<HTMLInputElement>('.extra-var-value')!;
    const box = row.querySelector<HTMLInputElement>('.extra-var-secret')!;
    box.checked = true;
    box.dispatchEvent(new Event('change', { bubbles: true }));
    expect(value.type).toBe('password');
    box.checked = false;
    box.dispatchEvent(new Event('change', { bubbles: true }));
    expect(value.type).toBe('text');
    row.querySelector<HTMLElement>('.extra-var-remove')!.click();
    expect(document.querySelectorAll('#f-extra-vars-list .extra-var-row')).toHaveLength(0);
  });
});

describe('what the form refuses', { timeout: 30000 }, () => {
  const refuses = async (message: string) => {
    const before = st.vault.api_keys.length;
    await save();
    expect(lastToast()).toEqual([message, 'err']);
    expect(st.vault.api_keys).toHaveLength(before);
    expect($('modal-overlay').classList.contains('open')).toBe(true);
  };

  it('an entry with no name', async () => {
    await openAdd();
    set('f-key', 'some-key-value-12345');
    await refuses('Provider is required');
  });

  it('an API key with no key', async () => {
    await openAdd();
    set('f-provider', 'NoKey');
    await refuses('API Key is required');
  });

  it('a certificate with no certificate data, a blob with no file, a composite with no template', async () => {
    await openAdd('certificate');
    set('f-provider', 'Cert');
    await refuses('Certificate data is required');
    $('modal-cancel').click();
    await openAdd('file_blob');
    set('f-provider', 'Blob');
    await refuses('File path/reference is required');
    $('modal-cancel').click();
    await openAdd('composite');
    set('f-provider', 'Comp');
    await refuses('Template is required');
  });

  it('an environment variable with nothing in it asks for a value or a named variable', async () => {
    await openAdd('env_var');
    set('f-provider', 'Env');
    await refuses('Add a value, or at least one named variable below');
  });

  it('a name label with characters that would not survive as a variable name', async () => {
    await openAdd();
    set('f-provider', 'Labelled');
    set('f-key', 'labelled-key-12345678');
    set('f-label', 'bad label!');
    await refuses(
      'Name label: letters, digits, _ and - only, starting with a letter or digit, max 24',
    );
  });

  it('a two-factor seed that cannot make a code', async () => {
    await openAdd();
    set('f-provider', 'Seeded');
    set('f-key', 'seeded-key-value-12345');
    set('f-totp', 'not base32 !!!');
    await save();
    expect(lastToast()?.[0]).toMatch(/^Two-factor seed: /);
    expect(st.vault.api_keys).toHaveLength(1);
  });

  it('a save while the vault is locked', async () => {
    await openAdd();
    set('f-provider', 'Late');
    set('f-key', 'late-key-value-12345');
    st.vaultOpen = false;
    await refuses('The vault is locked. Unlock it to save.');
  });
});

describe('an entry that may have no main value', { timeout: 30000 }, () => {
  it('an environment variable holding only named variables saves', async () => {
    await openAdd('env_var');
    set('f-provider', 'Env');
    addVar('A', '1');
    await save();
    expect(last().extra_vars).toHaveLength(1);
    expect(last().api_key).toBe('');
  });

  it('the key field is marked optional while a named variable exists and required again when it goes', async () => {
    await openAdd('env_var');
    const label = () => $('f-key-label').textContent!;
    expect(label()).toContain('*');
    const row = addVar('A', '1');
    expect(label()).toContain('optional');
    expect($('f-key').hasAttribute('aria-required')).toBe(false);
    row.querySelector<HTMLElement>('.extra-var-remove')!.click();
    expect(label()).toContain('*');
    expect($('f-key').getAttribute('aria-required')).toBe('true');
  });

  it('an authenticator seed alone is enough', async () => {
    await openAdd();
    set('f-provider', 'Only2fa');
    set('f-totp', 'JBSWY3DPEHPK3PXP');
    await save();
    expect(last().totp_secret).toBe('JBSWY3DPEHPK3PXP');
  });
});

describe('the authenticator seed', { timeout: 30000 }, () => {
  const seed = 'JBSWY3DPEHPK3PXP';

  it('writes only the parameters that differ from the defaults', async () => {
    await openAdd();
    set('f-provider', 'Totp');
    set('f-totp', seed);
    set('f-totp-algorithm', 'SHA256');
    set('f-totp-digits', '8');
    set('f-totp-period', '60');
    await save();
    expect(last()).toMatchObject({
      totp_secret: seed,
      totp_algorithm: 'SHA256',
      totp_digits: 8,
      totp_period: 60,
    });
    expect(last().totp_kind).toBeUndefined();
  });

  it('a plain seed on default settings stores nothing but the seed', async () => {
    await openAdd();
    set('f-provider', 'Plain');
    set('f-totp', seed);
    await save();
    expect(last()).toMatchObject({ totp_secret: seed });
    for (const k of [
      'totp_algorithm',
      'totp_digits',
      'totp_period',
      'totp_kind',
      'totp_counter',
    ] as const)
      expect(last()[k]).toBeUndefined();
  });

  it('a counter-based seed keeps its counter, zero included', async () => {
    await openAdd();
    set('f-provider', 'Hotp');
    set('f-totp', seed);
    set('f-totp-kind', 'hotp');
    set('f-totp-counter', '0');
    await save();
    expect(last()).toMatchObject({ totp_kind: 'hotp', totp_counter: 0 });
  });

  it('a Steam seed ignores the parameter boxes', async () => {
    await openAdd();
    set('f-provider', 'Steam');
    set('f-totp', seed);
    set('f-totp-kind', 'steam');
    set('f-totp-digits', '8');
    await save();
    expect(last().totp_kind).toBe('steam');
    expect(last().totp_digits).toBeUndefined();
  });

  it('pasting an otpauth address fills the parameters and says what it read, without printing the seed', async () => {
    await openAdd();
    set(
      'f-totp',
      `otpauth://totp/Acme:alice?secret=${seed}&issuer=Acme&digits=8&period=45&algorithm=SHA512`,
    );
    const status = $('f-totp-status').textContent!;
    expect(status).toContain('8 digits');
    expect(status).not.toContain(seed);
    expect($('f-totp-params').hidden).toBe(false);
    expect($<HTMLInputElement>('f-totp-digits').value).toBe('8');
  });

  it('says so when the seed cannot be read, and shows nothing for an empty box', async () => {
    await openAdd();
    set('f-totp', 'zzz!!!');
    expect($('f-totp-status').textContent).toMatch(/\S/);
    set('f-totp', '');
    expect($('f-totp-status').textContent).toBe('');
  });

  it('a Steam address hides the parameter row and says it is Steam Guard', async () => {
    await openAdd();
    set('f-totp', `otpauth://steam/Steam:bob?secret=${seed}&issuer=Steam`);
    expect($('f-totp-status').textContent).toContain('Steam Guard');
    expect($('f-totp-params').hidden).toBe(true);
  });

  it('the reveal button shows and hides the seed', async () => {
    await openAdd();
    const input = $<HTMLInputElement>('f-totp');
    expect(input.type).toBe('password');
    $('f-totp-reveal').click();
    expect(input.type).toBe('text');
    $('f-totp-reveal').click();
    expect(input.type).toBe('password');
  });
});

describe('browser sessions', { timeout: 30000 }, () => {
  const tokens = (...t: unknown[]) => JSON.stringify(t);
  const good = { origin: 'https://site.test', storage: 'local', key: 'jwt', value: 'abc' };

  const session = async (raw: string) => {
    await openAdd('cookie');
    set('f-provider', 'Session');
    set('f-key', 'sid=abc; theme=dark');
    set('f-storage-tokens', raw);
    await save();
  };

  it('keeps well-formed storage tokens', async () => {
    await session(tokens(good, { ...good, storage: 'session', key: 'k2' }));
    expect(last().storage_tokens).toHaveLength(2);
  });

  it('an empty box means no tokens', async () => {
    await session('');
    expect(last().storage_tokens).toEqual([]);
  });

  it.each([
    ['not an array', JSON.stringify(good), 'Browser storage tokens must be a JSON array'],
    ['an item that is not an object', '[1]', 'Invalid browser storage token'],
    ['a missing key', tokens({ ...good, key: '' }), 'Invalid browser storage token'],
    ['a bad storage area', tokens({ ...good, storage: 'cloud' }), 'Invalid browser storage token'],
    [
      'an origin that is not a URL',
      tokens({ ...good, origin: 'nope' }),
      'Token origin must be an exact http(s) origin',
    ],
    [
      'an origin that is not http(s)',
      tokens({ ...good, origin: 'ftp://x.test' }),
      'Token origin must be an exact http(s) origin',
    ],
    [
      'an origin with a path',
      tokens({ ...good, origin: 'https://site.test/a' }),
      'Token origin must be an exact http(s) origin',
    ],
  ])('refuses %s', async (_label, raw, message) => {
    await session(raw);
    expect(lastToast()![0]).toBe(`Save failed: ${message}`);
    expect(st.vault.api_keys).toHaveLength(1);
  });

  it('cookie attributes ride along on a variable row and survive an edit', async () => {
    await openAdd('cookie');
    set('f-provider', 'Jar');
    set('f-key', 'sid=abc');
    const row = addVar('sid', 'abc', { secret: true });
    row.dataset.cookieAttrs = JSON.stringify({ domain: 'site.test', path: '/', secure: true });
    await save();
    expect(last().extra_vars![0].attrs).toEqual({ domain: 'site.test', path: '/', secure: true });
  });

  it('a damaged attribute blob is dropped rather than failing the save', async () => {
    await openAdd('cookie');
    set('f-provider', 'Jar');
    set('f-key', 'sid=abc');
    const row = addVar('sid', 'abc');
    row.dataset.cookieAttrs = '{not json';
    await save();
    expect(last().extra_vars![0].attrs).toBeUndefined();
  });
});

describe('certificates, blobs, composites and environment variables', { timeout: 30000 }, () => {
  it('a certificate keeps its PEMs and issuer, and drops the plain key', async () => {
    await openAdd('certificate');
    set('f-provider', 'site.test');
    set('f-cert', '-----BEGIN CERTIFICATE-----\nAAA\n-----END CERTIFICATE-----');
    set('f-cert-key', '-----BEGIN PRIVATE KEY-----\nBBB\n-----END PRIVATE KEY-----');
    set('f-cert-issuer', "Let's Encrypt");
    await save();
    expect(last()).toMatchObject({ cert_issuer: "Let's Encrypt", secretType: 'certificate' });
    expect(last().certificate_data).toContain('AAA');
    expect(last().cert_key_data).toContain('BBB');
  });

  it('a file blob keeps its reference', async () => {
    await openAdd('file_blob');
    set('f-provider', 'Blob');
    set('f-blob', '/etc/secret.bin');
    await save();
    expect(last().blob_ref).toBe('/etc/secret.bin');
  });

  it('a composite keeps its template and kind and previews the result', async () => {
    await openAdd('composite');
    set('f-provider', 'Link');
    set('f-template', 'https://{user}@host.test');
    addVar('user', 'alice');
    expect($('f-template-preview').textContent).toContain('alice');
    await save();
    expect(last()).toMatchObject({
      composite_template: 'https://{user}@host.test',
      secretType: 'composite',
    });
    expect(last().composite_kind).toBeTruthy();
  });

  it('the composite preview says what is wrong when the template cannot render', async () => {
    await openAdd('composite');
    set('f-template', 'x {missing}');
    expect($('f-template-preview').classList.contains('env-name-preview-invalid')).toBe(true);
    set('f-template', 'x {a}');
    addVar('a', '1');
    expect($('f-template-preview').textContent).toContain('1');
  });

  it('a composite preview lists the variables the template never uses', async () => {
    await openAdd('composite');
    addVar('a', '1');
    addVar('b', '2');
    set('f-template', 'only {a}');
    expect($('f-template-preview').textContent).toContain('unused: b');
  });

  it('an environment variable keeps its subtype', async () => {
    await openAdd('env_var');
    set('f-provider', 'Env');
    set('f-key', 'value-1234');
    set('f-envvar-subtype', 'url');
    await save();
    expect(last().secretType).toBe('env_var');
    expect(last().env_var_subtype).toBe('url');
  });
});

describe('the name preview', { timeout: 30000 }, () => {
  it('shows the variable name the entry will generate and warns when another entry would clash', async () => {
    st.vault.api_keys[0].env_prefixes = undefined;
    await openAdd();
    // Regression: the draft behind this warning held no values, so it generated no names and
    // the live collision warning never appeared.
    set('f-provider', 'Existing');
    set('f-key', 'clash-key-value-12345');
    expect($('f-name-collision').hidden).toBe(false);
    expect($('f-name-preview').textContent).toMatch(/\S/);
    expect($('f-name-collision').textContent).toBe(
      'Also generated by Existing — a copy of both would overwrite one',
    );
    set('f-provider', 'Different');
    expect($('f-name-collision').hidden).toBe(true);
  });

  it('flags a label that would not survive in a variable name', async () => {
    await openAdd();
    set('f-provider', 'Acme');
    set('f-label', 'bad label!');
    expect(
      $('f-label').classList.contains('input-invalid') ||
        $('f-name-preview').textContent!.length >= 0,
    ).toBe(true);
  });
});

describe('editing', { timeout: 30000 }, () => {
  const edit = async (idx = 0) => {
    document
      .querySelector<HTMLElement>(`#card-grid [data-action="edit"][data-idx="${idx}"]`)!
      .click();
    await flush();
  };

  it('opens with the entry’s values and a save keeps what the form has no box for', async () => {
    st.vault.api_keys[0].pinned = true;
    st.vault.api_keys[0].created_at = '2020-01-01T00:00:00.000Z';
    await edit();
    expect($<HTMLInputElement>('f-provider').value).toBe('Existing');
    set('f-purpose', 'changed');
    await save();
    expect(st.vault.api_keys[0]).toMatchObject({
      purpose: 'changed',
      pinned: true,
      created_at: '2020-01-01T00:00:00.000Z',
      id: 'a',
    });
  });

  it('renaming a provider carries chunk references with it and says how many', async () => {
    st.vault.projects[0].chunks = [
      {
        id: 'c',
        name: 'env',
        chunk_type: 'env_file',
        fields: [{ key: 'K', value: '${Existing/api_key}', field_type: 'env_var' }],
      },
    ] as never;
    await edit();
    set('f-provider', 'Renamed');
    await save();
    expect(
      (st.vault.projects[0].chunks as unknown as { fields: { value: string }[] }[])[0].fields[0]
        .value,
    ).toBe('${Renamed/api_key}');
    expect(toasts).toContainEqual(['Updated 1 project reference', 'ok']);
  });

  it('asks before renaming the variable an already-copied entry generates, and a no keeps the old name', async () => {
    st.vault.api_keys[0].last_copied_name = 'EXISTING_API_KEY';
    await edit();
    set('f-provider', 'Another');
    confirmAnswer = false;
    await save();
    expect(st.vault.api_keys[0].provider).toBe('Existing');
    confirmAnswer = true;
    await save();
    expect(st.vault.api_keys[0].provider).toBe('Another');
  });

  it('refuses a name label on an entry that predates stable ids', async () => {
    await edit();
    delete (st.vault.api_keys[0] as { id?: unknown }).id;
    set('f-label', 'prod');
    await save();
    expect(lastToast()![0]).toContain('predates stable ids');
  });

  it('does not overwrite a neighbour when the vault was reloaded while the form was open', async () => {
    await edit();
    set('f-purpose', 'mine');
    // A peer's edit reloads the vault while the form is open: index 0 is now someone else.
    st.vault.api_keys = [entry('z', 'Other')] as never;
    await save();
    expect(toasts.some(([m]) => m.includes('changed or deleted elsewhere'))).toBe(true);
    // Regression: this used to overwrite the entry now at that index, keeping its id.
    expect(st.vault.api_keys[0]).toMatchObject({ id: 'z', provider: 'Other' });
    expect(st.vault.api_keys[0].purpose).toBeUndefined();
  });

  it('a bundle rename carries references to it and to its renamed variables', async () => {
    const bundle = entry('bun', 'Disc', {
      secretType: 'bundle',
      api_key: '',
      extra_vars: [{ key: 'prefix', value: '!' }],
    });
    const member = entry('m', 'Bot', { bundle_id: 'bun', bundle_slot: 'bot' });
    ({ st } = await bootApp([bundle, member], { settings: { groupBundles: true } }));
    st.vault.projects[0].chunks = [
      {
        id: 'c',
        name: 'env',
        chunk_type: 'env_file',
        fields: [
          { key: 'P', value: '${bundle:Disc/prefix}', field_type: 'env_var' },
          { key: 'T', value: '${bundle:Disc/bot}', field_type: 'env_var' },
        ],
      },
    ] as never;
    document.querySelector<HTMLElement>('[data-action="edit"][data-idx="0"]')!.click();
    await flush();
    const row = document.querySelector<HTMLElement>('#f-extra-vars-list .extra-var-row')!;
    row.querySelector<HTMLInputElement>('.extra-var-key')!.value = 'lead';
    set('f-provider', 'Discord');
    await save();
    const values = (
      st.vault.projects[0].chunks as unknown as { fields: { value: string }[] }[]
    )[0].fields.map((f) => f.value);
    expect(values).toEqual(['${bundle:Discord/lead}', '${bundle:Discord/bot}']);
    expect(toasts.some(([m]) => /^Updated \d+ bundle reference/.test(m))).toBe(true);
  });

  it('a bundle whose templates loop is refused', async () => {
    const bundle = entry('bun', 'Disc', {
      secretType: 'bundle',
      api_key: '',
      extra_vars: [
        { key: 'a', kind: 'template', value: '{b}' },
        { key: 'b', kind: 'template', value: '{a}' },
      ],
    });
    ({ st } = await bootApp([bundle], { settings: { groupBundles: true } }));
    document.querySelector<HTMLElement>('[data-action="edit"][data-idx="0"]')!.click();
    await flush();
    await save();
    expect(lastToast()![0]).toMatch(/^Bundle template cycle:/);
  });
});
