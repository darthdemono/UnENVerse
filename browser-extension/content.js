// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
/**
 * Catches a paste into an AI chat before the page sees it, asks the local host whether the text
 * holds a value from the vault, and either lets it through, offers to rewrite it, or lets the
 * person cancel. The page only ever receives text the person chose to paste. If the host cannot be
 * reached the paste goes through with a notice, because blocking every paste while a program is
 * not running would soon be turned off.
 */
(function (root) {
  'use strict';

  var core = root.UnvGuardCore;
  var TIMEOUT_MS = 8000;

  function el(doc, tag, text, style) {
    var node = doc.createElement(tag);
    if (text) node.textContent = text;
    if (style) node.setAttribute('style', style);
    return node;
  }

  /** Puts `text` into `target` as if it had been pasted. */
  function insert(doc, target, text) {
    try {
      if (target && target.focus) target.focus();
    } catch (e) {
      /* a detached element is fine */
    }
    var done = false;
    try {
      done = doc.execCommand('insertText', false, text);
    } catch (e) {
      done = false;
    }
    if (!done && target && typeof target.setRangeText === 'function') {
      target.setRangeText(text, target.selectionStart || 0, target.selectionEnd || 0, 'end');
      target.dispatchEvent(new Event('input', { bubbles: true }));
    }
  }

  function box(doc) {
    var host = el(
      doc,
      'div',
      '',
      'all:initial;position:fixed;right:16px;bottom:16px;z-index:2147483647',
    );
    var shadow = host.attachShadow({ mode: 'closed' });
    var card = el(
      doc,
      'div',
      '',
      'font:14px system-ui,sans-serif;background:#1b1f23;color:#f5f5f5;border:1px solid #8a8f98;border-radius:8px;padding:12px 14px;max-width:360px;box-shadow:0 4px 18px rgba(0,0,0,.4)',
    );
    card.setAttribute('role', 'alertdialog');
    shadow.appendChild(card);
    (doc.documentElement || doc).appendChild(host);
    return { host: host, card: card };
  }

  function button(doc, label, onClick) {
    var b = el(
      doc,
      'button',
      label,
      'margin:8px 6px 0 0;padding:5px 10px;border-radius:6px;border:1px solid #8a8f98;background:#2d333b;color:#f5f5f5;cursor:pointer',
    );
    b.setAttribute('type', 'button');
    b.addEventListener('click', onClick);
    return b;
  }

  function notice(doc, text) {
    var b = box(doc);
    b.card.appendChild(el(doc, 'div', text));
    b.card.setAttribute('role', 'status');
    setTimeout(function () {
      b.host.remove();
    }, 5000);
  }

  function warn(doc, names, choices, hooks) {
    var b = box(doc);
    b.card.appendChild(
      el(doc, 'div', 'This paste holds a secret from your vault: ' + names.join(', ') + '.'),
    );
    function close(then) {
      return function () {
        b.host.remove();
        then();
      };
    }
    var cancel = button(doc, 'Cancel', close(choices.cancel));
    b.card.appendChild(cancel);
    b.card.appendChild(button(doc, 'Replace with a reference', close(choices.replace)));
    b.card.appendChild(button(doc, 'Paste anyway', close(choices.anyway)));
    try {
      cancel.focus();
    } catch (e) {
      /* not focusable until attached */
    }
    if (hooks && hooks.onWarn) hooks.onWarn({ names: names, choices: choices, host: b.host });
  }

  /**
   * Starts guarding `doc`. `send(text)` returns a promise of the host's answer. Returns a function
   * that stops the guard (used by the tests).
   */
  function install(doc, send, hooks) {
    function onPaste(e) {
      var data = e.clipboardData;
      if (!data) return;
      var text = data.getData('text/plain');
      if (!core.shouldCheck(text, data.files ? data.files.length : 0)) return;
      e.preventDefault();
      e.stopImmediatePropagation();
      var target = doc.activeElement;
      var settled = false;
      var timer = setTimeout(function () {
        finish({ ok: false, error: 'timeout' });
      }, TIMEOUT_MS);
      function finish(answer) {
        if (settled) return;
        settled = true;
        clearTimeout(timer);
        var d = core.decide(answer);
        if (d.action === 'paste') return insert(doc, target, text);
        if (d.action === 'offline') {
          insert(doc, target, text);
          return notice(
            doc,
            'UnENVerse paste guard could not check this paste (' + d.reason + ').',
          );
        }
        warn(
          doc,
          d.names,
          {
            cancel: function () {},
            anyway: function () {
              insert(doc, target, text);
            },
            replace: function () {
              insert(doc, target, core.rewrite(text, d.spans));
            },
          },
          hooks,
        );
      }
      Promise.resolve()
        .then(function () {
          return send(text);
        })
        .then(finish, function () {
          finish({ ok: false, error: 'offline' });
        });
    }
    doc.addEventListener('paste', onPaste, true);
    return function stop() {
      doc.removeEventListener('paste', onPaste, true);
    };
  }

  root.UnvGuardContent = { install: install };

  var api =
    typeof browser !== 'undefined' ? browser : typeof chrome !== 'undefined' ? chrome : null;
  if (api && api.runtime && api.runtime.sendMessage && typeof document !== 'undefined') {
    install(document, function (text) {
      return api.runtime.sendMessage({ type: 'unv-guard-check', text: text });
    });
  }
})(typeof globalThis !== 'undefined' ? globalThis : this);
