#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
/**
 * Assembles the product site from website/ into an output folder.
 *
 *   node scripts/build-site.mjs --out site                    # latest release, from the GitHub API
 *   node scripts/build-site.mjs --out site --release r.json --sums SHA256SUMS   # offline / tests
 *
 * The site is plain HTML and one stylesheet. This script only does what a static
 * page cannot: it stamps the version and the download table from the latest
 * *release* (not from main, so the page never advertises a build that is not
 * published), fills partials and canonical URLs, and writes sitemap.xml.
 * It fails loudly when the release cannot be read: a site that quietly shows a
 * stale version is worse than a failed deploy.
 */
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import {
  parseChangelog,
  renderMarkdown,
  summary as changelogSummary,
  atomFeed,
  esc as escHtml,
} from './changelog-site.mjs';

export const SITE_URL = 'https://unenverse.darthdemono.com';
const REPO = 'darthdemono/UnENVerse';
const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const APP_ICON = path.join(ROOT, 'src-tauri', 'icons', '128x128.png');
/** Assets copied from the desktop app rather than stored under website/. */
export const GENERATED_SITE_ASSETS = ['/assets/app-icon.png'];

/** `SHA256SUMS` text to a name -> hex map. Accepts both `hash  name` and `hash *name`. */
export function parseSums(text) {
  const out = new Map();
  for (const line of text.split(/\r?\n/)) {
    const m = /^([0-9a-f]{64})\s+\*?(.+)$/i.exec(line.trim());
    if (m) out.set(m[2], m[1].toLowerCase());
  }
  return out;
}

export function fmtSize(bytes) {
  return bytes >= 1e6
    ? `${(bytes / 1e6).toFixed(1)} MB`
    : `${Math.max(1, Math.round(bytes / 1e3))} kB`;
}

export function esc(s) {
  return String(s).replace(
    /[&<>"]/g,
    (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' })[c],
  );
}

/** Which release assets are shown, and what each one is for. Signatures are not listed as rows. */
const KINDS = [
  {
    re: /(amd64|x86_64)\.AppImage$/,
    group: 'desktop',
    label: 'Linux x86_64, any distribution (AppImage)',
  },
  {
    re: /(arm64|aarch64)\.AppImage$/,
    group: 'desktop',
    label: 'Linux arm64, any distribution (AppImage)',
  },
  { re: /_amd64\.deb$/, group: 'desktop', label: 'Debian and Ubuntu, x86_64 (.deb)' },
  { re: /_arm64\.deb$/, group: 'desktop', label: 'Debian and Ubuntu, arm64 (.deb)' },
  { re: /\.x86_64\.rpm$/, group: 'desktop', label: 'Fedora and RHEL, x86_64 (.rpm)' },
  { re: /\.aarch64\.rpm$/, group: 'desktop', label: 'Fedora and RHEL, arm64 (.rpm)' },
  { re: /x64-setup\.exe$/, group: 'desktop', label: 'Windows x64 installer (.exe)' },
  { re: /arm64-setup\.exe$/, group: 'desktop', label: 'Windows on Arm installer (.exe)' },
  { re: /^unv-.*-linux-x86_64\.tar\.gz$/, group: 'cli', label: 'Linux x86_64: unv and unv-server' },
  { re: /^unv-.*-linux-aarch64\.tar\.gz$/, group: 'cli', label: 'Linux arm64: unv and unv-server' },
  {
    re: /^unv-.*-linux-x86_64-musl\.tar\.gz$/,
    group: 'cli',
    label: 'Linux x86_64, static (Alpine and any distribution)',
  },
  {
    re: /^unv-.*-linux-aarch64-musl\.tar\.gz$/,
    group: 'cli',
    label: 'Linux arm64, static (Alpine and any distribution)',
  },
  {
    re: /^unv-.*-windows-x86_64\.zip$/,
    group: 'cli',
    label: 'Windows x64: unv.exe and unv-server.exe',
  },
  {
    re: /^unv-.*-windows-aarch64\.zip$/,
    group: 'cli',
    label: 'Windows on Arm: unv.exe and unv-server.exe',
  },
];

export function classify(assets) {
  const rows = { desktop: [], cli: [] };
  for (const k of KINDS) {
    const a = assets.find((x) => k.re.test(x.name));
    if (a) rows[k.group].push({ ...k, asset: a });
  }
  return rows;
}

export function table(rows, sums) {
  if (!rows.length) return '<p>No files in this release.</p>';
  const body = rows
    .map(({ label, asset }) => {
      const sha = sums.get(asset.name);
      return `<tr><td>${esc(label)}</td><td><a href="${esc(asset.browser_download_url)}">${esc(asset.name)}</a></td><td>${fmtSize(asset.size)}</td><td class="hash">${sha ? esc(sha) : 'see SHA256SUMS'}</td></tr>`;
    })
    .join('\n');
  return `<div class="scroll"><table><thead><tr><th scope="col">For</th><th scope="col">File</th><th scope="col">Size</th><th scope="col">SHA-256</th></tr></thead><tbody>\n${body}\n</tbody></table></div>`;
}

export function jsonLd(version) {
  const doc = {
    '@context': 'https://schema.org',
    '@type': 'SoftwareApplication',
    name: 'UnENVerse',
    applicationCategory: 'SecurityApplication',
    operatingSystem: 'Linux, Windows',
    softwareVersion: version,
    license: 'https://www.apache.org/licenses/LICENSE-2.0',
    downloadUrl: `${SITE_URL}/download/`,
    url: `${SITE_URL}/`,
    image: `${SITE_URL}/assets/og.png`,
    description:
      'Self-hosted secrets manager: a desktop app, a command-line tool and an optional server over one encrypted vault. People, scripts and AI agents use credentials without seeing them.',
    featureList: [
      'Encrypted local vault (SQLCipher, Argon2id)',
      'Secrets used by scripts and AI agents without being printed',
      'Config files that follow a rotated secret',
      'Optional team server with per-user permissions',
    ],
    author: AUTHOR,
    offers: { '@type': 'Offer', price: '0', priceCurrency: 'USD' },
  };
  return JSON.stringify(doc).replace(/</g, '\\u003c');
}

/** The maintainer, as search engines should link the site to them. */
export const AUTHOR = {
  '@type': 'Person',
  name: 'Jubair Hasan',
  alternateName: 'DarthDemono',
  url: 'https://darthdemono.com/',
  sameAs: [
    'https://github.com/darthdemono',
    'https://www.linkedin.com/in/darthdemono/',
    'https://darthdemono.com/',
  ],
};

const attr = (html, re) => re.exec(html)?.[1];
const escAttr = (v) => v.replace(/&/g, '&amp;').replace(/"/g, '&quot;');

/**
 * Adds the head tags every page should carry but the hand-written pages do not
 * repeat: robots, author and `rel=me` identity links, Open Graph site/locale/image
 * size and alt, Twitter title/description/image (derived from the page's own
 * title, description and og:image), the llms.txt pointer, and a BreadcrumbList.
 * Idempotent: a tag a page already declares is never doubled. Redirect stubs
 * (noindex) are left alone.
 */
export function seo(html, urlPath) {
  if (/name="robots" content="noindex"/.test(html)) return html;
  const title = attr(html, /<title>([^<]*)<\/title>/);
  const desc = attr(html, /<meta name="description" content="([^"]*)"/);
  const image = attr(html, /<meta property="og:image" content="([^"]*)"/);
  const add = [];
  const has = (needle) => html.includes(needle);
  if (!has('name="robots"'))
    add.push(
      '<meta name="robots" content="index, follow, max-image-preview:large, max-snippet:-1">',
    );
  if (!has('name="author"')) add.push('<meta name="author" content="Jubair Hasan (DarthDemono)">');
  if (!has('rel="me"'))
    for (const href of AUTHOR.sameAs) add.push(`<link rel="me" href="${href}">`);
  if (!has('og:site_name')) add.push('<meta property="og:site_name" content="UnENVerse">');
  if (!has('og:locale')) add.push('<meta property="og:locale" content="en_US">');
  if (image && !has('og:image:width'))
    add.push(
      '<meta property="og:image:width" content="1200">',
      '<meta property="og:image:height" content="630">',
      `<meta property="og:image:alt" content="${escAttr(title ?? 'UnENVerse')}">`,
    );
  if (title && !has('twitter:title')) add.push(`<meta name="twitter:title" content="${title}">`);
  if (desc && !has('twitter:description'))
    add.push(`<meta name="twitter:description" content="${desc}">`);
  if (image && !has('twitter:image')) add.push(`<meta name="twitter:image" content="${image}">`);
  if (!has('/llms.txt'))
    add.push(
      '<link rel="alternate" type="text/plain" href="/llms.txt" title="UnENVerse for language models">',
    );
  if (!has('rel="canonical"')) add.push(`<link rel="canonical" href="${SITE_URL}${urlPath}">`);
  if (!has('/changelog/feed.xml'))
    add.push(
      '<link rel="alternate" type="application/atom+xml" href="/changelog/feed.xml" title="UnENVerse changelog">',
    );
  // One structured-data node per page that says what the page is and ties it to the site and its
  // author, so crawlers and answer engines do not have to infer either from the markup.
  if (
    title &&
    !has('"@type":"WebPage"') &&
    !has('"@type":"FAQPage"') &&
    !has('"@type":"TechArticle"')
  ) {
    const type =
      urlPath.startsWith('/changelog/') && urlPath !== '/changelog/' ? 'TechArticle' : 'WebPage';
    const page = {
      '@context': 'https://schema.org',
      '@type': type,
      name: title,
      ...(desc ? { description: desc } : {}),
      url: `${SITE_URL}${urlPath}`,
      inLanguage: 'en',
      isPartOf: { '@type': 'WebSite', name: 'UnENVerse', url: `${SITE_URL}/` },
      author: AUTHOR,
      publisher: AUTHOR,
      about: { '@type': 'SoftwareApplication', name: 'UnENVerse', url: `${SITE_URL}/` },
    };
    add.push(
      `<script type="application/ld+json">${JSON.stringify(page).replace(/</g, '\\u003c')}</script>`,
    );
  }
  if (urlPath === '/' && !has('"@type":"WebSite"')) {
    const site = {
      '@context': 'https://schema.org',
      '@type': 'WebSite',
      name: 'UnENVerse',
      alternateName: ['UnENVerse secrets manager', 'unv'],
      url: `${SITE_URL}/`,
      inLanguage: 'en',
      publisher: AUTHOR,
    };
    add.push(
      `<script type="application/ld+json">${JSON.stringify(site).replace(/</g, '\\u003c')}</script>`,
    );
  }
  if (urlPath !== '/' && title && !has('BreadcrumbList')) {
    const trail = [['Home', '/']];
    let acc = '';
    for (const seg of urlPath.split('/').filter(Boolean)) {
      acc += `/${seg}`;
      trail.push([
        seg === urlPath.split('/').filter(Boolean).at(-1)
          ? title.split(':')[0]
          : seg.charAt(0).toUpperCase() + seg.slice(1),
        `${acc}/`,
      ]);
    }
    const doc = {
      '@context': 'https://schema.org',
      '@type': 'BreadcrumbList',
      itemListElement: trail.map(([name, p], i) => ({
        '@type': 'ListItem',
        position: i + 1,
        name,
        item: `${SITE_URL}${p}`,
      })),
    };
    add.push(
      `<script type="application/ld+json">${JSON.stringify(doc).replace(/</g, '\\u003c')}</script>`,
    );
  }
  let out = add.length ? html.replace('</head>', `  ${add.join('\n  ')}\n</head>`) : html;
  // Microdata root, for consumers that read itemprop rather than JSON-LD.
  if (!/<html[^>]*itemscope/.test(out))
    out = out.replace(
      /<html lang="en">/,
      '<html lang="en" itemscope itemtype="https://schema.org/WebPage">',
    );
  if (title && !out.includes('itemprop="name"'))
    out = out.replace('</head>', `  <meta itemprop="name" content="${escAttr(title)}">\n</head>`);
  if (desc && !out.includes('itemprop="description"') && !/<meta itemprop="description"/.test(out))
    out = out.replace(
      '</head>',
      `  <meta itemprop="description" content="${escAttr(desc)}">\n</head>`,
    );
  return out;
}

const NAV = [
  'home',
  'features',
  'download',
  'quickstart',
  'comparison',
  'support',
  'help',
  'install',
  'administration',
  'developer',
  'roadmap',
  'changelog',
  'faq',
  'architecture',
  'guides',
  'security',
  'importing',
  'cli',
  'nodes',
];
/** Sections that also have an icon in the activity rail. */
const RAIL = ['home', 'download', 'guides', 'security'];

/**
 * Fills one page. `page` is the section key ("download", "home" for the front
 * page); `sub` is the child page inside it ("cli"), which then owns the sidebar
 * highlight while the section keeps its rail icon.
 */
export function fill(html, vars, partials, page, sub = '') {
  let out = html.replace('{{HEADER}}', partials.header).replace('{{FOOTER}}', partials.footer);
  const cur = (n) => (sub ? n === sub : n === page);
  for (const n of NAV) out = out.replaceAll(`{{CUR_${n}}}`, cur(n) ? ' aria-current="page"' : '');
  for (const n of RAIL)
    out = out.replaceAll(`{{RAIL_${n}}}`, n === page ? ' aria-current="page"' : '');
  for (const [k, v] of Object.entries(vars)) out = out.replaceAll(`{{${k}}}`, v);
  const left = out.match(/\{\{[A-Za-z_-]+\}\}/g);
  if (left) throw new Error(`unfilled placeholders: ${[...new Set(left)].join(', ')}`);
  return out;
}

function walk(dir) {
  return fs.readdirSync(dir, { withFileTypes: true }).flatMap((e) => {
    const p = path.join(dir, e.name);
    return e.isDirectory() ? walk(p) : [p];
  });
}

export function sitemap(urls, date) {
  const items = urls
    .map((u) => `  <url><loc>${SITE_URL}${u}</loc><lastmod>${date}</lastmod></url>`)
    .join('\n');
  return `<?xml version="1.0" encoding="UTF-8"?>\n<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">\n${items}\n</urlset>\n`;
}

/**
 * Writes /changelog/ (index, one page per section of CHANGELOG.md) and its Atom feed.
 * @returns the page URLs for the sitemap.
 */
export function changelogSite({ out, partials, vars, today, md }) {
  const text = md ?? fs.readFileSync(path.join(ROOT, 'CHANGELOG.md'), 'utf8');
  const entries = parseChangelog(text);
  const urls = ['/changelog/'];
  const meta = (e) => escHtml(changelogSummary(e, 110));
  const write = (rel, html, url) => {
    const dest = path.join(out, rel);
    fs.mkdirSync(path.dirname(dest), { recursive: true });
    fs.writeFileSync(dest, seo(html, url));
  };
  const tpl = (n) => fs.readFileSync(path.join(ROOT, 'website/partials', n), 'utf8');
  const first = entries.find((e) => e.slug !== 'unreleased') ?? entries[0];
  const list = entries
    .map(
      (e) =>
        `        <li><a href="/changelog/${e.slug}/"><strong>${escHtml(e.title)}</strong></a>${e.date ? ` <time datetime="${e.date}">${e.date}</time>` : ''}<br>${escHtml(changelogSummary(e, 160))}</li>`,
    )
    .join('\n');
  write(
    'changelog/index.html',
    fill(
      tpl('changelog-index.html'),
      { ...vars, CL_LIST: list, CL_LATEST: escHtml(first.title) },
      partials,
      'changelog',
    ),
    '/changelog/',
  );
  entries.forEach((e, i) => {
    const newer = entries[i - 1];
    const older = entries[i + 1];
    const nav = [
      newer
        ? `<a href="/changelog/${newer.slug}/" rel="prev">Newer: ${escHtml(newer.title)}</a>`
        : '',
      older
        ? `<a href="/changelog/${older.slug}/" rel="next">Older: ${escHtml(older.title)}</a>`
        : '',
    ]
      .filter(Boolean)
      .join(' · ');
    write(
      `changelog/${e.slug}/index.html`,
      fill(
        tpl('changelog-entry.html'),
        {
          ...vars,
          CL_TITLE: escHtml(e.title),
          CL_SLUG: e.slug,
          CL_META: meta(e),
          CL_DESC: escHtml(changelogSummary(e, 300)),
          CL_BODY: renderMarkdown(e.body),
          CL_NAV: nav,
        },
        partials,
        'changelog',
      ),
      `/changelog/${e.slug}/`,
    );
    urls.push(`/changelog/${e.slug}/`);
  });
  fs.writeFileSync(path.join(out, 'changelog/feed.xml'), atomFeed(entries, SITE_URL, today));
  return urls;
}

/**
 * Renders /faq/ from website/partials/faq.json, so the visible answers and the FAQPage
 * structured data are the same strings and cannot drift apart.
 * @returns the page URLs for the sitemap.
 */
export function faqSite({ out, partials, vars }) {
  const items = JSON.parse(fs.readFileSync(path.join(ROOT, 'website/partials/faq.json'), 'utf8'));
  const body = items
    .map(
      ({ q, a }) =>
        `      <div itemscope itemprop="mainEntity" itemtype="https://schema.org/Question">\n        <h2 itemprop="name">${esc(q)}</h2>\n        <div itemscope itemprop="acceptedAnswer" itemtype="https://schema.org/Answer"><p itemprop="text">${esc(a)}</p></div>\n      </div>`,
    )
    .join('\n');
  const ld = JSON.stringify({
    '@context': 'https://schema.org',
    '@type': 'FAQPage',
    mainEntity: items.map(({ q, a }) => ({
      '@type': 'Question',
      name: q,
      acceptedAnswer: { '@type': 'Answer', text: a },
    })),
  }).replace(/</g, '\\u003c');
  const tpl = fs.readFileSync(path.join(ROOT, 'website/partials/faq-page.html'), 'utf8');
  const html = seo(fill(tpl, { ...vars, FAQ_BODY: body, FAQ_LD: ld }, partials, 'faq'), '/faq/');
  fs.mkdirSync(path.join(out, 'faq'), { recursive: true });
  fs.writeFileSync(path.join(out, 'faq/index.html'), html);
  return ['/faq/'];
}

/**
 * Visible text of a page's <main>, as plain text with blank lines between blocks.
 *
 * A single pass over the characters, not a chain of regular expressions: the input is the site's
 * own generated pages, and a scanner that walks `<` to `>` cannot be fooled by a nested or
 * unterminated tag the way a pattern can.
 */
export function mainText(html) {
  const start = html.indexOf('<main');
  const from = start < 0 ? 0 : html.indexOf('>', start) + 1;
  const end = html.lastIndexOf('</main>');
  const body = html.slice(from, end < 0 ? html.length : end);
  const lower = body.toLowerCase();
  const SKIP = [
    ['<script', '</script>'],
    ['<style', '</style>'],
    ['<svg', '</svg>'],
  ];
  let out = '';
  let i = 0;
  while (i < body.length) {
    const lt = body.indexOf('<', i);
    if (lt < 0) {
      out += body.slice(i);
      break;
    }
    out += body.slice(i, lt);
    const skip = SKIP.find(([open]) => lower.startsWith(open, lt));
    if (skip) {
      const close = lower.indexOf(skip[1], lt);
      i = close < 0 ? body.length : close + skip[1].length;
      continue;
    }
    const gt = body.indexOf('>', lt);
    const tag = body
      .slice(lt + 1, gt < 0 ? body.length : gt)
      .trim()
      .toLowerCase();
    i = gt < 0 ? body.length : gt + 1;
    const closing = tag.startsWith('/');
    const name = (closing ? tag.slice(1) : tag).split(/[\s/]/)[0];
    if (!closing && /^h[1-6]$/.test(name)) out += `\n\n${'#'.repeat(Number(name[1]))} `;
    else if (!closing && name === 'li') out += '\n- ';
    else if (name === 'br') out += '\n';
    else if (closing && ['p', 'div', 'ul', 'ol', 'table', 'tr', 'pre', 'figure'].includes(name))
      out += '\n';
  }
  const ENTITIES = { lt: '<', gt: '>', quot: '"', '#39': "'", amp: '&' };
  return out
    .replace(/&(lt|gt|quot|#39|amp);/g, (_m, k) => ENTITIES[k])
    .replace(/[ \t]+\n/g, '\n')
    .replace(/\n[ \t]+/g, '\n')
    .replace(/\n{3,}/g, '\n\n')
    .trim();
}

/**
 * Writes /llms.txt (the curated index) and /llms-full.txt (the same index followed by the
 * text of the FAQ, features, architecture and the latest release notes), so a model can read
 * one document instead of crawling. Both are built from the pages, never hand-copied.
 */
export function llmsSite({ out, vars, md }) {
  const text = (name) => fs.readFileSync(path.join(ROOT, 'website/partials', name), 'utf8');
  const fillTxt = (t) =>
    t.replaceAll('{{VERSION}}', vars.VERSION).replaceAll('{{SITE_URL}}', SITE_URL);
  const entries = parseChangelog(md ?? fs.readFileSync(path.join(ROOT, 'CHANGELOG.md'), 'utf8'));
  const recent = entries.filter((e) => e.slug !== 'unreleased');
  const notes = recent
    .slice(0, 8)
    .map((e) => `- [${e.title}](${SITE_URL}/changelog/${e.slug}/): ${changelogSummary(e, 140)}`)
    .join('\n');
  const index = `${fillTxt(text('llms.txt')).trimEnd()}\n\n## Recent releases\n\n${notes}\n`;
  fs.writeFileSync(path.join(out, 'llms.txt'), index);
  const page = (rel) => mainText(fs.readFileSync(path.join(out, rel, 'index.html'), 'utf8'));
  const latest = recent
    .slice(0, 3)
    .map((e) => `## Release ${e.title}\n\n${e.body}`)
    .join('\n\n');
  fs.writeFileSync(
    path.join(out, 'llms-full.txt'),
    `${index}\n---\n\n# Full text\n\n${['faq', 'features', 'architecture'].map(page).join('\n\n---\n\n')}\n\n---\n\n# Latest release notes\n\n${latest}\n`,
  );
}

export function build({ src, out, release, sums, today = new Date().toISOString().slice(0, 10) }) {
  const version = release.tag_name.replace(/^v/, '');
  const parts = classify(release.assets);
  const shots = JSON.parse(fs.readFileSync(path.join(src, 'assets/screens/manifest.json'), 'utf8'));
  const vars = {
    VERSION: version,
    RELEASE_DATE: release.published_at.slice(0, 10),
    RELEASE_URL: release.html_url,
    SITE_URL,
    JSONLD: jsonLd(version),
    DESKTOP_TABLE: table(parts.desktop, sums),
    CLI_TABLE: table(parts.cli, sums),
  };
  for (const [name, d] of Object.entries(shots)) {
    vars[`W_${name}`] = String(d.width);
    vars[`H_${name}`] = String(d.height);
  }
  const partials = {
    header: fs.readFileSync(path.join(src, 'partials/header.html'), 'utf8'),
    footer: fs.readFileSync(path.join(src, 'partials/footer.html'), 'utf8'),
  };

  fs.rmSync(out, { recursive: true, force: true });
  const urls = [];
  for (const file of walk(src)) {
    const rel = path.relative(src, file);
    if (rel.startsWith('partials')) continue;
    const dest = path.join(out, rel);
    fs.mkdirSync(path.dirname(dest), { recursive: true });
    if (rel.endsWith('.html')) {
      const parts = rel.split(path.sep);
      const page = rel === 'index.html' ? 'home' : parts[0];
      const sub = parts.length > 2 ? parts[1] : '';
      const url = rel === 'index.html' ? '/' : `/${path.dirname(rel).split(path.sep).join('/')}/`;
      const html = seo(fill(fs.readFileSync(file, 'utf8'), vars, partials, page, sub), url);
      fs.writeFileSync(dest, html);
      // Redirect stubs say noindex; they are not pages to list.
      if (!/name="robots" content="noindex"/.test(html)) {
        urls.push(rel === 'index.html' ? '/' : `/${path.dirname(rel).split(path.sep).join('/')}/`);
      }
    } else {
      fs.copyFileSync(file, dest);
    }
  }
  for (const asset of GENERATED_SITE_ASSETS) {
    const dest = path.join(out, asset);
    fs.mkdirSync(path.dirname(dest), { recursive: true });
    fs.copyFileSync(APP_ICON, dest);
  }
  urls.push(...changelogSite({ out, partials, vars, today }), ...faqSite({ out, partials, vars }));
  llmsSite({ out, vars });
  fs.writeFileSync(path.join(out, 'sitemap.xml'), sitemap(urls.sort(), today));
  return { version, pages: urls };
}

async function latestRelease() {
  const headers = { Accept: 'application/vnd.github+json', 'User-Agent': 'unenverse-site-build' };
  if (process.env.GITHUB_TOKEN) headers.Authorization = `Bearer ${process.env.GITHUB_TOKEN}`;
  // `releases/latest` answers 404 for a minute or so after a release is published, and the docs
  // workflow starts on the same push that starts the release build, so retry before failing.
  let r;
  for (let attempt = 1; attempt <= 5; attempt += 1) {
    r = await fetch(`https://api.github.com/repos/${REPO}/releases/latest`, { headers });
    if (r.ok || (r.status !== 404 && r.status < 500) || attempt === 5) break;
    await new Promise((resolve) => setTimeout(resolve, 30_000));
  }
  if (!r.ok) throw new Error(`GitHub release lookup failed: HTTP ${r.status}`);
  const release = await r.json();
  const sumsAsset = release.assets.find((a) => a.name === 'SHA256SUMS');
  if (!sumsAsset) throw new Error('the latest release has no SHA256SUMS');
  const s = await fetch(sumsAsset.browser_download_url);
  if (!s.ok) throw new Error(`SHA256SUMS download failed: HTTP ${s.status}`);
  return { release, sumsText: await s.text() };
}

async function main() {
  const arg = (n) => {
    const i = process.argv.indexOf(n);
    return i > 0 ? process.argv[i + 1] : undefined;
  };
  const out = path.resolve(arg('--out') ?? 'site');
  let release;
  let sumsText;
  if (arg('--release')) {
    release = JSON.parse(fs.readFileSync(arg('--release'), 'utf8'));
    sumsText = fs.readFileSync(arg('--sums'), 'utf8');
  } else {
    ({ release, sumsText } = await latestRelease());
  }
  const { version, pages } = build({
    src: path.join(ROOT, 'website'),
    out,
    release,
    sums: parseSums(sumsText),
  });
  console.log(`site ${version}: ${pages.length} pages -> ${out}`);
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main().catch((e) => {
    console.error(e.message);
    process.exit(1);
  });
}
