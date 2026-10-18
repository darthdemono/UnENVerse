/**
 * Entry icons: Simple Icons slugs and files embedded in the vault.
 *
 * An embedded icon is vault data and so untrusted: it ends up in an `src`
 * attribute, possibly from someone else's server. The rules worth pinning are
 * that only raster images in a bounded size are accepted (never SVG), that an
 * invalid one renders as a letter instead of as markup, and that a typed slug
 * always wins over a stored file.
 */
import { describe, it, expect, beforeEach, vi } from 'vitest';
import {
  getIconSlug,
  isEmbeddedIcon,
  validateEmbeddedIcon,
  MAX_ICON_CHARS,
  iconHTML,
  setIconField,
  readIconField,
  pickIconFile,
  openIconPicker,
  closeIconPicker,
  initIconPicker,
  iconPicker,
} from '../src/ts/icons';
import { loadRealIndexHtml } from './helpers';

const toasts: [string, string?][] = [];
vi.mock('../src/ts/utils', async (importOriginal) => {
  const real = await importOriginal<typeof import('../src/ts/utils')>();
  return { ...real, showToast: (m: string, k?: string) => toasts.push([m, k]) };
});

const $ = <T extends HTMLElement = HTMLElement>(id: string) => document.getElementById(id) as T;
const PNG = 'data:image/png;base64,iVBORw0KGgo=';
const flush = (ms = 30) => new Promise((r) => setTimeout(r, ms));

describe('slugs', () => {
  it('a custom icon wins; otherwise the provider name is normalised; unknown providers have none', () => {
    expect(getIconSlug('Anything', 'github')).toBe('github');
    expect(getIconSlug('GitHub')).toBe('github');
    expect(getIconSlug('Git Hub!')).toBe('github');
    expect(getIconSlug('zzzz not a service zzzz')).toBeNull();
    expect(getIconSlug('')).toBeNull();
  });
});

describe('embedded icons', () => {
  it('recognises a data-image value, and nothing else', () => {
    expect(isEmbeddedIcon(PNG)).toBe(true);
    expect(isEmbeddedIcon('github')).toBe(false);
    expect(isEmbeddedIcon('')).toBe(false);
    expect(isEmbeddedIcon(null)).toBe(false);
    expect(isEmbeddedIcon(undefined)).toBe(false);
  });

  it('accepts the raster types and refuses SVG, scripts and malformed data', () => {
    for (const mime of ['png', 'jpeg', 'gif', 'webp', 'bmp', 'x-icon', 'vnd.microsoft.icon'])
      expect(validateEmbeddedIcon(`data:image/${mime};base64,AAAA`)).toBeNull();
    for (const bad of [
      'data:image/svg+xml;base64,PHN2Zz48L3N2Zz4=',
      'data:image/svg+xml,<svg onload=alert(1)>',
      'data:text/html;base64,AAAA',
      'data:image/png;base64,AA"onerror="x',
      'data:image/png,AAAA',
      'javascript:alert(1)',
    ])
      expect(validateEmbeddedIcon(bad), bad).toContain('Icon must be a PNG');
  });

  it('refuses an icon over the size limit, and says the limit', () => {
    const big = `data:image/png;base64,${'A'.repeat(MAX_ICON_CHARS)}`;
    expect(validateEmbeddedIcon(big)).toBe('Icon is too large (max 96 KB of encoded data)');
    expect(validateEmbeddedIcon(`data:image/png;base64,${'A'.repeat(1000)}`)).toBeNull();
  });
});

describe('rendering an icon', () => {
  it('shows a valid embedded file as an image with the provider as alt text', () => {
    const out = String(iconHTML('GitHub', PNG));
    expect(out).toContain('si-icon-custom');
    expect(out).toContain(`src="${PNG}"`);
    expect(out).toContain('alt="GitHub"');
  });

  it('shows an invalid embedded file as the provider’s initial, never as an image', () => {
    const out = String(iconHTML('zeta', 'data:image/svg+xml;base64,PHN2Zz4='));
    expect(out).toContain('si-fallback');
    expect(out).toContain('>Z<');
    expect(out).not.toContain('<img');
  });

  it('escapes a hostile provider name in alt text and the initial', () => {
    expect(String(iconHTML('"><script>x</script>', PNG))).not.toContain('<script>');
    expect(String(iconHTML('<b>', 'data:image/svg+xml,x'))).not.toContain('<b>');
  });

  it('known provider gets its bundled icon, an unknown one its initial, and a nameless one a question mark', () => {
    expect(String(iconHTML('GitHub'))).toContain('<img');
    expect(String(iconHTML('zzzz not a service zzzz'))).toContain('>Z<');
    expect(String(iconHTML(''))).toContain('>?<');
  });
});

describe('the icon field', () => {
  const field = () => document.createElement('input');

  it('keeps an embedded file out of the visible value but remembers it, with its size', () => {
    const f = field();
    setIconField(f, PNG);
    expect(f.value).toBe('');
    expect(f.dataset.iconData).toBe(PNG);
    expect(f.placeholder).toMatch(/Custom file \(~1 KB\)/);
    expect(readIconField(f)).toBe(PNG);
  });

  it('a typed slug replaces a stored file, and clearing a file field forgets the file', () => {
    const f = field();
    setIconField(f, PNG);
    f.value = '  github ';
    expect(readIconField(f)).toBe('github');
    setIconField(f, 'gitlab');
    expect(f.dataset.iconData).toBeUndefined();
    expect(f.value).toBe('gitlab');
    expect(f.placeholder).toContain('Simple Icons slug');
    setIconField(f, null);
    expect(f.value).toBe('');
    expect(readIconField(f)).toBeUndefined();
  });

  it('copes with a missing field', () => {
    expect(() => setIconField(null, 'x')).not.toThrow();
    expect(readIconField(null)).toBeUndefined();
  });
});

describe('picking a file', () => {
  const chooseFile = (file: File | null, readerFails = false) => {
    const onPicked = vi.fn();
    const onError = vi.fn();
    pickIconFile(onPicked, onError);
    const input = document.querySelector<HTMLInputElement>('input[type=file]')!;
    expect(input).toBeTruthy();
    expect(input.style.left).toBe('-9999px');
    Object.defineProperty(input, 'files', { value: file ? [file] : [] });
    if (readerFails) {
      vi.spyOn(FileReader.prototype, 'readAsDataURL').mockImplementation(function (
        this: FileReader,
      ) {
        this.onerror?.(new ProgressEvent('error') as ProgressEvent<FileReader>);
      });
    }
    input.dispatchEvent(new Event('change'));
    return { onPicked, onError, input };
  };

  beforeEach(() => {
    document.body.innerHTML = '';
    vi.restoreAllMocks();
  });

  it('hands back a valid image as a data URI and removes its input', async () => {
    const { onPicked, onError, input } = chooseFile(
      new File([new Uint8Array([137, 80, 78, 71])], 'a.png', { type: 'image/png' }),
    );
    await flush();
    expect(onPicked).toHaveBeenCalledWith(expect.stringMatching(/^data:image\/png;base64,/));
    expect(onError).not.toHaveBeenCalled();
    expect(input.isConnected).toBe(false);
  });

  it('reports an SVG and does not accept it', async () => {
    const { onPicked, onError } = chooseFile(
      new File(['<svg/>'], 'a.svg', { type: 'image/svg+xml' }),
    );
    await flush();
    expect(onPicked).not.toHaveBeenCalled();
    expect(onError.mock.calls[0][0]).toContain('Icon must be a PNG');
  });

  it('reports an unreadable file; choosing nothing or cancelling just cleans up', async () => {
    const failed = chooseFile(new File(['x'], 'a.png', { type: 'image/png' }), true);
    await flush();
    expect(failed.onError).toHaveBeenCalledWith('Could not read that file');
    expect(failed.input.isConnected).toBe(false);
    vi.restoreAllMocks();
    const none = chooseFile(null);
    await flush();
    expect(none.onPicked).not.toHaveBeenCalled();
    expect(none.input.isConnected).toBe(false);
    pickIconFile(vi.fn(), vi.fn());
    const input = document.querySelector<HTMLInputElement>('input[type=file]')!;
    input.dispatchEvent(new Event('cancel'));
    expect(input.isConnected).toBe(false);
    pickIconFile(vi.fn(), vi.fn());
    window.dispatchEvent(new Event('focus'));
    await flush(350);
    expect(document.querySelector('input[type=file]')).toBeNull();
  });
});

describe('the picker dialog', () => {
  const names = () => [...document.querySelectorAll('.icon-item-name')].map((n) => n.textContent);

  beforeEach(() => {
    loadRealIndexHtml();
    toasts.length = 0;
    iconPicker.target = null;
    iconPicker.selected = null;
    initIconPicker();
  });

  it('opens with every icon, filters by slug or name, and says when nothing matches', () => {
    openIconPicker();
    expect($('icon-picker-overlay').classList.contains('open')).toBe(true);
    const all = names().length;
    expect(all).toBeGreaterThan(100);
    const search = $<HTMLInputElement>('icon-search');
    search.value = 'GITHUB';
    search.dispatchEvent(new Event('input'));
    expect(names().some((n) => /github/i.test(n!))).toBe(true);
    expect(names().length).toBeLessThan(all);
    search.value = 'qqqqqqqqqqqq';
    search.dispatchEvent(new Event('input'));
    expect($('icon-grid').textContent).toContain('No icons found');
  });

  it('selecting an icon previews it in the form and highlights it; closing reports it', () => {
    const field = document.createElement('input');
    const preview = document.createElement('div');
    const onClose = vi.fn();
    openIconPicker(field, preview, onClose);
    ($('icon-grid').querySelector('[data-slug="github"]') as HTMLElement).click();
    expect(field.value).toBe('github');
    expect(preview.innerHTML).toContain('<img');
    expect($('icon-grid').querySelector('.selected')?.getAttribute('data-slug')).toBe('github');
    $('icon-picker-close').click();
    expect(onClose).toHaveBeenCalledWith('github');
    expect($('icon-picker-overlay').classList.contains('open')).toBe(false);
    expect(iconPicker.target).toBeNull();
    expect(iconPicker.selected).toBeNull();
  });

  it('without a callback, closing writes the selection to the field; with none selected it clears it', () => {
    const field = document.createElement('input');
    field.value = 'gitlab';
    const preview = document.createElement('div');
    openIconPicker(field, preview);
    expect(iconPicker.selected).toBe('gitlab');
    closeIconPicker();
    expect(field.value).toBe('gitlab');
    openIconPicker(field, preview);
    iconPicker.selected = null;
    closeIconPicker();
    expect(field.value).toBe('');
    expect(preview.innerHTML).toBe('');
    closeIconPicker();
  });

  it('closes on Escape (only while open) and on a click on the backdrop, but not inside the box', () => {
    openIconPicker();
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'a' }));
    expect($('icon-picker-overlay').classList.contains('open')).toBe(true);
    $('icon-picker-modal').click();
    expect($('icon-picker-overlay').classList.contains('open')).toBe(true);
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
    expect($('icon-picker-overlay').classList.contains('open')).toBe(false);
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
    openIconPicker();
    $('icon-picker-overlay').click();
    expect($('icon-picker-overlay').classList.contains('open')).toBe(false);
  });

  it('a typed name is applied and closes the dialog; an empty one asks for a name', () => {
    const field = document.createElement('input');
    openIconPicker(field);
    $('icon-manual-apply').click();
    expect(toasts).toContainEqual(['Type an icon name first', 'err']);
    $<HTMLInputElement>('icon-manual').value = ' mycustom ';
    $('icon-manual-apply').click();
    expect(field.value).toBe('mycustom');
    expect($('icon-picker-overlay').classList.contains('open')).toBe(false);
  });

  it('Clear removes the icon assignment', () => {
    const field = document.createElement('input');
    field.value = 'github';
    openIconPicker(field);
    $('icon-clear').click();
    expect(field.value).toBe('');
    expect($('icon-picker-overlay').classList.contains('open')).toBe(false);
  });

  it('a bundled icon that fails to load is replaced by the provider’s initial', () => {
    document.body.insertAdjacentHTML(
      'beforeend',
      '<div id="host"><img class="si-icon" alt="Zed"></div><img id="other" alt="Q">',
    );
    document.querySelector('#host img')!.dispatchEvent(new Event('error'));
    expect($('host').innerHTML).toContain('si-fallback');
    expect($('host').textContent).toBe('Z');
    document.querySelector('#other')!.dispatchEvent(new Event('error'));
    expect(document.getElementById('other')).toBeTruthy();
    document.body.insertAdjacentHTML('beforeend', '<div id="h2"><img class="si-icon"></div>');
    document.querySelector('#h2 img')!.dispatchEvent(new Event('error'));
    expect($('h2').textContent).toBe('?');
  });

  it('uploading a file selects it and closes; a bad file shows the reason', async () => {
    const field = document.createElement('input');
    openIconPicker(field);
    $('icon-upload')?.click();
    const input = document.querySelector<HTMLInputElement>('input[type=file]')!;
    Object.defineProperty(input, 'files', {
      value: [new File([new Uint8Array([1, 2, 3])], 'a.png', { type: 'image/png' })],
    });
    input.dispatchEvent(new Event('change'));
    await flush();
    expect(field.value).toMatch(/^data:image\/png;base64,/);
    expect($('icon-picker-overlay').classList.contains('open')).toBe(false);
    openIconPicker(field);
    $('icon-upload')?.click();
    const bad = document.querySelector<HTMLInputElement>('input[type=file]')!;
    Object.defineProperty(bad, 'files', {
      value: [new File(['<svg/>'], 'a.svg', { type: 'image/svg+xml' })],
    });
    bad.dispatchEvent(new Event('change'));
    await flush();
    expect(toasts.some(([m, k]) => m.includes('Icon must be a PNG') && k === 'err')).toBe(true);
  });
});
