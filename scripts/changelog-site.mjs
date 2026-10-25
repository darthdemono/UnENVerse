// SPDX-License-Identifier: PolyForm-Strict-1.0.0
// Copyright Jubair Hasan.
/**
 * Turns CHANGELOG.md into the site's changelog pages and an Atom feed.
 *
 * One page per `## ` section (a version or, in the older notes, a date), an index that links
 * them, and `feed.xml`. The markdown it has to read is the changelog's own: headings,
 * bullets, numbered lists, tables, fenced code, paragraphs and inline code, bold and links.
 * Anything else is escaped text. Nothing here fetches anything.
 */

export const esc = (s) =>
  String(s).replace(
    /[&<>"]/g,
    (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' })[c],
  );

/** Inline markdown on already-untrusted text: code spans, bold, links. */
export function inline(text) {
  const parts = text.split(/(`[^`]+`)/);
  return parts
    .map((part) => {
      if (part.length > 1 && part.startsWith('`') && part.endsWith('`'))
        return `<code>${esc(part.slice(1, -1))}</code>`;
      let s = esc(part);
      s = s.replace(/\*\*([^*]+)\*\*/g, '<strong>$1</strong>');
      s = s.replace(/\[([^\]]+)\]\((https?:\/\/[^)\s]+|\/[^)\s]*)\)/g, '<a href="$2">$1</a>');
      return s;
    })
    .join('');
}

/** A small markdown subset to HTML. Heading levels are shifted by `shift`. */
export function renderMarkdown(md, shift = 1) {
  const out = [];
  const lines = md.replace(/\r\n/g, '\n').split('\n');
  let i = 0;
  const flushPara = (buf) => {
    if (buf.length) out.push(`<p>${inline(buf.join(' '))}</p>`);
    buf.length = 0;
  };
  const para = [];
  while (i < lines.length) {
    const line = lines[i];
    const fence = /^```(\w*)/.exec(line);
    if (fence) {
      flushPara(para);
      const code = [];
      i++;
      while (i < lines.length && !lines[i].startsWith('```')) code.push(lines[i++]);
      i++;
      out.push(`<pre><code>${esc(code.join('\n'))}</code></pre>`);
      continue;
    }
    const h = /^(#{2,6})\s+(.*)$/.exec(line);
    if (h) {
      flushPara(para);
      const level = Math.min(6, h[1].length + shift);
      out.push(`<h${level}>${inline(h[2])}</h${level}>`);
      i++;
      continue;
    }
    if (/^\s*\|.*\|\s*$/.test(line) && /^\s*\|[\s:|-]+\|\s*$/.test(lines[i + 1] ?? '')) {
      flushPara(para);
      const cells = (l) =>
        l
          .trim()
          .replace(/^\||\|$/g, '')
          .split('|')
          .map((c) => c.trim());
      const head = cells(line);
      i += 2;
      const rows = [];
      while (i < lines.length && /^\s*\|.*\|\s*$/.test(lines[i])) rows.push(cells(lines[i++]));
      out.push(
        `<div class="scroll"><table><thead><tr>${head.map((c) => `<th scope="col">${inline(c)}</th>`).join('')}</tr></thead><tbody>${rows
          .map((r) => `<tr>${r.map((c) => `<td>${inline(c)}</td>`).join('')}</tr>`)
          .join('')}</tbody></table></div>`,
      );
      continue;
    }
    const li = /^(\s*)([-*]|\d+\.)\s+(.*)$/.exec(line);
    if (li) {
      flushPara(para);
      const ordered = /\d/.test(li[2]);
      const items = [];
      while (i < lines.length) {
        const m = /^(\s*)([-*]|\d+\.)\s+(.*)$/.exec(lines[i]);
        if (m && m[1].length === li[1].length) {
          items.push([m[3]]);
          i++;
        } else if (m || (/^\s+\S/.test(lines[i]) && items.length)) {
          items[items.length - 1].push(lines[i].trim().replace(/^([-*]|\d+\.)\s+/, ''));
          i++;
        } else break;
      }
      const tag = ordered ? 'ol' : 'ul';
      out.push(
        `<${tag}>${items.map((it) => `<li>${inline(it.join(' '))}</li>`).join('')}</${tag}>`,
      );
      continue;
    }
    if (!line.trim()) {
      flushPara(para);
      i++;
      continue;
    }
    para.push(line.trim());
    i++;
  }
  flushPara(para);
  return out.join('\n');
}

/**
 * `## 0.57.0 (2026-10-21)`, `## Unreleased` or `## 2026-06-11` become entries newest first, in file order.
 * @returns {{title:string, slug:string, date:string, body:string}[]}
 */
export function parseChangelog(md) {
  const entries = [];
  let cur = null;
  for (const line of md.replace(/\r\n/g, '\n').split('\n')) {
    const h = /^##\s+(.+?)\s*$/.exec(line);
    if (h && !line.startsWith('###')) {
      cur = { title: h[1], body: [] };
      entries.push(cur);
    } else if (cur) cur.body.push(line);
  }
  const seen = new Set();
  return entries.map((e) => {
    const ver =
      /^v?(\d+\.\d+\.(?:\d+|x))(?:\s*\((\d{4}-\d{2}-\d{2})(?:\s+to\s+\d{4}-\d{2}-\d{2})?\))?/.exec(
        e.title,
      );
    const day = /^(\d{4}-\d{2}-\d{2})/.exec(e.title);
    const clean = (t) =>
      t
        .toLowerCase()
        .replace(/[^a-z0-9.]+/g, '-')
        .replace(/^-+|-+$/g, '');
    let slug = ver ? ver[1] : day && e.title.trim() === day[1] ? day[1] : clean(e.title);
    for (let n = 2; seen.has(slug); n++) slug = `${slug}-${n}`;
    seen.add(slug);
    return {
      title: e.title,
      slug,
      date: ver?.[2] ?? day?.[1] ?? '',
      body: e.body.join('\n').trim(),
    };
  });
}

/** First bullet or sentence of an entry, for list descriptions and the feed. */
export function summary(entry, max = 200) {
  const first = entry.body.split('\n').find((l) => /^\s*([-*]|\d+\.)\s+/.test(l)) ?? entry.body;
  const text = first
    .replace(/^\s*([-*]|\d+\.)\s+/, '')
    .replace(/[`*]/g, '')
    .replace(/\[([^\]]+)\]\([^)]*\)/g, '$1')
    .trim();
  return text.length > max ? `${text.slice(0, max - 1).trimEnd()}…` : text;
}

export function atomFeed(entries, siteUrl, updated) {
  const items = entries
    .filter((e) => e.slug !== 'unreleased')
    .slice(0, 30)
    .map((e) => {
      const when = `${e.date || updated}T00:00:00Z`;
      return `  <entry>
    <title>UnENVerse ${esc(e.title)}</title>
    <id>${siteUrl}/changelog/${e.slug}/</id>
    <link href="${siteUrl}/changelog/${e.slug}/"/>
    <updated>${when}</updated>
    <summary>${esc(summary(e, 400))}</summary>
  </entry>`;
    })
    .join('\n');
  return `<?xml version="1.0" encoding="UTF-8"?>
<feed xmlns="http://www.w3.org/2005/Atom">
  <title>UnENVerse changelog</title>
  <id>${siteUrl}/changelog/</id>
  <link href="${siteUrl}/changelog/feed.xml" rel="self"/>
  <link href="${siteUrl}/changelog/"/>
  <updated>${updated}T00:00:00Z</updated>
  <author><name>Jubair Hasan</name></author>
${items}
</feed>
`;
}
