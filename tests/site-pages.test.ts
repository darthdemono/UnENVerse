// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
// The product site's generated pages: changelog, FAQ, llms.txt and the checks that keep every page
// findable (the product name in each title and description) and its structured data true.
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { afterAll, beforeAll, describe, expect, it } from 'vitest';
// @ts-expect-error plain .mjs without types
import * as site from '../scripts/build-site.mjs';
// @ts-expect-error plain .mjs without types
import * as cl from '../scripts/changelog-site.mjs';

const root = path.resolve(__dirname, '..');
let out: string;
const read = (rel: string) => fs.readFileSync(path.join(out, rel), 'utf8');

const release = {
  tag_name: 'v9.9.9',
  published_at: '2030-01-02T00:00:00Z',
  html_url: 'https://example.invalid/r',
  assets: [
    ['UnENVerse_9.9.9_amd64.AppImage', 1],
    ['UnENVerse_9.9.9_aarch64.AppImage', 1],
    ['UnENVerse_9.9.9_arm64.deb', 1],
    ['UnENVerse-9.9.9-1.aarch64.rpm', 1],
    ['UnENVerse_9.9.9_arm64-setup.exe', 1],
    ['unv-9.9.9-linux-x86_64.tar.gz', 1],
    ['unv-9.9.9-linux-aarch64.tar.gz', 1],
    ['unv-9.9.9-linux-x86_64-musl.tar.gz', 1],
    ['unv-9.9.9-linux-aarch64-musl.tar.gz', 1],
    ['unv-9.9.9-windows-aarch64.zip', 1],
  ].map(([name, size]) => ({
    name,
    size,
    browser_download_url: `https://example.invalid/${name}`,
  })),
};

beforeAll(() => {
  out = fs.mkdtempSync(path.join(fs.realpathSync(os.tmpdir()), 'site-pages-'));
  site.build({
    src: path.join(root, 'website'),
    out,
    release,
    sums: new Map(),
    today: '2030-01-03',
  });
});
afterAll(() => fs.rmSync(out, { recursive: true, force: true }));

describe('changelog pages', () => {
  const md = `# Changelog\n\nintro\n\n## Unreleased\n\n- soon\n\n## 1.2.0 (2030-01-01)\n\n### Added\n\n- A \`code\` **bold** [link](https://example.com) and <b>raw</b>.\n\n| a | b |\n| - | - |\n| 1 | 2 |\n\n## 2030-01-01\n\n1. one\n2. two\n\n## 2030-01-01\n\ntext\n`;

  it('splits sections, gives each a unique slug and keeps the date', () => {
    const entries = cl.parseChangelog(md);
    expect(entries.map((e: { slug: string }) => e.slug)).toEqual([
      'unreleased',
      '1.2.0',
      '2030-01-01',
      '2030-01-01-2',
    ]);
    expect(entries[1].date).toBe('2030-01-01');
  });

  it('escapes raw html and renders lists, tables and inline markup', () => {
    const html = cl.renderMarkdown(cl.parseChangelog(md)[1].body);
    expect(html).toContain('<code>code</code>');
    expect(html).toContain('<strong>bold</strong>');
    expect(html).toContain('<a href="https://example.com">link</a>');
    expect(html).toContain('&lt;b&gt;raw&lt;/b&gt;');
    expect(html).toContain('<th scope="col">a</th>');
    expect(html).not.toContain('<b>');
  });

  it('writes an index, one page per section and an Atom feed without the unreleased section', () => {
    expect(read('changelog/index.html')).toContain('UnENVerse changelog');
    const first = cl.parseChangelog(fs.readFileSync(path.join(root, 'CHANGELOG.md'), 'utf8'))[0];
    expect(fs.existsSync(path.join(out, 'changelog', first.slug, 'index.html'))).toBe(true);
    const feed = read('changelog/feed.xml');
    expect(feed).toContain('<feed xmlns="http://www.w3.org/2005/Atom">');
    expect(feed).not.toContain('/changelog/unreleased/');
    expect(read('sitemap.xml')).toContain('/changelog/');
  });
});

describe('the FAQ', () => {
  it('shows the same answers it publishes as FAQPage data', () => {
    const items = JSON.parse(
      fs.readFileSync(path.join(root, 'website/partials/faq.json'), 'utf8'),
    ) as { q: string; a: string }[];
    const html = read('faq/index.html');
    const ld = JSON.parse(
      /<script type="application\/ld\+json">(\{"@context":"https:\/\/schema.org","@type":"FAQPage".*?\})<\/script>/.exec(
        html,
      )![1],
    );
    expect(ld.mainEntity).toHaveLength(items.length);
    for (const { q } of items) expect(html).toContain(q.replace(/&/g, '&amp;'));
    for (const must of ['Rust', 'SQLCipher', 'Argon2id']) expect(html).toContain(must);
  });
});

describe('every page can be found', () => {
  const pages = (): string[] => {
    const walk = (d: string): string[] =>
      fs.readdirSync(d, { withFileTypes: true }).flatMap((e) => {
        const p = path.join(d, e.name);
        return e.isDirectory() ? walk(p) : p.endsWith('.html') ? [p] : [];
      });
    return walk(out);
  };

  it('names the product in the title and the description, and keeps the description short', () => {
    const bad: string[] = [];
    for (const f of pages()) {
      const html = fs.readFileSync(f, 'utf8');
      if (/name="robots" content="noindex"/.test(html)) continue;
      const title = /<title>([^<]*)<\/title>/.exec(html)?.[1] ?? '';
      const desc = /<meta name="description" content="([^"]*)"/.exec(html)?.[1] ?? '';
      if (!title.includes('UnENVerse')) bad.push(`${path.relative(out, f)}: title "${title}"`);
      if (!desc.includes('UnENVerse')) bad.push(`${path.relative(out, f)}: description "${desc}"`);
      if (desc.length > 200)
        bad.push(`${path.relative(out, f)}: description is ${desc.length} characters`);
    }
    expect(bad).toEqual([]);
  });

  it('carries structured data and microdata on every page', () => {
    for (const rel of [
      'index.html',
      'features/index.html',
      'download/index.html',
      'security/index.html',
    ]) {
      const html = read(rel);
      expect(html).toContain('application/ld+json');
      expect(html).toContain('itemtype="https://schema.org/WebPage"');
      expect(html).toContain('rel="canonical"');
    }
    expect(read('index.html')).toContain('"@type":"WebSite"');
  });
});

describe('llms.txt', () => {
  it('indexes every product page and the full-text companion', () => {
    const txt = read('llms.txt');
    for (const p of [
      'features',
      'faq',
      'architecture',
      'changelog',
      'download',
      'quickstart',
      'security',
      'roadmap',
    ])
      expect(txt).toContain(`/${p}/`);
    expect(txt).toContain('llms-full.txt');
    expect(txt).toContain('9.9.9');
    expect(txt.length).toBeGreaterThan(6000);
    const full = read('llms-full.txt');
    expect(full).toContain('Why SQLCipher?');
    expect(full).toContain('vault-core');
  });
});

describe('the download page', () => {
  it('lists arm64 and static builds as their own rows and shows the Docker image', () => {
    const rows = site.classify(release.assets);
    expect(rows.desktop.map((r: { label: string }) => r.label).join('\n')).toMatch(/arm64/);
    expect(rows.cli.map((r: { label: string }) => r.label).join('\n')).toMatch(/static/);
    const dl = read('download/index.html');
    expect(dl).toContain('docker run');
    expect(dl).toContain('ghcr.io/darthdemono/unenverse/unv-server:9.9.9');
    expect(dl).toContain('linux/arm64');
  });
});

describe('the architecture page', () => {
  it('names every crate in the workspace', () => {
    const members = /members\s*=\s*\[([^\]]*)\]/
      .exec(fs.readFileSync(path.join(root, 'Cargo.toml'), 'utf8'))![1]
      .split(',')
      .map((m) => m.replace(/["\s]/g, ''))
      .filter(Boolean);
    const html = read('architecture/index.html');
    for (const m of members) expect(html, m).toContain(m);
  });
});
