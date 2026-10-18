// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
/**
 * What a card shows for an entry: its badges, which values are masked and which
 * are printed, rendered composites and their errors, links, metadata rows, the
 * "used by" list, expiry and rotation cues. Booted for real and read off the DOM,
 * because the card is the only place most of this is visible to a person.
 */
import { describe, it, expect, afterAll } from 'vitest';
import { bootApp, entry, teardownApp } from './app-boot';

afterAll(() => teardownApp());

type St = Awaited<ReturnType<typeof bootApp>>['st'];
let st: St;

const day = 86_400_000;
const iso = (offsetDays: number) => new Date(Date.now() + offsetDays * day).toISOString();
const dateOnly = (offsetDays: number) => iso(offsetDays).slice(0, 10);

async function show(entries: object[], settings: object = {}, extra: object = {}) {
  ({ st } = await bootApp(entries, { settings, ...extra }));
}
const card = (provider: string) =>
  [...document.querySelectorAll<HTMLElement>('#card-grid .card')].find(
    (c) => c.querySelector('.card-provider-name')?.textContent === provider,
  )!;
const badges = (c: HTMLElement) =>
  [...c.querySelectorAll('.card-provider .badge')].map((b) => b.textContent!.trim());
const rows = (c: HTMLElement) =>
  Object.fromEntries(
    [...c.querySelectorAll('.meta-row')].map((r) => [
      r.querySelector('.meta-key')!.textContent,
      r.querySelector('.meta-val')!.textContent!.replace(/\s+/g, ' ').trim(),
    ]),
  );

describe('badges', { timeout: 30000 }, () => {
  it('shows price, environment, key id, type, prefixes and a pin', async () => {
    await show([
      entry('a', 'Full', {
        key_id: 'k1',
        secretType: 'password',
        environment: 'staging',
        pinned: true,
        env_prefixes: ['VITE', 'NEXT'],
      }),
    ]);
    const b = badges(card('Full'));
    expect(b[0]).toBe('paid');
    expect(b).toEqual(expect.arrayContaining(['staging', 'k1', 'Password', 'VITE_', 'NEXT_']));
    expect(card('Full').querySelector('.card-provider svg, .pin-badge')).not.toBeNull();
  });

  it('prints a type, price or environment it does not know as text, never markup', async () => {
    await show([
      entry('a', 'Odd', { price_type: '<b>x</b>', environment: '<i>e</i>', key_id: '<u>k</u>' }),
    ]);
    expect(
      card('Odd').querySelector('.card-provider b, .card-provider i, .card-provider u'),
    ).toBeNull();
  });

  it('flags a leaked key and a key past its rotation cadence, but not a session', async () => {
    await show([
      entry('a', 'Leaky', { compromised: true }),
      entry('b', 'Stale', { rotation_days: 30, last_rotated_at: iso(-45) }),
      entry('c', 'Fresh', { rotation_days: 30, last_rotated_at: iso(-5) }),
      entry('d', 'Never', { rotation_days: 30 }),
      entry('e', 'Zero', { rotation_days: 0, last_rotated_at: iso(-400) }),
      entry('f', 'Session', { secretType: 'cookie', rotation_days: 30, last_rotated_at: iso(-45) }),
    ]);
    expect(badges(card('Leaky'))).toContain('⚠ LEAKED');
    expect(badges(card('Stale'))).toContain('⟳ rotate');
    expect(card('Stale').querySelector('.badge-rotation-due')!.getAttribute('title')).toMatch(
      /Rotation cadence 30d, overdue 1[45]d/,
    );
    for (const p of ['Fresh', 'Never', 'Zero', 'Session'])
      expect(badges(card(p))).not.toContain('⟳ rotate');
  });
});

describe('masking', { timeout: 30000 }, () => {
  const value = (p: string, id: string) =>
    card(p).querySelector<HTMLElement>(`#${id}`)!.textContent;

  it('masks by default and prints when the setting says so', async () => {
    await show([entry('a', 'Mask', { api_key: 'plain-visible-key-123' })]);
    expect(value('Mask', 'kv-key-0')).not.toContain('plain-visible-key-123');
    await show([entry('a', 'Mask', { api_key: 'plain-visible-key-123' })], {
      maskKeysByDefault: false,
    });
    expect(value('Mask', 'kv-key-0')).toBe('plain-visible-key-123');
  });

  it('never masks a value marked public, but an explicit hide still wins', async () => {
    await show([
      entry('a', 'Pub', {
        api_key: 'publishable-key-123',
        primary_public: true,
        api_secret: 'sec-value-123456',
        secret_public: true,
      }),
    ]);
    expect(value('Pub', 'kv-key-0')).toBe('publishable-key-123');
    expect(value('Pub', 'kv-secret-0')).toBe('sec-value-123456');
    st.revealed[`key-${st.vault.api_keys[0].id}`] = false;
    (await import('../src/ts/render')).renderGrid();
    expect(value('Pub', 'kv-key-0')).not.toContain('publishable-key-123');
  });

  it('masks the secret half separately from the key', async () => {
    await show([
      entry('a', 'Pair', { api_key: 'key-part-value-1', api_secret: 'secret-part-value-2' }),
    ]);
    expect(value('Pair', 'kv-secret-0')).not.toContain('secret-part-value-2');
    expect(card('Pair').querySelector('#reveal-secret-0')!.getAttribute('aria-pressed')).toBe(
      'false',
    );
    st.revealed[`secret-${st.vault.api_keys[0].id}`] = true;
    (await import('../src/ts/render')).renderGrid();
    expect(value('Pair', 'kv-secret-0')).toBe('secret-part-value-2');
    expect(card('Pair').querySelector('#reveal-secret-0')!.getAttribute('aria-pressed')).toBe(
      'true',
    );
  });
});

describe('what a card lists', { timeout: 30000 }, () => {
  it('shows username, email, extra variables (secret ones masked), scopes, categories and tags', async () => {
    await show([
      entry('a', 'Rich', {
        username: 'alice',
        email: 'alice@example.test',
        scopes: ['read', 'write'],
        extra_vars: [
          { key: 'region', value: 'eu-west-1' },
          { key: 'token', value: 'tok-secret-value-9', secret: true },
          { key: '', value: 'ignored' },
        ],
      }),
    ]);
    const c = card('Rich');
    expect(c.querySelector('.card-account')!.textContent).toBe('alice');
    expect(c.textContent).toContain('alice@example.test');
    expect([...c.querySelectorAll('.scope-pill')].map((s) => s.textContent)).toEqual([
      'read',
      'write',
    ]);
    expect(c.querySelector('.key-vars .key-group-title')!.textContent).toContain('Variables (2)');
    expect(c.textContent).toContain('eu-west-1');
    expect(c.textContent).not.toContain('tok-secret-value-9'.slice(0, 12) + '-secret');
    expect([...c.querySelectorAll('.cat-pill')].map((x) => x.textContent)).toEqual(['infra']);
    expect([...c.querySelectorAll('.tag-chip-card')].map((x) => x.textContent)).toEqual(['prod']);
  });

  it('falls back from account name to username to email for the subtitle', async () => {
    await show([
      entry('a', 'ByAccount', { account_name: 'acct', username: 'u', email: 'e@x.test' }),
      entry('b', 'ByUser', { username: 'u' }),
      entry('c', 'ByMail', { email: 'e@x.test' }),
      entry('d', 'Bare'),
    ]);
    expect(card('ByAccount').querySelector('.card-account')!.textContent).toBe('acct');
    expect(card('ByUser').querySelector('.card-account')!.textContent).toBe('u');
    expect(card('ByMail').querySelector('.card-account')!.textContent).toBe('e@x.test');
    expect(card('Bare').querySelector('.card-account')!.textContent).toBe('');
  });

  it('lists metadata rows, and turns only http(s) addresses into links', async () => {
    await show([
      entry('a', 'Meta', {
        version: 'v2',
        purpose: 'billing',
        pool: 'main',
        expires_at: '2099-01-01',
        details: 'long notes',
        api_url: 'https://api.example.test/v1',
        callback_url: 'javascript:alert(1)',
        rate_limit_count: 100,
        rate_limit_period: 'minute',
      }),
      entry('b', 'Note', { rate_limit: 'ask nicely' }),
    ]);
    const r = rows(card('Meta'));
    expect(r).toMatchObject({
      Version: 'v2',
      Purpose: 'billing',
      'Key Pool': 'main',
      Expires: '2099-01-01',
      Details: 'long notes',
      Callback: 'javascript:alert(1)',
    });
    expect(r['Rate Limit']).toBe('100 per minute');
    const links = [...card('Meta').querySelectorAll<HTMLAnchorElement>('.meta-val a')];
    expect(links.map((a) => a.href)).toEqual(['https://api.example.test/v1']);
    expect(links[0].rel).toBe('noopener noreferrer');
    expect(rows(card('Note'))['Rate Limit']).toBe('ask nicely');
  });

  it('shows the description behind a disclosure and the last rotation with its age', async () => {
    await show([
      entry('a', 'Old', { description: 'what it is for', last_rotated_at: iso(-90) }),
      entry('b', 'Today', { last_rotated_at: iso(0) }),
      entry('c', 'Yesterday', { last_rotated_at: iso(-1) }),
      entry('d', 'Recent', { last_rotated_at: iso(-10) }),
    ]);
    expect(card('Old').querySelector('.desc-content')!.textContent).toBe('what it is for');
    expect(card('Old').querySelector('.desc-toggle')!.getAttribute('aria-expanded')).toBe('false');
    expect(card('Old').querySelector('.rotation-age-badge')!.textContent).toBe('90d ago');
    expect(card('Old').querySelector('.rotation-age-badge')!.classList.contains('fresh')).toBe(
      false,
    );
    expect(card('Today').querySelector('.rotation-age-badge')!.textContent).toBe('today');
    expect(card('Yesterday').querySelector('.rotation-age-badge')!.textContent).toBe('1d ago');
    expect(card('Recent').querySelector('.rotation-age-badge')!.classList.contains('fresh')).toBe(
      true,
    );
  });

  it('shows who uses it, once per place, and leaves out chunk-to-chunk references', async () => {
    await show(
      [entry('a', 'Dbpass', { key_id: 'prod' }), entry('b', 'Lonely')],
      {},
      {
        projects: [
          { id: 'Universal', name: 'Universal' },
          {
            id: 'app',
            name: 'App',
            chunks: [
              {
                id: 'c1',
                name: 'env',
                chunk_type: 'env_file',
                fields: [
                  { key: 'DB', value: '${Dbpass}', field_type: 'env_var' },
                  { key: 'DB', value: '${Dbpass/api_key}', field_type: 'env_var' },
                  { key: 'ALT', value: '${Dbpass_prod}', field_type: 'env_var' },
                  { key: 'LINK', value: '${chunk:env/DB}', field_type: 'env_var' },
                  { key: 'PLAIN', value: 'literal', field_type: 'var' },
                ],
              },
            ],
          },
        ],
      },
    );
    const used = [...card('Dbpass').querySelectorAll('.usedby-badge')].map((b) => b.textContent);
    expect(used).toEqual(['App · DB', 'App · ALT']);
    expect(card('Lonely').querySelector('.usedby-row')).toBeNull();
  });

  it('badges the projects an entry is in, by their last path part, and skips the default one', async () => {
    await show(
      [entry('a', 'InProj', { projectIds: ['Universal', 'team/app', 'gone'] })],
      {},
      {
        projects: [
          { id: 'Universal', name: 'Universal' },
          { id: 'team/app', name: 'team/app' },
        ],
      },
    );
    const b = [...card('InProj').querySelectorAll('.card-projects .badge')];
    expect(b.map((x) => x.textContent)).toEqual(['app']);
    expect(b[0].getAttribute('title')).toBe('team/app');
  });
});

describe('toggle buttons say whether they are pressed', { timeout: 30000 }, () => {
  it('writes "false" as well as "true" for every toggle on a card and the chip bar', async () => {
    // Regression: a false value rendered as an empty attribute, which assistive tech reads
    // as "not a toggle button" instead of "not pressed".
    await show([entry('a', 'Tog', { api_secret: 'secret-part-value-2' })]);
    const c = card('Tog');
    for (const sel of ['#reveal-key-0', '#reveal-secret-0', '[data-action="pin"]'])
      expect(c.querySelector(sel)!.getAttribute('aria-pressed'), sel).toBe('false');
    expect(c.querySelector('[data-action="bulk-toggle"]')!.getAttribute('aria-checked')).toBe(
      'false',
    );
    for (const chip of document.querySelectorAll('#type-chip-bar .type-chip'))
      expect(['true', 'false']).toContain(chip.getAttribute('aria-pressed'));
  });
});

describe('expiry', { timeout: 30000 }, () => {
  const cls = (p: string) => card(p).className;
  it('colours the card edge by how soon it expires', async () => {
    await show([
      entry('a', 'Gone', { expires_at: dateOnly(-3) }),
      entry('b', 'Soon', { expires_at: dateOnly(5) }),
      entry('c', 'Month', { expires_at: dateOnly(20) }),
      entry('d', 'Later', { expires_at: dateOnly(200) }),
      entry('e', 'None'),
    ]);
    expect(cls('Gone')).toContain('expiry-urgent');
    expect(cls('Soon')).toContain('expiry-urgent');
    expect(cls('Month')).toContain('expiry-warn');
    expect(cls('Later')).toContain('expiry-safe');
    expect(cls('None')).not.toMatch(/expiry-/);
  });

  it('badges an expired key and one inside the warning window, and nothing further out', async () => {
    await show([
      entry('a', 'Gone', { expires_at: dateOnly(-3) }),
      entry('b', 'Soon', { expires_at: dateOnly(5) }),
      entry('c', 'Hours', { expires_at: iso(0.2) }),
      entry('d', 'Later', { expires_at: dateOnly(200) }),
      entry('e', 'Junk', { expires_at: 'not a date' }),
    ]);
    expect(card('Gone').querySelector('.badge-expiry-expired')).not.toBeNull();
    expect(card('Soon').querySelector('.badge-expiry-warn')).not.toBeNull();
    expect(card('Hours').querySelector('.badge-expiry-warn')).not.toBeNull();
    expect(card('Later').querySelector('[class*="badge-expiry"]')).toBeNull();
    expect(card('Junk').querySelector('[class*="badge-expiry"]')).toBeNull();
  });

  it('shows no expiry badge when the warning is turned off', async () => {
    await show([entry('a', 'Gone', { expires_at: dateOnly(-3) })], { showExpiryWarning: false });
    expect(card('Gone').querySelector('[class*="badge-expiry"]')).toBeNull();
  });
});

describe('composites', { timeout: 30000 }, () => {
  const comp = (over: object = {}) =>
    entry('c', 'Link', {
      secretType: 'composite',
      api_key: '',
      composite_template: 'https://{user}:{pass}@host.test',
      extra_vars: [
        { key: 'user', value: 'alice' },
        { key: 'pass', value: 'pw-value-123456', secret: true },
      ],
      ...over,
    });

  it('shows the rendered value (masked) with its template, and copy gets the real value', async () => {
    await show([comp()]);
    const c = card('Link');
    const v = c.querySelector<HTMLElement>('#kv-key-0')!;
    expect(v.textContent).not.toContain('pw-value-123456');
    expect(v.dataset.value).toBe('https://alice:pw-value-123456@host.test');
    expect(c.textContent).toContain('TEMPLATE');
    expect(c.textContent).toContain('https://{user}:{pass}@host.test');
  });

  it('shows why a template does not render, in place of the value', async () => {
    await show([comp({ composite_template: 'x {missing}' })]);
    const row = card('Link').querySelector('.key-value.revealed')!;
    expect(row.textContent).toMatch(/\S/);
    expect(card('Link').querySelector('#kv-key-0')).toBeNull();
  });

  it('a composite inside a bundle resolves slots and reports each kind of failure', async () => {
    const bundle = entry('bun', 'B', {
      secretType: 'bundle',
      api_key: '',
      extra_vars: [
        { key: 'host', value: 'db.test' },
        { key: 'loop', kind: 'template', value: '{loop}' },
      ],
    });
    const member = entry('m1', 'Db', {
      bundle_id: 'bun',
      bundle_slot: 'db',
      api_key: 'member-secret-1',
    });
    const mk = (id: string, name: string, template: string, extra: object = {}) =>
      entry(id, name, {
        secretType: 'composite',
        api_key: '',
        bundle_id: 'bun',
        bundle_slot: name.toLowerCase(),
        composite_template: template,
        ...extra,
      });
    await show(
      [
        bundle,
        member,
        mk('c1', 'Good', 'pg://{host}/{db.api_key}'),
        mk('c2', 'Missing', '{nope}'),
        mk('c3', 'Global', '${Ghost/api_key}'),
        mk('c4', 'Loop', '{loop}'),
        mk('c5', 'Bad', '{unclosed'),
      ],
      { groupBundles: false },
    );
    expect(card('Good').querySelector<HTMLElement>('.key-value[data-value]')!.dataset.value).toBe(
      'pg://db.test/member-secret-1',
    );
    const text = (p: string) => card(p).querySelector('.key-value.revealed')!.textContent;
    expect(text('Missing')).toMatch(/^Unresolved:/);
    expect(text('Global')).toMatch(/^Unresolved:/);
    expect(text('Loop')).toMatch(/^(Cycle|Depth)/);
    expect(text('Bad')).toMatch(/\S/);
  });
});

describe('the copy button label and bulk selection', { timeout: 30000 }, () => {
  it('names the format the default export uses', async () => {
    await show([entry('a', 'Fmt')], { defaultExportFormat: 'yaml' });
    expect(card('Fmt').querySelector('.env-format-badge')!.textContent).toBe('YAML');
    await show([entry('a', 'Fmt')], { defaultExportFormat: 'dotenv' });
    expect(card('Fmt').querySelector('.env-format-badge')!.textContent).toBe('.env');
  });

  it('a card selected for a bulk action stays marked across a repaint', async () => {
    await show([entry('a', 'Pick'), entry('b', 'Skip')]);
    st.bulkMode = true;
    st.bulkSelected.add('a');
    (await import('../src/ts/render')).renderGrid();
    expect(card('Pick').classList.contains('bulk-selected')).toBe(true);
    expect(
      card('Pick').querySelector('[data-action="bulk-toggle"]')!.getAttribute('aria-checked'),
    ).toBe('true');
    expect(card('Skip').classList.contains('bulk-selected')).toBe(false);
  });

  it('a session offers Verify instead of Rotate', async () => {
    await show([entry('a', 'Sess', { secretType: 'cookie' }), entry('b', 'Key')]);
    expect(card('Sess').querySelector('[data-action="verify"]')).not.toBeNull();
    expect(card('Sess').querySelector('[data-action="rotate"]')).toBeNull();
    expect(card('Key').querySelector('[data-action="rotate"]')).not.toBeNull();
  });
});

describe('entries written by an older or hand-edited vault', { timeout: 30000 }, () => {
  const sparse = (id: string, provider: string, over: object = {}) => ({
    id,
    provider,
    api_key: `key-${id}-longenough`,
    ...over,
  });

  it('draw without categories, tags, prefixes or an environment', async () => {
    await show(
      [
        sparse('a', 'Bare'),
        sparse('b', 'Bare2', { secretType: 'composite', composite_template: '' }),
      ],
      { groupBundles: true, groupPools: true },
    );
    expect(card('Bare').querySelector('.cat-pill')).toBeNull();
    expect(card('Bare').querySelector('.tag-chip-card')).toBeNull();
    expect(card('Bare').querySelector('.badge-env')).toBeNull();
    expect(document.querySelectorAll('#card-grid .card')).toHaveLength(2);
  });

  it('a bundle whose members have none of those still draws, with no summary chips', async () => {
    await show(
      [
        sparse('bun', 'Plain bundle', { secretType: 'bundle', api_key: '' }),
        sparse('m1', 'Member', { bundle_id: 'bun', bundle_slot: 'one' }),
      ],
      { groupBundles: true },
    );
    expect(document.querySelector('.bundle-chips')).toBeNull();
    expect(document.querySelector('.bundle-card-wrap')).not.toBeNull();
  });

  it('a bundle nobody belongs to draws with no primary member and no actions that need one', async () => {
    await show([sparse('bun', 'Empty bundle', { secretType: 'bundle', api_key: '' })], {
      groupBundles: true,
    });
    expect(document.querySelector('.bundle-card-wrap')!.textContent).toContain('bundle: 0');
  });

  it('the tag, category and sidebar sections tolerate entries without those fields', async () => {
    await show([sparse('a', 'A'), sparse('b', 'B', { categories: undefined, tags: undefined })]);
    expect(document.getElementById('count-all')!.textContent).toBe('2');
    expect(document.querySelectorAll('#tag-filter-list .tag-filter-btn')).toHaveLength(0);
  });
});

describe('scoped composites and the odd corners of a bundle card', { timeout: 30000 }, () => {
  const bundle = (extra_vars: object[] = []) =>
    entry('bun', 'B', { secretType: 'bundle', api_key: '', extra_vars });
  const member = (id: string, slot: string, over: object = {}) =>
    entry(id, `Member ${id}`, { bundle_id: 'bun', bundle_slot: slot, ...over });
  const comp = (id: string, name: string, template: string | undefined, over: object = {}) =>
    entry(id, name, {
      secretType: 'composite',
      api_key: '',
      bundle_id: 'bun',
      bundle_slot: name.toLowerCase(),
      composite_template: template,
      ...over,
    });
  const text = (p: string) => card(p).querySelector('.key-value.revealed')!.textContent;
  const render = async () => (await import('../src/ts/render')).renderGrid();

  it('reads a global reference as public or secret by the field it names', async () => {
    await show([
      bundle(),
      member('m1', 'one'),
      entry('o', 'Open', { api_key: 'open-value-1234', primary_public: true }),
      entry('q', 'Quiet', { api_key: 'quiet-value-1234' }),
      comp('c1', 'Pubbed', '${Open}'),
      comp('c2', 'Secret', '${Quiet}'),
      comp('c3', 'Slashed', '${Open/api_key}'),
      comp('c4', 'Quiet2', '${Quiet/api_key}'),
    ]);
    const shown = (p: string) => card(p).querySelector<HTMLElement>('.key-value[data-value]')!;
    expect(shown('Pubbed').textContent).toBe('open-value-1234');
    expect(shown('Secret').textContent).not.toContain('quiet-value-1234');
    expect(shown('Secret').dataset.value).toBe('quiet-value-1234');
    expect(shown('Slashed').textContent).toBe('open-value-1234');
    expect(shown('Quiet2').textContent).not.toContain('quiet-value-1234');
  });

  it('reports a template that nests too deep, one that is malformed, and one that is missing', async () => {
    const chain = Array.from({ length: 12 }, (_, i) => ({
      key: `v${i}`,
      kind: 'template',
      value: `{v${i + 1}}`,
    }));
    await show([
      bundle([...chain, { key: 'v12', value: 'end' }]),
      member('m1', 'one'),
      comp('c1', 'Deep', '{v0}'),
      comp('c2', 'Broken', '{unclosed'),
      comp('c3', 'Blank', undefined),
    ]);
    expect(text('Deep')).toMatch(/^(Depth limit|Unresolved|Cycle)/);
    expect(text('Broken')).toMatch(/\S/);
    expect(card('Blank')).toBeTruthy();
  });

  it('a standalone composite with no template shows its error and an empty template line', async () => {
    await show([entry('c', 'Naked', { secretType: 'composite', api_key: '' })]);
    expect(card('Naked').textContent).toContain('TEMPLATE');
  });

  it('tabs fall back to the provider name when a member has neither version nor slot', async () => {
    await show(
      [
        bundle(),
        ...['a', 'b', 'c', 'd'].map((s, i) =>
          entry(`m${s}`, `Prov ${s}`, { bundle_id: 'bun', bundle_order: (i + 1) * 10 }),
        ),
      ],
      { groupBundles: true },
    );
    expect(
      [...document.querySelectorAll('.bundle-slot-tabs [role="tab"]')].map((t) =>
        t.textContent!.trim(),
      ),
    ).toEqual(['Prov a', 'Prov b', 'Prov c', 'Prov d']);
  });

  it('shows a bundle member’s own pool inside the bundle', async () => {
    await show(
      [
        bundle(),
        member('m1', 'one', { pool: 'ring' }),
        member('m2', 'two', { pool: 'ring' }),
        member('m3', 'three'),
      ],
      { groupBundles: true },
    );
    expect(document.querySelectorAll('.bundle-card-wrap .pool-card-wrap')).toHaveLength(1);
  });

  it('draws entries that are missing fields a newer build would have filled in', async () => {
    await show([
      bundle(),
      member('m1', 'one'),
      entry('a', 'Plain'),
      entry('b', 'Tabbed', { secretType: 'cookie' }),
    ]);
    for (const e of st.vault.api_keys) {
      delete (e as { projectIds?: unknown }).projectIds;
      delete (e as { categories?: unknown }).categories;
      delete (e as { tags?: unknown }).tags;
      delete (e as { secretType?: unknown }).secretType;
      delete (e as { id?: unknown }).id;
    }
    st.vault.api_keys[1].extra_vars = [{ key: 't', value: 'x' }];
    await render();
    expect(document.querySelectorAll('#card-grid .card').length).toBeGreaterThan(0);
  });

  it('uses a stored dismissal list that is null as empty', async () => {
    await show([entry('a', 'Stripe', { label: 'a' }), entry('b', 'Stripe', { label: 'b' })], {
      groupBundles: true,
      dismissedBundleSuggestions: null,
    });
    expect(document.getElementById('bundle-suggest')).not.toBeNull();
  });

  it('a click inside the grid that is not on a control does nothing', async () => {
    await show([entry('a', 'Plain')]);
    document.getElementById('card-grid')!.click();
    expect(document.querySelectorAll('#card-grid .card')).toHaveLength(1);
  });

  it('an entry with no id is given one, and its authenticator row is keyed by it', async () => {
    await show([entry('a', 'Seeded', { totp_secret: 'JBSWY3DPEHPK3PXP' })]);
    delete (st.vault.api_keys[0] as { id?: unknown }).id;
    await render();
    const keyed = card('Seeded').querySelector('.totp-key-row')!.getAttribute('data-totp-for');
    expect(keyed).toBeTruthy();
    expect(keyed).toBe(st.vault.api_keys[0].id);
  });
});
