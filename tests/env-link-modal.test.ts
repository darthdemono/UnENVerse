// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
/**
 * "Link to vault": turning plain KEY=value lines of an env-file chunk into
 * references to vault entries (matching ones) or into new entries (everything
 * else the person ticks).
 *
 * The risk is quiet: a wrong link points a deployed file at the wrong secret.
 * So the dialog must show what it will do, default to ticking only confident
 * matches, move a literal secret into the vault rather than copy it, and never
 * create a second entry under a name that already exists.
 */
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { st } from '../src/ts/state';
import { openEnvLinkModal, closeEnvLinkModal, applyEnvLink } from '../src/ts/render';
import { loadRealIndexHtml, makeEntry, makeProject, makeVault, resetState } from './helpers';

const toasts: [string, string?][] = [];
vi.mock('../src/ts/utils', async (importOriginal) => {
  const real = await importOriginal<typeof import('../src/ts/utils')>();
  return { ...real, showToast: (m: string, k?: string) => toasts.push([m, k]) };
});

const $ = (id: string) => document.getElementById(id)!;
const f = (key: string, value: string) => ({ key, value, field_type: 'var' as const });
const rows = () => [...document.querySelectorAll<HTMLElement>('.env-link-row')];

function setup(fields: ReturnType<typeof f>[], entries: object[] = [], projectId = 'web') {
  const chunk = {
    id: 'c1',
    name: 'app.env',
    chunk_type: 'env_file',
    fields,
  } as never as import('../src/ts/types').SecretChunk;
  const proj = makeProject({ id: projectId, name: 'Web', chunks: [chunk] } as never);
  st.vault = makeVault({
    api_keys: entries.map((e) => makeEntry(e as never)),
    projects: [makeProject({ id: 'Universal', name: 'Universal' }), proj],
  });
  st.store = { save: async () => {}, isRemote: false } as never;
  openEnvLinkModal(proj, chunk);
  return { chunk, proj };
}

beforeEach(() => {
  loadRealIndexHtml();
  resetState(st);
  toasts.length = 0;
});

describe('the dialog', () => {
  it('names the chunk and field count, and shows each field as matched, linked, new or skipped', () => {
    setup(
      [
        f('STRIPE_KEY', 'sk_live_longvalue'),
        f('DB_URL', '${Pg/key}'),
        f('NEW_THING_KEY', 'brand-new-value'),
        f('EMPTY', ''),
      ],
      [{ id: 's', provider: 'Stripe', api_key: 'sk_live_longvalue' }],
    );
    expect($('env-link-subtitle').textContent).toBe('app.env — 4 fields');
    expect($('env-link-overlay').classList.contains('open')).toBe(true);
    const kinds = rows().map((r) => [...r.classList].find((c) => c.startsWith('env-link-row--')));
    expect(kinds).toEqual([
      'env-link-row--match',
      'env-link-row--linked',
      'env-link-row--create',
      'env-link-row--skip',
    ]);
  });

  it('ticks confident matches by default and leaves suggested new entries unticked', () => {
    setup(
      [f('STRIPE_KEY', 'sk_live_longvalue'), f('NEW_THING_KEY', 'brand-new-value')],
      [{ id: 's', provider: 'Stripe', api_key: 'sk_live_longvalue' }],
    );
    const boxes = rows().map((r) => r.querySelector<HTMLInputElement>('.env-link-cb')!.checked);
    expect(boxes).toEqual([true, false]);
  });

  it('shows what a match points at, and escapes names', () => {
    setup(
      [f('X_KEY', 'v-long-enough-value')],
      [{ id: 's', provider: '<img src=x onerror=1>', api_key: 'v-long-enough-value' }],
    );
    expect(document.querySelector('.env-link-list img, #env-link-list img')).toBeNull();
  });

  it('closing hides it and forgets the chunk, so Apply then does nothing', () => {
    const { chunk } = setup(
      [f('STRIPE_KEY', 'sk_live_longvalue')],
      [{ id: 's', provider: 'Stripe', api_key: 'sk_live_longvalue' }],
    );
    closeEnvLinkModal();
    expect($('env-link-overlay').classList.contains('open')).toBe(false);
    applyEnvLink();
    expect(chunk.fields[0].value).toBe('sk_live_longvalue');
    expect(toasts).toEqual([]);
  });
});

describe('applying', () => {
  it('replaces a ticked match with a reference and assigns the entry to the project', () => {
    const { chunk } = setup(
      [f('STRIPE_KEY', 'sk_live_longvalue')],
      [{ id: 's', provider: 'Stripe', api_key: 'sk_live_longvalue' }],
    );
    applyEnvLink();
    expect(chunk.fields[0].value).toBe('${Stripe/key}');
    expect(st.vault.api_keys[0].projectIds).toContain('web');
    expect(toasts).toContainEqual(['1 linked, 0 created ✓', 'ok']);
    expect($('env-link-overlay').classList.contains('open')).toBe(false);
  });

  it('leaves an unticked match as it was', () => {
    const { chunk } = setup(
      [f('STRIPE_KEY', 'sk_live_longvalue')],
      [{ id: 's', provider: 'Stripe', api_key: 'sk_live_longvalue' }],
    );
    document.querySelector<HTMLInputElement>('.env-link-cb')!.checked = false;
    applyEnvLink();
    expect(chunk.fields[0].value).toBe('sk_live_longvalue');
    expect(toasts).toContainEqual(['Nothing selected', '']);
    expect($('env-link-overlay').classList.contains('open')).toBe(true);
  });

  it('moves a ticked new secret into the vault as an entry in the project and refers to it', () => {
    const { chunk } = setup([f('NEW_THING_KEY', 'brand-new-value')]);
    const row = rows()[0];
    row.querySelector<HTMLInputElement>('.env-link-cb')!.checked = true;
    row.querySelector<HTMLInputElement>('.env-link-name-input')!.value = ' NewThing ';
    applyEnvLink();
    const created = st.vault.api_keys.find((e) => e.provider === 'NewThing')!;
    expect(created).toMatchObject({
      api_key: 'brand-new-value',
      price_type: 'local',
      projectIds: ['Universal', 'web'],
    });
    expect(chunk.fields[0].value).toBe('${NewThing_KEY/key}');
    expect(created.key_id).toBe('KEY');
    expect(toasts).toContainEqual(['0 linked, 1 created ✓', 'ok']);
  });

  it('a password-type user-name field is stored as the username, and referred to as such', () => {
    const { chunk } = setup([f('DB_USER', 'admin')]);
    const row = rows()[0];
    row.querySelector<HTMLInputElement>('.env-link-cb')!.checked = true;
    row.querySelector<HTMLInputElement>('.env-link-name-input')!.value = 'Db';
    const type = row.querySelector<HTMLSelectElement>('.env-link-type-select')!;
    type.value = 'password';
    applyEnvLink();
    const created = st.vault.api_keys.find((e) => e.provider === 'Db')!;
    expect(created.api_key).toBe('');
    expect(created.username).toBe('admin');
    expect(chunk.fields[0].value).toBe('${Db_USER/username}');
  });

  it('a new password entry is referred to by its password', () => {
    const { chunk } = setup([f('DB_PASSWORD', 'hunter2-long')]);
    const row = rows()[0];
    row.querySelector<HTMLInputElement>('.env-link-cb')!.checked = true;
    row.querySelector<HTMLInputElement>('.env-link-name-input')!.value = 'Db';
    row.querySelector<HTMLSelectElement>('.env-link-type-select')!.value = 'password';
    applyEnvLink();
    expect(chunk.fields[0].value).toBe('${Db_PASSWORD/password}');
    expect(st.vault.api_keys.find((e) => e.provider === 'Db')!.api_key).toBe('hunter2-long');
  });

  it('never creates a second entry under a name that already exists: it adopts the existing one', () => {
    const { chunk } = setup(
      [f('NEW_THING_KEY', 'brand-new-value')],
      [
        {
          id: 'old',
          provider: 'NewThing',
          key_id: 'KEY',
          api_key: 'the-original',
          projectIds: ['Universal'],
        },
      ],
    );
    const row = rows().find((r) => r.classList.contains('env-link-row--create'))!;
    row.querySelector<HTMLInputElement>('.env-link-cb')!.checked = true;
    row.querySelector<HTMLInputElement>('.env-link-name-input')!.value = 'NewThing';
    applyEnvLink();
    expect(st.vault.api_keys.filter((e) => e.provider === 'NewThing')).toHaveLength(1);
    expect(st.vault.api_keys[0].api_key).toBe('the-original');
    expect(st.vault.api_keys[0].projectIds).toContain('web');
    expect(chunk.fields[0].value).toBe('${NewThing_KEY/key}');
    expect(toasts).toContainEqual(['1 linked, 0 created ✓', 'ok']);
  });

  it('in the catch-all project nothing is added to an entry’s projects', () => {
    setup(
      [f('STRIPE_KEY', 'sk_live_longvalue')],
      [{ id: 's', provider: 'Stripe', api_key: 'sk_live_longvalue' }],
      'Universal',
    );
    applyEnvLink();
    expect(st.vault.api_keys[0].projectIds).toEqual(['Universal']);
  });

  it('only fields the dialog listed and that the chunk still has are touched', () => {
    const { chunk } = setup(
      [f('STRIPE_KEY', 'sk_live_longvalue')],
      [{ id: 's', provider: 'Stripe', api_key: 'sk_live_longvalue' }],
    );
    chunk.fields = [];
    applyEnvLink();
    expect(toasts).toContainEqual(['Nothing selected', '']);
  });

  it('rows that are already linked are left alone while the others are applied', () => {
    const { chunk } = setup(
      [f('DB_URL', '${Pg/key}'), f('STRIPE_KEY', 'sk_live_longvalue')],
      [{ id: 's', provider: 'Stripe', api_key: 'sk_live_longvalue' }],
    );
    applyEnvLink();
    expect(chunk.fields.map((x) => x.value)).toEqual(['${Pg/key}', '${Stripe/key}']);
    expect(toasts).toContainEqual(['1 linked, 0 created ✓', 'ok']);
  });

  it('a match on an entry that carries a key id is found by provider and key id, and assigned once', () => {
    const { chunk } = setup(
      [f('STRIPE_LIVE_KEY', 'sk_live_longvalue')],
      [
        {
          id: 's',
          provider: 'Stripe',
          key_id: 'LIVE',
          api_key: 'sk_live_longvalue',
          projectIds: ['Universal', 'web'],
        },
      ],
    );
    const row = rows()[0];
    row.dataset.ref = 'Stripe_LIVE/key';
    applyEnvLink();
    expect(chunk.fields[0].value).toBe('${Stripe_LIVE/key}');
    expect(st.vault.api_keys[0].projectIds).toEqual(['Universal', 'web']);
  });

  it('a reference with no field part still finds its entry for the project assignment', () => {
    setup(
      [f('STRIPE_KEY', 'sk_live_longvalue')],
      [{ id: 's', provider: 'Stripe', api_key: 'sk_live_longvalue' }],
    );
    rows()[0].dataset.ref = 'Stripe';
    applyEnvLink();
    expect(st.vault.api_keys[0].projectIds).toContain('web');
  });

  it('a match whose entry is gone is still written as a reference, with nothing to assign', () => {
    const { chunk } = setup(
      [f('STRIPE_KEY', 'sk_live_longvalue')],
      [{ id: 's', provider: 'Stripe', api_key: 'sk_live_longvalue' }],
    );
    rows()[0].dataset.ref = 'Vanished/key';
    applyEnvLink();
    expect(chunk.fields[0].value).toBe('${Vanished/key}');
    expect(st.vault.api_keys[0].projectIds).not.toContain('web');
  });

  it('a new entry with no name typed takes the variable’s own name, and one without a chosen type is an API key', () => {
    const { chunk } = setup([f('PLAIN', 'some-value-here')]);
    const row = rows()[0];
    row.querySelector<HTMLInputElement>('.env-link-cb')!.checked = true;
    row.querySelector<HTMLInputElement>('.env-link-name-input')!.value = '';
    row.querySelector<HTMLSelectElement>('.env-link-type-select')!.value = '';
    applyEnvLink();
    const created = st.vault.api_keys.find((e) => e.provider === 'PLAIN')!;
    expect(created.secretType).toBe('api_key');
    expect(chunk.fields[0].value).toBe('${PLAIN/key}');
  });

  it('in the catch-all project a new entry joins only the default project', () => {
    setup([f('NEW_THING_KEY', 'brand-new-value')], [], 'Universal');
    const row = rows()[0];
    row.querySelector<HTMLInputElement>('.env-link-cb')!.checked = true;
    applyEnvLink();
    expect(st.vault.api_keys[0].projectIds).toEqual(['Universal']);
  });

  it('adopting an existing entry with no key id refers to it by name, and picks the field by its type', () => {
    const { chunk } = setup(
      [f('ZETA_USER', 'bob'), f('ZETA_SECRET', 'a-long-secret-1')],
      [
        {
          id: 'p',
          provider: 'Mailer',
          api_key: 'pw',
          secretType: 'password',
          projectIds: ['Universal', 'web'],
        },
        { id: 'k', provider: 'Keyed', api_key: 'kk' },
      ],
    );
    const [userRow, secretRow] = rows();
    userRow.querySelector<HTMLInputElement>('.env-link-cb')!.checked = true;
    userRow.querySelector<HTMLInputElement>('.env-link-name-input')!.value = 'Mailer';
    userRow.dataset.keyId = '';
    secretRow.querySelector<HTMLInputElement>('.env-link-cb')!.checked = true;
    secretRow.querySelector<HTMLInputElement>('.env-link-name-input')!.value = 'Keyed';
    secretRow.dataset.keyId = '';
    applyEnvLink();
    expect(chunk.fields.map((x) => x.value)).toEqual(['${Mailer/username}', '${Keyed/key}']);
  });

  it('adopting an existing entry that has a password and a key id refers to the password', () => {
    const { chunk } = setup(
      [f('ZETA_PASSWORD', 'a-long-secret-1')],
      [
        {
          id: 'p',
          provider: 'Mailer',
          key_id: 'PASSWORD',
          api_key: 'pw',
          secretType: 'password',
          projectIds: ['Universal', 'web'],
        },
      ],
    );
    const row = rows()[0];
    row.querySelector<HTMLInputElement>('.env-link-cb')!.checked = true;
    row.querySelector<HTMLInputElement>('.env-link-name-input')!.value = 'Mailer';
    applyEnvLink();
    expect(chunk.fields[0].value).toBe('${Mailer_PASSWORD/password}');
    expect(st.vault.api_keys[0].projectIds).toEqual(['Universal', 'web']);
  });

  it('a row with no variable name is skipped', () => {
    const { chunk } = setup(
      [f('STRIPE_KEY', 'sk_live_longvalue')],
      [{ id: 's', provider: 'Stripe', api_key: 'sk_live_longvalue' }],
    );
    delete rows()[0].dataset.key;
    applyEnvLink();
    expect(chunk.fields[0].value).toBe('sk_live_longvalue');
  });

  it('a ticked row that is neither a match nor a new entry changes nothing', () => {
    const { chunk } = setup([f('EMPTY', '')]);
    const row = rows()[0];
    row.innerHTML += '<input type="checkbox" class="env-link-cb" checked>';
    applyEnvLink();
    expect(chunk.fields[0].value).toBe('');
    expect(toasts).toContainEqual(['Nothing selected', '']);
  });

  it('a linked row names what it points at, or nothing when the reference is unknown', () => {
    setup([f('DB_URL', '${Pg/key}')]);
    expect(rows()[0].textContent).toContain('Pg/key');
  });
});
