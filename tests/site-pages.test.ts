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
    ['UnENVerse_9.9.9_aarch64.dmg', 1],
    ['unv-9.9.9-macos-aarch64.tar.gz', 1],
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

describe('the glossary page', () => {
  it('lists every defined term with its own anchor, says how many are defined, and publishes the term set', () => {
    const { terms } = JSON.parse(
      fs.readFileSync(path.join(root, 'glossary/terms.json'), 'utf8'),
    ) as { terms: { id: string; term: string; definition: string }[] };
    const { pending } = JSON.parse(
      fs.readFileSync(path.join(root, 'glossary/pending.json'), 'utf8'),
    ) as { pending: string[] };
    const html = read('glossary/index.html');
    for (const t of terms) {
      expect(html, t.id).toContain(`id="${t.id}"`);
      expect(html, t.id).toContain(t.definition.replace(/&/g, '&amp;'));
    }
    expect(html).toContain(
      pending.length
        ? `${terms.length} terms are defined so far; ${pending.length} more`
        : `${terms.length} terms are defined. Every field, setting and secret type`,
    );
    const ld = JSON.parse(
      /<script type="application\/ld\+json">(\{"@context":"https:\/\/schema.org","@type":"DefinedTermSet".*?\})<\/script>/.exec(
        html,
      )![1],
    );
    expect(ld.hasDefinedTerm).toHaveLength(terms.length);
    expect(read('sitemap.xml')).toContain('/glossary/');
  });
});

describe('site search', () => {
  const pages = (d: string): string[] =>
    fs
      .readdirSync(d, { withFileTypes: true })
      .flatMap((e) =>
        e.isDirectory()
          ? pages(path.join(d, e.name))
          : e.name === 'index.html'
            ? [path.join(d, e.name)]
            : [],
      );

  // The bug: the header showed something shaped like a search bar that was only a link to the
  // guides page.
  it('puts a real search form, not a link dressed as one, in the header of every page', () => {
    for (const f of pages(out)) {
      const html = fs.readFileSync(f, 'utf8');
      if (!html.includes('<header class="site">')) continue;
      const rel = path.relative(out, f);
      expect(html, rel).toContain('<form class="searchlike" role="search" action="/search/"');
      expect(html, rel).toMatch(/<input type="search" name="q"[^>]*aria-label="Search the site"/);
      expect(html, rel).not.toContain('<a class="searchlike"');
    }
  });

  it('indexes pages, glossary terms and FAQ questions, each with a place that exists', () => {
    const idx = JSON.parse(read('search-index.json')) as {
      t: string;
      u: string;
      k: string;
      d: string;
    }[];
    for (const e of idx) {
      expect(e.t, e.u).toBeTruthy();
      expect(e.u.startsWith('/'), e.u).toBe(true);
      expect(['page', 'guide', 'glossary', 'faq']).toContain(e.k);
    }
    const urls = new Set(idx.map((e) => e.u));
    expect(urls.has('/guides/importing/')).toBe(true);
    expect(idx.find((e) => e.u === '/guides/importing/')?.k).toBe('guide');
    expect(idx.some((e) => e.k === 'glossary' && e.u === '/glossary/#api-key')).toBe(true);
    expect([...urls].some((u) => u.startsWith('/search/'))).toBe(false);
    expect([...urls].some((u) => /^\/changelog\/[^/]+\//.test(u))).toBe(false);
    const faq = read('faq/index.html');
    for (const e of idx.filter((x) => x.k === 'faq'))
      expect(faq, e.u).toContain(`id="${e.u.split('#')[1]}"`);
  });

  it('ranks a title hit above a description hit and needs every word to match', async () => {
    // @ts-expect-error plain browser script without types
    await import('../website/assets/site-search.js');
    const { rank } = (
      globalThis as unknown as {
        UnvSiteSearch: { rank: (e: unknown[], q: string) => { u: string }[] };
      }
    ).UnvSiteSearch;
    const idx = [
      { t: 'Importing from another manager', u: '/a/', k: 'guide', d: 'move your data', h: '' },
      { t: 'Features', u: '/b/', k: 'page', d: 'includes importing and export', h: '' },
      { t: 'Nodes', u: '/c/', k: 'guide', d: 'deploy configs', h: '' },
    ];
    expect(rank(idx, 'import').map((e) => e.u)).toEqual(['/a/', '/b/']);
    expect(rank(idx, 'import nodes')).toEqual([]);
    expect(rank(idx, '')).toEqual([]);
    expect(rank(idx, 'zzz')).toEqual([]);
  });

  it('has a results page that is not indexed and ships the script', () => {
    const html = read('search/index.html');
    expect(html).toContain('name="robots" content="noindex"');
    expect(html).toContain('id="search-results"');
    expect(fs.existsSync(path.join(out, 'assets/site-search.js'))).toBe(true);
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
      if (html.includes('name="robots" content="noindex"')) continue;
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
