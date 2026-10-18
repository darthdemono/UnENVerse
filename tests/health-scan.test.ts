/**
 * Health scan: what the vault owner should be told, and what they should not.
 *
 * A scan that cries wolf is as bad as one that stays quiet: people stop reading
 * it. So each rule is pinned from both sides, the finding it must raise and the
 * near-miss it must leave alone.
 */
import { describe, it, expect, beforeAll, beforeEach, vi } from 'vitest';
import { st, resetViewState } from '../src/ts/state';
import { loadRealIndexHtml, makeEntry, makeProject, makeVault, resetState } from './helpers';

const toasts: [string, string?][] = [];
vi.mock('../src/ts/utils', async (importOriginal) => {
  const real = await importOriginal<typeof import('../src/ts/utils')>();
  return {
    ...real,
    showToast: (m: string, k?: string) => toasts.push([m, k]),
    showConfirm: async () => true,
  };
});

const $ = (id: string) => document.getElementById(id)!;
const day = (n: number) => new Date(Date.now() + n * 86_400_000).toISOString();
let boundNodes: ChildNode[] = [];

beforeAll(async () => {
  loadRealIndexHtml();
  (await import('../src/ts/tools-markup')).mountToolsPanes();
  (await import('../src/ts/tools')).initTools();
  boundNodes = Array.from(document.body.childNodes);
});

beforeEach(() => {
  toasts.length = 0;
  document.body.innerHTML = '';
  for (const node of boundNodes) document.body.appendChild(node);
  resetState(st);
  resetViewState();
  ($('health-filter-severity') as HTMLSelectElement).value = 'all';
  ($('health-filter-type') as HTMLSelectElement).value = '';
  $('health-filter-severity').dispatchEvent(new Event('change'));
  $('health-filter-type').dispatchEvent(new Event('change'));
});

/** A clean, described, already-rotated entry, so a test only adds the one thing it is about. */
const sound = (over: object = {}) =>
  makeEntry({
    provider: 'Sound',
    api_key: 'a-perfectly-long-random-value-1',
    description: 'used by the build',
    last_rotated_at: day(-1),
    ...over,
  } as any);

const scan = (entries: any[], projects: any[] = []) => {
  st.vault = makeVault({ api_keys: entries });
  st.vault.projects.push(...projects);
  $('health-scan-btn').click();
  return $('health-results').textContent ?? '';
};

describe('a healthy vault', () => {
  it('says so', () => {
    expect(scan([sound()])).toContain('No issues found');
  });
});

describe('credentials that are exposed or weak', () => {
  it('flags a compromised entry as critical', () => {
    const out = scan([sound({ compromised: true })]);
    expect(out).toContain('Marked COMPROMISED');
    expect(out).toContain('Critical');
  });

  it('flags a short password but not a short value in an entry that keeps its payload elsewhere', () => {
    expect(scan([sound({ secretType: 'password', api_key: 'short1' })])).toContain(
      'Short or weak secret value',
    );
    expect(
      scan([sound({ secretType: 'env_var', api_key: '', extra_vars: [{ key: 'A', value: 'x' }] })]),
    ).not.toContain('Short or weak');
  });

  it('does not call a long token-shaped key weak', () => {
    expect(
      scan([sound({ secretType: 'api_key', api_key: 'sk_live_0123456789abcdefABCDEF' })]),
    ).not.toContain('Short or weak');
  });

  it('flags a secret that starts with a dictionary word', () => {
    expect(scan([sound({ api_key: 'Password-but-long-enough-2026' })])).toContain(
      'common weak value',
    );
    expect(scan([sound({ api_key: 'xqzv-long-and-random-0000' })])).not.toContain(
      'common weak value',
    );
  });

  it('names every entry that shares one secret value', () => {
    const out = scan([
      sound({ provider: 'One', api_key: 'shared-secret-value-xyz' }),
      sound({ provider: 'Two', api_key: 'shared-secret-value-xyz' }),
      sound({ provider: 'Three', api_key: 'another-distinct-value-1' }),
    ]);
    expect(out).toContain('Same secret value as Two');
    expect(out).toContain('Same secret value as One');
    expect(out.match(/Same secret value/g)).toHaveLength(2);
  });

  it('ignores very short values when looking for duplicates', () => {
    expect(
      scan([
        sound({ provider: 'A', api_key: 'abc12' }),
        sound({ provider: 'B', api_key: 'abc12' }),
      ]),
    ).not.toContain('Same secret value');
  });
});

describe('dates and rotation', () => {
  it('flags expired as critical and expiring within 30 days as a warning, not later ones', () => {
    expect(scan([sound({ expires_at: day(-2) })])).toContain('Expired on');
    const soon = scan([sound({ expires_at: day(10) })]);
    expect(soon).toContain('Expiring');
    expect(soon).not.toContain('Expired on');
    expect(scan([sound({ expires_at: day(90) })])).not.toContain('Expir');
  });

  it('flags a rotation that is overdue, using the cadence', () => {
    const out = scan([sound({ rotation_days: 30, last_rotated_at: day(-100) })]);
    expect(out).toMatch(/Rotation overdue by 70d \(every 30d\)/);
    expect(scan([sound({ rotation_days: 30, last_rotated_at: day(-5) })])).not.toContain('overdue');
  });

  it('never nags a browser session about rotation, but wants it verified', () => {
    const out = scan([
      sound({ secretType: 'cookie', rotation_days: 1, last_rotated_at: day(-100) }),
    ]);
    expect(out).not.toContain('Rotation overdue');
    expect(out).toContain('Session never verified');
    expect(scan([sound({ secretType: 'cookie', last_verified_at: day(-1) })])).not.toContain(
      'Session never verified',
    );
  });

  it('notes an entry that has never been rotated, unless it has a history', () => {
    expect(scan([sound({ last_rotated_at: undefined })])).toContain('Never rotated');
    expect(
      scan([sound({ last_rotated_at: undefined, version_history: [{ saved_at: day(-3) }] })]),
    ).not.toContain('Never rotated');
  });

  it('asks for a description when there is none', () => {
    expect(scan([sound({ description: undefined })])).toContain('No description');
    expect(
      scan([sound({ description: undefined, api_description: 'legacy field' })]),
    ).not.toContain('No description');
  });
});

describe('recovery codes', () => {
  const codes = (lines: string[]) =>
    sound({
      secretType: 'recovery_codes',
      extra_vars: [{ key: 'codes', value: lines.join('\n') }],
    });

  it('warns at two codes left and is critical at none', () => {
    expect(scan([codes(['a1', 'b2', 'c3', 'd4'])])).not.toContain('recovery codes left');
    expect(scan([codes(['a1', 'b2', 'c3\tused', 'd4\tused'])])).toContain(
      'Only 2 of 4 recovery codes left',
    );
    const none = scan([codes(['a1\tused', 'b2\tused'])]);
    expect(none).toContain('Only 0 of 2 recovery codes left');
    expect(none).toContain('Critical');
  });
});

describe('references between entries and projects', () => {
  const chunkWith = (value: string) =>
    makeProject({
      id: 'p1',
      name: 'Proj',
      chunks: [{ id: 'c', name: 'Env', chunk_type: 'env_file', fields: [{ key: 'K', value }] }],
    } as any);

  it('flags a reference to a deleted entry and suggests the closest name', () => {
    const out = scan([sound({ provider: 'GitHub' })], [chunkWith('${Githb/api_key}')]);
    expect(out).toContain('Stale ref ${Githb/api_key}');
    expect(out).toContain('did you mean ${GitHub/…}?');
  });

  it('offers no guess when nothing is close, and reports each stale ref once', () => {
    const out = scan(
      [sound({ provider: 'GitHub' })],
      [chunkWith('${Zzzzzzzzzz/api_key}'), chunkWith('${Zzzzzzzzzz/api_key}')],
    );
    expect(out).toContain('Stale ref ${Zzzzzzzzzz/api_key}');
    expect(out).not.toContain('did you mean');
  });

  it('accepts a reference that resolves', () => {
    expect(scan([sound({ provider: 'GitHub' })], [chunkWith('${GitHub/api_key}')])).not.toContain(
      'Stale ref',
    );
  });

  it('flags an entry assigned to a project that nothing references, but not a Universal-only one', () => {
    const inProject = sound({ provider: 'Lonely', projectIds: ['Universal', 'p1'] });
    expect(scan([inProject], [chunkWith('plain')])).toContain(
      'In a project but no chunk references it',
    );
    expect(scan([sound({ provider: 'Lonely', projectIds: ['Universal'] })])).not.toContain(
      'no chunk references it',
    );
    expect(scan([inProject], [chunkWith('${Lonely/api_key}')])).not.toContain(
      'no chunk references it',
    );
  });
});

describe('generated names', () => {
  it('warns when the name an entry would copy as changed since it was last copied', () => {
    const out = scan([sound({ provider: 'Renamed', last_copied_name: 'OLD_NAME' })]);
    expect(out).toContain('Generated name changed since last copy: OLD_NAME →');
  });

  it('names both sides of a collision, and an entry that collides with itself', () => {
    const out = scan([
      sound({ provider: 'Stripe', api_key: 'first-value-for-stripe-1' }),
      sound({ provider: 'Stripe', api_key: 'second-value-for-stripe-2' }),
    ]);
    expect(out).toContain('is also generated by Stripe');
    const own = scan([
      sound({ provider: 'Spotify', primary_role: 'id', extra_vars: [{ key: 'ID', value: 'x' }] }),
    ]);
    expect(own).toContain('is generated twice by this entry');
  });
});

describe('presentation', () => {
  it('lists the worst entry first and groups findings per entry', () => {
    const out = scan([
      sound({ provider: 'Zeta', description: undefined, api_key: 'zeta-unique-value-000001' }),
      sound({ provider: 'Alpha', compromised: true, api_key: 'alpha-unique-value-00001' }),
    ]);
    expect(out.indexOf('Alpha')).toBeLessThan(out.indexOf('Zeta'));
    expect(document.querySelectorAll('.health-entry-group')).toHaveLength(2);
    expect(out).toContain('1 finding');
  });

  it('filters by severity and by secret type without rescanning', () => {
    scan([
      sound({ provider: 'Bad', compromised: true, api_key: 'bad-unique-value-000000001' }),
      sound({
        provider: 'Meh',
        description: undefined,
        secretType: 'password',
        api_key: 'meh-unique-value-000000001',
      }),
    ]);
    const sev = $('health-filter-severity') as HTMLSelectElement;
    sev.value = 'high';
    sev.dispatchEvent(new Event('change'));
    expect($('health-results').textContent).toContain('Bad');
    expect($('health-results').textContent).not.toContain('Meh');
    sev.value = 'all';
    sev.dispatchEvent(new Event('change'));
    const type = $('health-filter-type') as HTMLSelectElement;
    type.value = 'password';
    type.dispatchEvent(new Event('change'));
    expect($('health-results').textContent).toContain('Meh');
    expect($('health-results').textContent).not.toContain('Bad');
    type.value = 'ssh_key';
    type.dispatchEvent(new Event('change'));
    expect($('health-results').textContent).toContain('No findings match this filter');
  });

  it('puts findings about a project or chunk, not an entry, under Other', () => {
    const out = scan(
      [sound()],
      [
        makeProject({
          id: 'p1',
          name: 'Proj',
          chunks: [
            {
              id: 'c',
              name: 'Env',
              chunk_type: 'env_file',
              fields: [{ key: 'K', value: '${Nope/x}' }],
            },
          ],
        } as any),
      ],
    );
    expect(out).toContain('Other');
    expect(out).toContain('Proj / Env');
  });
});

describe('findings that need more than one entry or a chunk', () => {
  it('names a renamed variable against what the last copy produced', () => {
    const out = scan([
      sound({ provider: 'Renamed', last_copied_name: 'OLD_NAME' }),
      sound({ provider: 'Steady', api_key: 'another-long-value-2', last_copied_name: 'STEADY' }),
    ]);
    expect(out).toContain('Generated name changed since last copy: OLD_NAME → RENAMED');
    expect(out).not.toContain('STEADY →');
  });

  it('flags two entries that generate the same variable, naming the other side, and one that clashes with itself', () => {
    const out = scan([
      sound({ provider: 'Same', api_key: 'first-long-random-value-1' }),
      sound({ provider: 'Same', api_key: 'second-long-random-value-2' }),
      sound({
        provider: 'Alone',
        api_key: 'third-long-random-value-3',
        primary_role: 'id',
        extra_vars: [{ key: 'ID', value: 'x' }],
      }),
    ]);
    expect(out).toContain('SAME is also generated by Same');
    expect(out).toContain('ALONE_ID is generated twice by this entry');
  });

  it('suggests the closest provider for a stale reference and says nothing when none is close', () => {
    const chunk = (v: string) => ({
      id: v,
      name: 'env',
      chunk_type: 'env_file',
      fields: [{ key: 'K', value: v, field_type: 'env_var' }],
    });
    const out = scan(
      [sound({ provider: 'Stripe' })],
      [
        makeProject({ id: 'a', name: 'App', chunks: [chunk('${Stipe/key}')] } as any),
        makeProject({ id: 'b', name: 'Other', chunks: [chunk('${Zzzzzzzzzzz/key}')] } as any),
        makeProject({
          id: 'c',
          name: 'Dupe',
          chunks: [chunk('${Stipe/key}'), chunk('${Stipe/key}')],
        } as any),
      ],
    );
    expect(out).toContain('Stale ref ${Stipe/key} — did you mean ${Stripe/…}?');
    expect(out).toContain('Stale ref ${Zzzzzzzzzzz/key}');
    expect(out).not.toContain('Zzzzzzzzzzz/key} — did you mean');
    expect(out.match(/Stale ref \$\{Stipe\/key\}/g)).toHaveLength(2);
  });

  it('flags an entry in a project when nothing references it, by name or by name and key id', () => {
    const referenced = {
      id: 'c1',
      name: 'env',
      chunk_type: 'env_file',
      fields: [{ key: 'K', value: '${Keyed_prod/key}', field_type: 'env_var' }],
    };
    const out = scan(
      [
        sound({ provider: 'Keyed', key_id: 'prod', projectIds: ['Universal', 'p'] }),
        sound({
          provider: 'Unused',
          api_key: 'unused-long-random-value',
          projectIds: ['Universal', 'p'],
        }),
      ],
      [makeProject({ id: 'p', name: 'P', chunks: [referenced] } as any)],
    );
    expect(out).toContain('In a project but no chunk references it');
    expect(out.match(/In a project but no chunk references it/g)).toHaveLength(1);
  });

  it('puts findings that belong to no entry under "Other"', () => {
    const out = scan(
      [sound()],
      [
        makeProject({
          id: 'p',
          name: 'P',
          chunks: [
            {
              id: 'c',
              name: 'env',
              chunk_type: 'env_file',
              fields: [{ key: 'K', value: '${Gone/key}', field_type: 'env_var' }],
            },
          ],
        } as any),
      ],
    );
    expect(out).toContain('Other');
    expect(out).toContain('Stale ref');
  });

  it('shows a plain "?" for an entry with no name, and pluralises counts', () => {
    const out = scan([sound({ provider: '', compromised: true, expires_at: day(-3) })]);
    expect(out).toContain('?');
    expect(out).toMatch(/2 findings/);
  });
});

describe('acting on a finding', () => {
  it('Delete empty bundle asks, deletes only a bundle that is still empty, and rescans', async () => {
    st.vault = makeVault({
      api_keys: [
        sound(),
        sound({ id: 'empty', provider: 'Nothing', secretType: 'bundle', api_key: '' }),
      ],
    });
    $('health-scan-btn').click();
    const btn = () =>
      document.querySelector<HTMLElement>('[data-action="health-delete-empty-bundle"]')!;
    expect(btn()).not.toBeNull();
    // Someone adds a member while the question is open: the delete must not go ahead.
    st.vault.api_keys.push(sound({ provider: 'Joined', bundle_id: 'empty' }));
    btn().click();
    await new Promise((r) => setTimeout(r, 50));
    expect(st.vault.api_keys.some((e) => e.id === 'empty')).toBe(true);
    st.vault.api_keys = st.vault.api_keys.filter((e) => e.provider !== 'Joined');
    btn().click();
    await new Promise((r) => setTimeout(r, 50));
    expect(st.vault.api_keys.some((e) => e.id === 'empty')).toBe(false);
  });

  it('a delete button for a bundle that has gone does nothing', async () => {
    st.vault = makeVault({
      api_keys: [sound({ id: 'empty', provider: 'Nothing', secretType: 'bundle', api_key: '' })],
    });
    $('health-scan-btn').click();
    const btn = document.querySelector<HTMLElement>('[data-action="health-delete-empty-bundle"]')!;
    st.vault.api_keys = [];
    btn.click();
    await new Promise((r) => setTimeout(r, 30));
    expect(st.vault.api_keys).toEqual([]);
  });

  it('a click on the results that is not on a finding does nothing', () => {
    scan([sound({ compromised: true })]);
    $('health-results').click();
    const header = document.querySelector<HTMLElement>('[data-action="health-jump"]')!;
    header.dataset.id = '';
    header.click();
  });

  it('jumping to an entry that has been deleted since the scan says so', () => {
    scan([sound({ compromised: true })]);
    st.vault.api_keys = [];
    document.querySelector<HTMLElement>('[data-action="health-jump"]')!.click();
    expect(toasts).toContainEqual(['That entry no longer exists', 'err']);
  });

  it('an empty filter result says no findings match, not that the vault is healthy', () => {
    scan([sound({ compromised: true })]);
    ($('health-filter-severity') as HTMLSelectElement).value = 'low';
    $('health-filter-severity').dispatchEvent(new Event('change'));
    expect($('health-results').textContent).toContain('No findings match this filter');
  });
});
