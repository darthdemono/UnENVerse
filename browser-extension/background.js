// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
/**
 * Passes a page's paste to the local UnENVerse host and returns its answer. Nothing else: no
 * network request, no storage, no copy of the text kept after the answer arrives. The host
 * answers requests in the order they were sent, so replies are matched by position.
 */
(function (root) {
  'use strict';

  var HOST = 'io.unenverse.guard';
  var api = typeof browser !== 'undefined' ? browser : chrome;
  var port = null;
  var pending = [];

  function offline(why) {
    return { ok: false, error: 'offline', message: String(why) };
  }

  function connect() {
    port = api.runtime.connectNative(HOST);
    port.onMessage.addListener(function (message) {
      var waiting = pending.shift();
      if (waiting) waiting(message);
    });
    port.onDisconnect.addListener(function () {
      var why = (api.runtime.lastError && api.runtime.lastError.message) || 'the host closed';
      port = null;
      var queue = pending;
      pending = [];
      queue.forEach(function (waiting) {
        waiting(offline(why));
      });
    });
  }

  function ask(message) {
    return new Promise(function (resolve) {
      try {
        if (!port) connect();
        pending.push(resolve);
        port.postMessage(message);
      } catch (e) {
        pending.pop();
        port = null;
        resolve(offline(e));
      }
    });
  }

  api.runtime.onMessage.addListener(function (message, sender, sendResponse) {
    // Only this extension's own content script may ask.
    if (sender.id !== api.runtime.id) return false;
    if (!message || message.type !== 'unv-guard-check' || typeof message.text !== 'string')
      return false;
    ask({ op: 'check', text: message.text }).then(sendResponse);
    return true;
  });

  root.UnvGuardBackground = { ask: ask };
})(typeof globalThis !== 'undefined' ? globalThis : this);
