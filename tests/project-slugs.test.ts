// SPDX-License-Identifier: PolyForm-Strict-1.0.0
// Copyright Jubair Hasan.
/**
 * Project ids ("slugs") and the create dialogs.
 *
 * A project's id is what entries, permission rules and `${...}` scopes point at.
 * So: a slug chosen by hand must survive renames and promotions, a derived one
 * must follow its name, and creating a project must never silently reuse an id.
 */
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { st, Settings } from '../src/ts/state';
import {
  validateSlug,
  hasCustomSlug,
  slugifyProjectName,
  renameProject,
  deleteProject,
  openProjectCreateModal,
  saveProjectCreate,
  setProjectCreateType,
  openCategoryCreateModal,
  saveCategoryCreate,
  doSetFilter,
  applyExperimentalTypeVisibility,
} from '../src/ts/projects';
import { loadRealIndexHtml, makeEntry, makeProject, makeVault, resetState } from './helpers';

let promptAnswer = 'x';
const toasts: string[] = [];
vi.mock('../src/ts/utils', async (importOriginal) => {
  const real = await importOriginal<typeof import('../src/ts/utils')>();
  return {
    ...real,
    showConfirm: async () => true,
    showPrompt: async () => promptAnswer,
    showToast: (m: string) => toasts.push(m),
  };
});

const $ = <T extends HTMLElement = HTMLElement>(id: string) => document.getElementById(id) as T;

beforeEach(() => {
  loadRealIndexHtml();
  resetState(st);
  toasts.length = 0;
  Settings.set('experimentalProjectTypes', false);
});

describe('validateSlug', () => {
  const existing = [makeProject({ id: 'taken', name: 'Taken' })];

  it('accepts lowercase words joined by - or /', () => {
    for (const ok of ['web', 'web-2', 'acme/web', 'a1/b2-c3'])
      expect(validateSlug(ok, existing)).toBeNull();
  });

  it('names what is wrong', () => {
    expect(validateSlug('   ', existing)).toBe('Slug cannot be empty');
    expect(validateSlug('a'.repeat(65), existing)).toContain('64 characters');
    expect(validateSlug('Universal', existing)).toContain('reserved');
    expect(validateSlug('universal', existing)).toContain('reserved');
    for (const bad of ['Web', 'a b', '-a', 'a-', 'a--b', 'a//b', '/a', 'a_b'])
      expect(validateSlug(bad, existing), bad).toContain('lowercase letters');
    expect(validateSlug('taken', existing)).toBe('Slug "taken" is already taken');
  });

  it('lets a project keep its own slug', () => {
    expect(validateSlug('taken', existing, 'taken')).toBeNull();
  });
});

describe('derived versus chosen slugs', () => {
  it('a slug is "custom" only when it differs from what the name would give', () => {
    expect(hasCustomSlug(makeProject({ id: 'my-app', name: 'My App' }))).toBe(false);
    expect(hasCustomSlug(makeProject({ id: 'prod', name: 'My App' }))).toBe(true);
    expect(slugifyProjectName('Acme / Web')).toBe('acme-/-web');
  });

  it('renaming keeps a chosen slug, so entries and rules still point at it', async () => {
    st.vault = makeVault({
      projects: [
        makeProject({ id: 'Universal', name: 'Universal' }),
        makeProject({ id: 'prod', name: 'Production' }),
      ],
      api_keys: [makeEntry({ projectIds: ['Universal', 'prod'] })],
    });
    promptAnswer = 'Live';
    await renameProject('prod');
    expect(st.vault.projects[1]).toMatchObject({ id: 'prod', name: 'Live' });
    expect(st.vault.api_keys[0].projectIds).toEqual(['Universal', 'prod']);
  });

  it('renaming re-derives a derived slug and moves the entries with it', async () => {
    st.vault = makeVault({
      projects: [
        makeProject({ id: 'Universal', name: 'Universal' }),
        makeProject({ id: 'old', name: 'Old' }),
      ],
      api_keys: [makeEntry({ projectIds: ['Universal', 'old'] })],
    });
    promptAnswer = 'Brand New';
    await renameProject('old');
    expect(st.vault.projects[1].id).toBe('brand-new');
    expect(st.vault.api_keys[0].projectIds).toEqual(['Universal', 'brand-new']);
  });

  it('a sub-project with a chosen slug keeps it when its parent is renamed or deleted', async () => {
    const build = () => {
      st.vault = makeVault({
        projects: [
          makeProject({ id: 'Universal', name: 'Universal' }),
          makeProject({ id: 'acme', name: 'Acme' }),
          makeProject({ id: 'web-prod', name: 'Acme/Web' }),
          makeProject({ id: 'acme/api', name: 'Acme/Api' }),
        ],
        api_keys: [makeEntry({ projectIds: ['Universal', 'web-prod', 'acme/api'] })],
      });
    };
    build();
    promptAnswer = 'Corp';
    await renameProject('acme');
    expect(st.vault.projects.map((p) => [p.id, p.name])).toEqual([
      ['Universal', 'Universal'],
      ['corp', 'Corp'],
      ['web-prod', 'Corp/Web'],
      ['corp/api', 'Corp/Api'],
    ]);
    expect(st.vault.api_keys[0].projectIds).toEqual(['Universal', 'web-prod', 'corp/api']);

    build();
    await deleteProject('acme');
    expect(st.vault.projects.map((p) => [p.id, p.name])).toEqual([
      ['Universal', 'Universal'],
      ['web-prod', 'Web'],
      ['api', 'Api'],
    ]);
    expect(st.vault.api_keys[0].projectIds).toEqual(['Universal', 'web-prod', 'api']);
  });

  it('renaming to a name whose derived id belongs to another project is refused', async () => {
    st.vault = makeVault({
      projects: [
        makeProject({ id: 'Universal', name: 'Universal' }),
        makeProject({ id: 'a', name: 'A' }),
        makeProject({ id: 'b', name: 'B' }),
      ],
    });
    promptAnswer = 'B';
    await renameProject('a');
    expect(toasts).toContain('Project already exists');
    expect(st.vault.projects[1].id).toBe('a');
  });
});

describe('creating a project', () => {
  const create = (name: string, o: { slug?: string; desc?: string; type?: any } = {}) => {
    openProjectCreateModal();
    if (o.type) setProjectCreateType(o.type);
    $<HTMLInputElement>('project-create-name').value = name;
    $<HTMLInputElement>('project-create-desc').value = o.desc ?? '';
    $<HTMLInputElement>('project-create-slug').value = o.slug ?? '';
    saveProjectCreate();
  };
  const created = () => st.vault.projects[st.vault.projects.length - 1];

  beforeEach(() => {
    st.vault = makeVault({ projects: [makeProject({ id: 'Universal', name: 'Universal' })] });
  });

  it('opens clean and shows the id an empty slug field will produce', () => {
    openProjectCreateModal();
    expect($('project-create-overlay').classList.contains('open')).toBe(true);
    const name = $<HTMLInputElement>('project-create-name');
    name.value = 'My New App';
    name.dispatchEvent(new Event('input'));
    expect($<HTMLInputElement>('project-create-slug').placeholder).toBe('my-new-app');
    name.value = '!!!';
    name.dispatchEvent(new Event('input'));
    expect($<HTMLInputElement>('project-create-slug').placeholder).toBe('derived from the name');
  });

  it('needs a name, and a name that makes an id', () => {
    create('   ');
    expect(toasts).toContain('Name is required');
    create('!!!');
    expect(toasts.some((t) => t.includes('empty slug'))).toBe(true);
    expect(st.vault.projects).toHaveLength(1);
  });

  it('derives the id from the name, trims it, and keeps an optional description', () => {
    create('  Home Lab  ', { desc: ' my rack ' });
    expect(created()).toMatchObject({ id: 'home-lab', name: 'Home Lab', description: 'my rack' });
    expect(created().project_type).toBeUndefined();
    expect($('project-create-overlay').classList.contains('open')).toBe(false);
  });

  it('uses a chosen slug, validated the same way the CLI does', () => {
    create('Prod', { slug: 'prod-eu' });
    expect(created().id).toBe('prod-eu');
    create('Other', { slug: 'Bad Slug' });
    expect(toasts.some((t) => t.includes('lowercase letters'))).toBe(true);
    expect(st.vault.projects.map((p) => p.id)).not.toContain('Bad Slug');
  });

  it('refuses a name or slug that would reuse an existing id', () => {
    create('Home Lab');
    create('home lab');
    expect(toasts).toContain('Project already exists');
    create('Something', { slug: 'home-lab' });
    expect(toasts.some((t) => t.includes('already taken'))).toBe(true);
    expect(st.vault.projects.filter((p) => p.id === 'home-lab')).toHaveLength(1);
  });

  it('creates the missing parents of a nested name, once', () => {
    create('Acme/Web/Edge');
    expect(st.vault.projects.map((p) => [p.id, p.name])).toEqual([
      ['Universal', 'Universal'],
      ['acme', 'Acme'],
      ['acme/web', 'Acme/Web'],
      ['acme/web/edge', 'Acme/Web/Edge'],
    ]);
    create('Acme/Web/Api');
    expect(st.vault.projects.filter((p) => p.name === 'Acme/Web')).toHaveLength(1);
  });

  it.each([
    'wireguard',
    'docker',
    'nginx',
    'kubernetes',
    'ssh_config',
    'traefik',
    'apache',
    'haproxy',
    'ansible',
    'postgres',
    'prometheus',
  ])('a %s project starts with its starter chunks', (type) => {
    Settings.set('experimentalProjectTypes', true);
    create(`P-${type}`, { type });
    expect(created().project_type).toBe(type);
    expect(created().chunks?.length ?? 0).toBeGreaterThan(0);
  });

  it('a plain project starts empty', () => {
    create('Plain');
    expect(created().chunks).toBeUndefined();
  });
});

describe('experimental project types', () => {
  it('stay hidden, and cannot be chosen, until the setting is on', () => {
    st.vault = makeVault({ projects: [makeProject({ id: 'Universal', name: 'Universal' })] });
    openProjectCreateModal();
    const experimental = [...document.querySelectorAll<HTMLElement>('.project-type-btn')].filter(
      (b) => b.style.display === 'none',
    );
    expect(experimental.length).toBeGreaterThan(0);
    setProjectCreateType(experimental[0].dataset.ptype as any);
    $<HTMLInputElement>('project-create-name').value = 'Gated';
    saveProjectCreate();
    expect(st.vault.projects[st.vault.projects.length - 1].project_type).toBeUndefined();

    Settings.set('experimentalProjectTypes', true);
    applyExperimentalTypeVisibility();
    expect(experimental[0].style.display).toBe('');
    expect(experimental[0].classList.contains('experimental')).toBe(true);
  });

  it('the type picker marks Generic as the selected radio on open', () => {
    st.vault = makeVault();
    openProjectCreateModal();
    const active = document.querySelector('.project-type-btn.active') as HTMLElement;
    expect(active.dataset.ptype).toBe('generic');
    expect(active.getAttribute('aria-checked')).toBe('true');
    expect(active.getAttribute('role')).toBe('radio');
  });
});

describe('creating a category', () => {
  it('lowercases it, rejects blanks and duplicates, and closes on success', () => {
    st.vault = makeVault({ user_categories: ['infra'] });
    openCategoryCreateModal();
    expect($('category-create-overlay').classList.contains('open')).toBe(true);
    $<HTMLInputElement>('category-create-name').value = '  ';
    saveCategoryCreate();
    expect(toasts).toContain('Name is required');
    $<HTMLInputElement>('category-create-name').value = 'INFRA';
    saveCategoryCreate();
    expect(toasts).toContain('Category already exists');
    $<HTMLInputElement>('category-create-name').value = 'Media';
    saveCategoryCreate();
    expect(st.vault.user_categories).toEqual(['infra', 'media']);
    expect($('category-create-overlay').classList.contains('open')).toBe(false);
    expect(toasts).toContain('Created "media"');
  });
});

describe('sidebar filters', () => {
  it('choosing a filter twice turns it off again', () => {
    doSetFilter('category', 'infra');
    expect(st.filter).toEqual({ type: 'category', value: 'infra' });
    doSetFilter('category', 'infra');
    expect(st.filter).toEqual({ type: 'all', value: '' });
    doSetFilter('price', 'free');
    doSetFilter('category', 'infra');
    expect(st.filter).toEqual({ type: 'category', value: 'infra' });
  });
});

describe('deleting and renaming around sub-projects and categories', () => {
  const setup = () => {
    st.vault = makeVault({
      api_keys: [
        makeEntry({
          id: 'e1',
          provider: 'E1',
          projectIds: ['Universal', 'acme', 'acme/web'],
          categories: ['infra', 'infra/db'],
        } as any),
        makeEntry({ id: 'e2', provider: 'E2', projectIds: ['Universal', 'web'] } as any),
        makeEntry({
          id: 'e3',
          provider: 'E3',
          categories: undefined,
          projectIds: undefined,
        } as any),
      ],
      user_categories: ['infra', 'infra/db', 'infra/web', 'solo'],
      projects: [
        makeProject({ id: 'Universal', name: 'Universal' }),
        makeProject({ id: 'acme', name: 'Acme' }),
        makeProject({ id: 'acme/web', name: 'Acme/Web' }),
        makeProject({ id: 'web', name: 'web' }),
      ],
    });
    st.store = { save: async () => {}, isRemote: false } as never;
  };

  it('deleting a project promotes its sub-project, avoids an id already taken, and keeps the entry in it', async () => {
    setup();
    await deleteProject('acme');
    const web = st.vault.projects.find((p) => p.name === 'Web')!;
    expect(web.id).toBe('web-1');
    expect(st.vault.api_keys[0].projectIds).toEqual(['Universal', 'web-1']);
    expect(toasts).toContain('Project "Acme" deleted');
  });

  it('deleting the selected project, or the parent of the selected one, moves the selection sensibly', async () => {
    setup();
    st.currentSelectedProjectIds = ['acme'];
    await deleteProject('acme');
    expect(st.currentSelectedProjectIds).toEqual(['Universal']);
    setup();
    st.currentSelectedProjectIds = ['acme/web'];
    await deleteProject('acme');
    expect(st.currentSelectedProjectIds).toEqual(['web-1']);
  });

  it('deleting an unknown project, or Universal, does nothing', async () => {
    setup();
    await deleteProject('ghost');
    await deleteProject('Universal');
    expect(st.vault.projects).toHaveLength(4);
  });

  it('renaming a parent renames its children, and gives a child a free id when its derived one is taken', async () => {
    setup();
    st.vault.projects.push(makeProject({ id: 'corp/web', name: 'Other' }));
    promptAnswer = 'Corp';
    await renameProject('acme');
    expect(st.vault.projects.find((p) => p.name === 'Corp/Web')!.id).toBe('corp/web-1');
    expect(st.vault.api_keys[0].projectIds).toEqual(['Universal', 'corp', 'corp/web-1']);
  });

  it('renaming the selected project follows the selection; an unchanged or blank name does nothing', async () => {
    setup();
    st.currentSelectedProjectIds = ['acme'];
    promptAnswer = 'Corp';
    await renameProject('acme');
    expect(st.currentSelectedProjectIds).toEqual(['corp']);
    promptAnswer = '  ';
    await renameProject('corp');
    promptAnswer = 'Corp';
    await renameProject('corp');
    await renameProject('ghost');
    expect(st.vault.projects.find((p) => p.id === 'corp')!.name).toBe('Corp');
  });

  it('deleting a category takes its sub-categories from the list and the entries, and resets a filter inside it', async () => {
    setup();
    st.filter = { type: 'category', value: 'infra/db' };
    await (await import('../src/ts/projects')).deleteCategory('infra');
    expect(st.vault.user_categories).toEqual(['solo']);
    expect(st.vault.api_keys[0].categories).toEqual([]);
    expect(st.filter).toEqual({ type: 'all', value: '' });
  });

  it('deleting a category with one sub-category says "category", and with none says nothing extra', async () => {
    setup();
    const confirms: string[] = [];
    const utils = await import('../src/ts/utils');
    vi.spyOn(utils, 'showConfirm').mockImplementation(async (m: string) => {
      confirms.push(m);
      return true;
    });
    st.vault.user_categories = ['a', 'a/b', 'z'];
    const { deleteCategory } = await import('../src/ts/projects');
    await deleteCategory('a');
    await deleteCategory('z');
    expect(confirms[0]).toContain('1 sub-category will be deleted too.');
    expect(confirms[1]).toBe('Delete category "z"?');
  });

  it('renaming a category carries its sub-categories, the entries and a filter; a taken name is refused', async () => {
    setup();
    st.filter = { type: 'category', value: 'infra/db' };
    promptAnswer = 'platform';
    const { renameCategory } = await import('../src/ts/projects');
    await renameCategory('infra');
    expect(st.vault.user_categories).toEqual(['platform', 'platform/db', 'platform/web', 'solo']);
    expect(st.vault.api_keys[0].categories).toEqual(['platform', 'platform/db']);
    expect(st.filter).toEqual({ type: 'category', value: 'platform/db' });
    promptAnswer = 'solo';
    await renameCategory('platform');
    expect(toasts).toContain('Category "solo" already exists');
    promptAnswer = 'platform';
    await renameCategory('platform');
    expect(st.vault.user_categories).toContain('platform');
  });
});
