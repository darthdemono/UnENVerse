/*
 * UnENVerse site search. Runs in the page against /search-index.json; nothing is sent anywhere.
 * `rank` is plain and has no DOM, so the tests can run it.
 */
(function (root, factory) {
  var api = factory();
  // Always a global (the page and the test read it); also a module where there is one.
  root.UnvSiteSearch = api;
  if (typeof module === 'object' && module.exports) module.exports = api;
  if (typeof document !== 'undefined') api.mount(document);
})(typeof globalThis !== 'undefined' ? globalThis : this, function () {
  'use strict';

  function words(s) {
    return String(s)
      .toLowerCase()
      .split(/[^\p{L}\p{N}]+/u)
      .filter(Boolean);
  }

  /** Entries matching every word of `query`, best first. Title beats headings beats description. */
  function rank(entries, query, limit) {
    var q = words(query);
    if (!q.length) return [];
    var whole = String(query).trim().toLowerCase();
    var out = [];
    entries.forEach(function (e) {
      var title = words(e.t);
      var tl = String(e.t).toLowerCase();
      var head = String(e.h || '').toLowerCase();
      var desc = String(e.d || '').toLowerCase();
      var score = 0;
      for (var i = 0; i < q.length; i++) {
        var w = q[i];
        var s = 0;
        if (title.indexOf(w) >= 0) s += 8;
        else if (
          title.some(function (t) {
            return t.indexOf(w) === 0;
          })
        )
          s += 5;
        else if (tl.indexOf(w) >= 0) s += 3;
        if (head.indexOf(w) >= 0) s += 3;
        if (desc.indexOf(w) >= 0) s += 2;
        if (!s) return;
        score += s;
      }
      if (tl === whole) score += 10;
      out.push({ e: e, score: score });
    });
    out.sort(function (a, b) {
      return b.score - a.score || String(a.e.t).localeCompare(String(b.e.t));
    });
    return out.slice(0, limit || 8).map(function (x) {
      return x.e;
    });
  }

  var KINDS = { guide: 'Guide', glossary: 'Glossary', faq: 'FAQ', page: 'Page', docs: 'Docs' };
  var cache = null;

  function load() {
    if (cache) return cache;
    cache = fetch('/search-index.json')
      .then(function (r) {
        if (!r.ok) throw new Error('no index');
        return r.json();
      })
      .catch(function () {
        cache = null;
        return [];
      });
    return cache;
  }

  function item(doc, e) {
    var a = doc.createElement('a');
    a.href = e.u;
    a.setAttribute('role', 'option');
    var kind = doc.createElement('span');
    kind.className = 'sr-kind';
    kind.textContent = KINDS[e.k] || 'Page';
    var title = doc.createElement('strong');
    title.textContent = e.t;
    a.appendChild(kind);
    a.appendChild(title);
    if (e.d) {
      var d = doc.createElement('span');
      d.className = 'sr-desc';
      d.textContent = e.d.length > 110 ? e.d.slice(0, 107) + '...' : e.d;
      a.appendChild(d);
    }
    return a;
  }

  function mount(doc) {
    var form = doc.querySelector('form.searchlike');
    if (form) mountBar(doc, form);
    var page = doc.getElementById('search-results');
    if (page) mountPage(doc, page);
  }

  function mountBar(doc, form) {
    var input = form.querySelector('input[type=search]');
    var list = form.querySelector('[role=listbox]');
    if (!input || !list) return;
    var active = -1;
    function close() {
      list.hidden = true;
      input.setAttribute('aria-expanded', 'false');
      input.removeAttribute('aria-activedescendant');
      active = -1;
    }
    function setActive(n) {
      var opts = list.querySelectorAll('[role=option]');
      opts.forEach(function (o, i) {
        o.setAttribute('aria-selected', String(i === n));
      });
      active = n;
      if (opts[n]) input.setAttribute('aria-activedescendant', opts[n].id);
    }
    function render(results, q) {
      list.textContent = '';
      results.forEach(function (e, i) {
        var a = item(doc, e);
        a.id = 'sr-' + i;
        list.appendChild(a);
      });
      var all = doc.createElement('a');
      all.href = '/search/?q=' + encodeURIComponent(q);
      all.className = 'sr-all';
      all.textContent = results.length ? 'See all results' : 'No match. Search the whole site';
      list.appendChild(all);
      list.hidden = false;
      input.setAttribute('aria-expanded', 'true');
      active = -1;
    }
    input.addEventListener('focus', load);
    input.addEventListener('input', function () {
      var q = input.value.trim();
      if (!q) return close();
      load().then(function (idx) {
        if (input.value.trim() === q) render(rank(idx, q, 6), q);
      });
    });
    input.addEventListener('keydown', function (ev) {
      var opts = list.querySelectorAll('[role=option]');
      if (ev.key === 'ArrowDown' && !list.hidden) {
        ev.preventDefault();
        setActive(Math.min(active + 1, opts.length - 1));
      } else if (ev.key === 'ArrowUp' && !list.hidden) {
        ev.preventDefault();
        setActive(Math.max(active - 1, 0));
      } else if (ev.key === 'Enter' && active >= 0 && opts[active]) {
        ev.preventDefault();
        window.location.href = opts[active].href;
      } else if (ev.key === 'Escape') {
        close();
      }
    });
    doc.addEventListener('click', function (ev) {
      if (!form.contains(ev.target)) close();
    });
    doc.addEventListener('keydown', function (ev) {
      var t = ev.target;
      var typing = t && (t.tagName === 'INPUT' || t.tagName === 'TEXTAREA' || t.isContentEditable);
      if (ev.key === '/' && !typing && !ev.ctrlKey && !ev.metaKey && !ev.altKey) {
        ev.preventDefault();
        input.focus();
      }
    });
  }

  function mountPage(doc, page) {
    var input = doc.getElementById('search-page-q');
    var q = new URLSearchParams(window.location.search).get('q') || '';
    if (input) input.value = q;
    var note = doc.getElementById('search-note');
    function show(query) {
      load().then(function (idx) {
        page.textContent = '';
        var results = rank(idx, query, 50);
        if (!query.trim()) {
          if (note) note.textContent = 'Type a word to search the guides, FAQ, glossary and pages.';
          return;
        }
        if (note)
          note.textContent = results.length
            ? results.length + ' result' + (results.length === 1 ? '' : 's') + ' for "' + query + '"'
            : 'Nothing found for "' + query + '". Try the guides, the FAQ or the glossary.';
        results.forEach(function (e) {
          var li = doc.createElement('li');
          li.appendChild(item(doc, e));
          page.appendChild(li);
        });
      });
    }
    if (input)
      input.addEventListener('input', function () {
        show(input.value);
      });
    show(q);
  }

  return { rank: rank, words: words, mount: mount };
});
