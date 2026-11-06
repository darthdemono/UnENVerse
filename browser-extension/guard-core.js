// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
/**
 * The page-independent half of the paste guard: what to do with the host's answer. No DOM, no
 * browser API, so the repository's own tests can run it. The extension never sees a secret value:
 * the host answers with entry names and positions, and the text it rewrites is the user's own paste.
 */
(function (root) {
  'use strict';

  /** Shortest paste worth sending to the host. Matches the host's own short-value rule. */
  var MIN_CHARS = 8;

  /** Only a text paste with no files is checked; anything else is left to the page. */
  function shouldCheck(text, fileCount) {
    return typeof text === 'string' && text.length >= MIN_CHARS && !fileCount;
  }

  /**
   * Replaces the host's spans in `text`, right to left so earlier offsets stay valid. A span with a
   * reference becomes that reference; one without becomes a visible marker. Overlapping or
   * out-of-range spans are ignored rather than trusted.
   */
  function rewrite(text, spans) {
    var list = (spans || [])
      .filter(function (s) {
        return (
          s &&
          Number.isInteger(s.start) &&
          Number.isInteger(s.end) &&
          s.start >= 0 &&
          s.end > s.start &&
          s.end <= text.length
        );
      })
      .sort(function (a, b) {
        return b.start - a.start;
      });
    var out = text;
    var limit = text.length + 1;
    list.forEach(function (s) {
      if (s.end > limit) return;
      out = out.slice(0, s.start) + (s.ref || '[secret removed]') + out.slice(s.end);
      limit = s.start;
    });
    return out;
  }

  /** The distinct entry names in an answer, in order. */
  function names(answer) {
    var seen = [];
    ((answer && answer.hits) || []).forEach(function (h) {
      if (!h.short && h.entry && seen.indexOf(h.entry) < 0) seen.push(h.entry);
    });
    return seen;
  }

  /**
   * What to do with an answer: `paste` (clean), `warn` (a secret was found), or `offline` (the host
   * could not answer, so the paste goes through with a notice instead of being blocked for good).
   */
  function decide(answer) {
    if (!answer || answer.ok !== true) {
      return {
        action: 'offline',
        reason: answer && answer.error ? String(answer.error) : 'no answer',
      };
    }
    var found = names(answer);
    if (!found.length) return { action: 'paste' };
    return { action: 'warn', names: found, spans: answer.spans || [] };
  }

  var api = {
    MIN_CHARS: MIN_CHARS,
    shouldCheck: shouldCheck,
    rewrite: rewrite,
    names: names,
    decide: decide,
  };
  root.UnvGuardCore = api;
  if (typeof module === 'object' && module.exports) module.exports = api;
})(typeof globalThis !== 'undefined' ? globalThis : this);
