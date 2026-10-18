// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
/**
 * Dropping a file on the vault: .env, Python config, YAML and JSON, and the
 * "which variables, with what settings" dialog they lead to.
 *
 * The questions: does the dialog show what is in the file (and not a whole
 * secret), do the chosen category/project/price/environment land on the new
 * entries, is a Python config offered as a bundle that keeps its templates, and
 * does a file that is not usable say so rather than do nothing.
 */
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { st } from '../src/ts/state';
import {
  handleFileSelect,
  confirmEnvImport,
  closeEnvImportModal,
  openEnvImportModal,
  copyAll,
  exportAs,
  exportK8sSecret,
  exportTfvars,
  exportEncryptedBackup,
} from '../src/ts/import-export';
import { loadRealIndexHtml, makeEntry, makeProject, makeVault, resetState } from './helpers';

const toasts: string[] = [];
let confirmAnswer = true;
let promptAnswer: string | null = 'Config bundle';
const saved: { content: string; name: string }[] = [];
let saveResult: any = { ok: true, path: '/dl/f' };
const copied: string[] = [];
vi.mock('../src/ts/utils', async (importOriginal) => {
  const real = await importOriginal<typeof import('../src/ts/utils')>();
  return {
    ...real,
    showToast: (m: string) => toasts.push(m),
    showConfirm: async () => confirmAnswer,
    showPrompt: async () => promptAnswer,
    saveFile: async (content: string, name: string) => {
      saved.push({ content, name });
      return saveResult;
    },
    clipboardWrite: async (t: string) => {
      copied.push(t);
    },
  };
});

const $ = <T extends HTMLElement = HTMLElement>(id: string) => document.getElementById(id) as T;
const flush = (ms = 120) => new Promise((r) => setTimeout(r, ms));
const names = () => [...document.querySelectorAll('.env-import-name')].map((n) => n.textContent);

async function drop(name: string, text: string) {
  const input = document.createElement('input');
  Object.defineProperty(input, 'files', { value: [new File([text], name)] });
  handleFileSelect(input);
  await flush();
}

beforeEach(() => {
  loadRealIndexHtml();
  resetState(st);
  st.vault = makeVault({
    user_categories: ['infra'],
    projects: [
      makeProject({ id: 'Universal', name: 'Universal' }),
      makeProject({ id: 'web', name: 'Web' }),
    ],
  });
  toasts.length = 0;
  saved.length = 0;
  copied.length = 0;
  confirmAnswer = true;
  promptAnswer = 'Config bundle';
  saveResult = { ok: true, path: '/dl/f' };
  delete (window as any).__TAURI__;
});

describe('dropping a .env file', () => {
  it('lists the variables, shortening long values, and counts them', async () => {
    await drop('app.env', `A=1\nLONG=${'x'.repeat(60)}\n`);
    expect(names()).toEqual(['A', 'LONG']);
    expect($('env-import-subtitle').textContent).toContain('Found 2 variables');
    const shown = document.querySelectorAll('.env-import-val')[1].textContent!;
    expect(shown).toBe('x'.repeat(40) + '…');
    expect($('env-import-overlay').classList.contains('open')).toBe(true);
  });

  it('says so when the file holds no variables, and opens nothing', async () => {
    await drop('empty.env', '# only a comment\n');
    expect(toasts).toContain('No variables found in .env file');
    expect($('env-import-overlay').classList.contains('open')).toBe(false);
  });

  it('a single variable reads in the singular, and the category and project menus offer what the vault has', async () => {
    await drop('one.env', 'A=1');
    expect($('env-import-subtitle').textContent).toContain('Found 1 variable.');
    const cats = [...$<HTMLSelectElement>('env-import-category').options].map((o) => o.value);
    expect(cats).toEqual(['', 'infra']);
    const projects = [...$<HTMLSelectElement>('env-import-project').options].map((o) => o.value);
    expect(projects).toEqual(['web', 'Universal']);
  });

  it('imports only the ticked variables, with the chosen category, project, price and environment', async () => {
    await drop('app.env', 'A=1\nB=2\nC=3');
    document.querySelectorAll<HTMLInputElement>('.env-import-check')[1].checked = false;
    $<HTMLSelectElement>('env-import-category').value = 'infra';
    $<HTMLSelectElement>('env-import-project').value = 'web';
    $<HTMLSelectElement>('env-import-price').value = 'paid';
    $<HTMLSelectElement>('env-import-env').value = 'production';
    confirmEnvImport();
    expect(st.vault.api_keys.map((e) => e.provider)).toEqual(['A', 'C']);
    expect(st.vault.api_keys[0]).toMatchObject({
      secretType: 'env_var',
      price_type: 'paid',
      environment: 'production',
      categories: ['infra'],
      projectIds: ['web', 'Universal'],
    });
    expect(st.vault.api_keys[0].created_at).toBeTruthy();
    expect(toasts).toContain('Imported 2 variables ✓');
    expect($('env-import-overlay').classList.contains('open')).toBe(false);
  });

  it('with nothing chosen the defaults are Universal, free and no environment; one variable reads singular', async () => {
    await drop('app.env', 'A=1');
    $<HTMLSelectElement>('env-import-project').value = '';
    $<HTMLSelectElement>('env-import-price').value = '';
    confirmEnvImport();
    expect(st.vault.api_keys[0]).toMatchObject({
      price_type: 'free',
      environment: null,
      categories: [],
      projectIds: ['Universal'],
    });
    expect(toasts).toContain('Imported 1 variable ✓');
  });

  it('refuses to import when everything is unticked, and closing leaves the vault alone', async () => {
    await drop('app.env', 'A=1');
    document
      .querySelectorAll<HTMLInputElement>('.env-import-check')
      .forEach((c) => (c.checked = false));
    confirmEnvImport();
    expect(toasts).toContain('No variables selected');
    expect(st.vault.api_keys).toHaveLength(0);
    closeEnvImportModal();
    expect($('env-import-overlay').classList.contains('open')).toBe(false);
  });

  it('escapes variable names and values in the dialog', () => {
    openEnvImportModal([{ name: '<img src=x onerror=alert(1)>', value: '<b>v</b>' }]);
    expect(document.querySelector('#env-import-list img')).toBeNull();
    expect(document.querySelector('#env-import-list b')).toBeNull();
  });
});

describe('dropping a Python config', () => {
  const PY = 'host = "h.test"\nurl = f"https://{host}/x"\nPORT = 8080\n';

  it('offers it as a bundle that keeps templates, and creates the bundle when accepted', async () => {
    await drop('settings.py', PY);
    const bundle = st.vault.api_keys.find((e) => e.secretType === 'bundle')!;
    expect(bundle.provider).toBe('Config bundle');
    expect(bundle.extra_vars!.map((v) => [v.key, v.kind])).toEqual([
      ['host', 'string'],
      ['url', 'template'],
      ['PORT', 'int'],
    ]);
    expect(toasts.some((t) => t.startsWith('Imported bundle "Config bundle" (3 variables'))).toBe(
      true,
    );
  });

  it('declining the bundle falls back to the plain variable picker', async () => {
    confirmAnswer = false;
    await drop('settings.py', PY);
    expect(st.vault.api_keys).toHaveLength(0);
    expect(names()).toContain('PORT');
  });

  it('a blank bundle name cancels quietly', async () => {
    promptAnswer = '  ';
    await drop('settings.py', PY);
    expect(st.vault.api_keys).toHaveLength(0);
  });

  it('reports warnings for a name assigned twice', async () => {
    await drop('settings.py', 'k = "a"\nk = "b"\n');
    expect(toasts.some((t) => t.includes('2 variables, 1 warnings'))).toBe(true);
    expect(toasts.some((t) => t.includes('assigned more than once'))).toBe(true);
  });

  it('says so when nothing in it is a supported assignment', async () => {
    await drop('settings.py', 'import os\nprint(os.environ)\n');
    expect(toasts).toContain('No supported Python assignments found');
  });
});

describe('Python assignments', () => {
  it('reads strings, numbers and booleans, decodes escapes, and never evaluates code', async () => {
    const { parsePythonAssignments } = await import('../src/ts/import-export');
    const vars = parsePythonAssignments(
      [
        'A = "x\\ty"',
        "B = 'it\\'s \\n ok'",
        'C = 0x1F  # hex',
        'D = True',
        'E = None',
        'F = 1.5e3',
        'G = os.environ["X"]',
        'H = "unterminated',
        'I = "a" + "b"',
        "J = 'bad \\q escape'",
        'K = "bad \\q"',
        'L = r"raw"',
        '    M = "indented"',
      ].join('\n'),
    );
    expect(Object.fromEntries(vars.map((v) => [v.name, v.value]))).toEqual({
      A: 'x\ty',
      B: "it's \n ok",
      C: '0x1F',
      D: 'True',
      E: 'None',
      F: '1.5e3',
      L: 'raw',
      M: 'indented',
    });
  });
});

describe('dropping YAML and JSON', () => {
  it('a flat YAML map opens the picker with non-strings as JSON text', async () => {
    await drop('c.yaml', 'TOKEN: abc\nPORT: 8080\n');
    expect(names()).toEqual(['TOKEN', 'PORT']);
    expect([...document.querySelectorAll('.env-import-val')].map((n) => n.textContent)).toEqual([
      'abc',
      '8080',
    ]);
  });

  it('a YAML vault replaces the vault; a YAML list or scalar is not importable', async () => {
    await drop('v.yml', 'api_keys:\n  - provider: Y\n    api_key: k\n');
    expect(st.vault.api_keys.map((e) => e.provider)).toEqual(['Y']);
    await drop('l.yaml', '- a\n- b\n');
    expect(toasts).toContain('No importable data in YAML');
    await drop('s.yaml', 'just text');
    expect(toasts.filter((t) => t === 'No importable data in YAML')).toHaveLength(2);
  });

  it('broken YAML or JSON is reported, with the reason', async () => {
    await drop('bad.yaml', 'a: [unclosed');
    expect(toasts.some((t) => t.startsWith('Invalid YAML'))).toBe(true);
    await drop('bad.json', '{nope');
    expect(toasts.some((t) => t.startsWith('Invalid:'))).toBe(true);
    await drop('arr.json', '[1,2]');
    expect(toasts).toContain('No importable data in JSON');
  });

  it('a backup envelope for something other than a vault is refused', async () => {
    await drop('v.json', '{"api_keys": "nope"}');
    expect(names()).toEqual(['api_keys']);
  });

  it('a TOML file without the desktop app reports the failure, and an empty one says so', async () => {
    await drop('c.toml', 'a = 1');
    expect(toasts.some((t) => t.startsWith('TOML import failed'))).toBe(true);
    (window as any).__TAURI__ = { core: { invoke: async () => [] } };
    await drop('c.toml', '');
    expect(toasts).toContain('No supported TOML values found');
  });

  it('choosing no file does nothing', () => {
    const input = document.createElement('input');
    Object.defineProperty(input, 'files', { value: [] });
    handleFileSelect(input);
    expect(toasts).toHaveLength(0);
  });
});

describe('exports', () => {
  beforeEach(() => {
    st.vault.api_keys = [
      makeEntry({ provider: 'Stripe', api_key: 'sk-1' }),
      makeEntry({ provider: 'GitHub', api_key: 'gh-2' }),
    ];
  });

  it('copy all puts every visible key on the clipboard in the chosen format and says how many', async () => {
    copyAll('env');
    await flush();
    expect(copied[0]).toContain('sk-1');
    expect(copied[0]).toContain('gh-2');
    expect(toasts).toContain('2 keys copied');
    copyAll('yaml');
    copyAll('json');
    await flush();
    expect(copied[1]).toContain('sk-1');
    expect(JSON.parse(copied[2])).toBeTruthy();
  });

  it('vault-wide copy never emits the full profile, and says so', async () => {
    const { Settings } = await import('../src/ts/state');
    Settings.set('copyProfile', 'full');
    copyAll('env');
    await flush();
    expect(toasts.some((t) => t.includes('"full" is per-entry only'))).toBe(true);
    copyAll('json');
    await flush();
    expect(toasts).toContain('2 keys copied');
    Settings.set('copyProfile', 'basic');
  });

  it('export writes a file named for its format and reports the path or the failure', async () => {
    await exportAs('yaml');
    await exportAs('json');
    await exportAs('env');
    expect(saved.map((s) => s.name)).toEqual(['unenverse.yaml', 'unenverse.json', 'unenverse.env']);
    expect(toasts).toContain('Exported to /dl/f');
    saveResult = { ok: true, path: null };
    await exportAs('env');
    expect(toasts).toContain('Exported as .env');
    saveResult = { ok: false, error: 'disk full' };
    await exportAs('json');
    expect(toasts).toContain('Export failed: disk full');
  });

  it('Kubernetes and Terraform exports carry each entry under its generated name', async () => {
    exportK8sSecret('team');
    exportTfvars();
    await flush();
    expect(saved[0].name).toBe('team-secret.yaml');
    expect(saved[0].content).toContain('kind: Secret');
    expect(saved[0].content).toContain('"sk-1"');
    expect(saved[1].name).toBe('unenverse.tfvars');
    expect(saved[1].content).toMatch(/^stripe = "sk-1"/m);
    expect(toasts).toContain('Exported k8s Secret ✓ → /dl/f');
  });

  it('an encrypted backup is refused for a weak password and written for a strong one', async () => {
    await exportEncryptedBackup('short');
    expect(toasts).toContain('Backup password must be at least 12 characters');
    expect(saved).toHaveLength(0);
    await exportEncryptedBackup('a-strong-backup-password');
    expect(saved[0].name).toMatch(/^unenverse-\d{4}-\d{2}-\d{2}\.vaultbak$/);
    expect(saved[0].content).not.toContain('sk-1');
  });
});
