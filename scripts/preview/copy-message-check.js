// Checks that each message in a session's conversation has a button under it that copies the
// message as it was written: a reply's markdown, formatting and all, and what you wrote; that it
// sits under a reply's left edge and under the right edge of what you wrote; and that it says it
// copied. With PREVIEW_TRANSCRIPT it opens that session's real conversation, whose replies have
// formatting to copy; without, the first session's stand-in conversation.
//   scripts/preview.sh sessions '' scripts/preview/copy-message-check.js
//   PREVIEW_TRANSCRIPT=<key> scripts/preview.sh sessions '' scripts/preview/copy-message-check.js
(function () {
  // Every project's sessions are listed, so PREVIEW_TRANSCRIPT's is wherever it works.
  localStorage.setItem('codealong.project', 'All projects');
  // Headless Chrome may refuse the clipboard; the check wants what was asked of it.
  var copied = [];
  Object.defineProperty(navigator, 'clipboard', { configurable: true, value: { writeText: function (t) { copied.push(t); return Promise.resolve(); } } });

  var FORMATS = [['a', /\]\(/], ['strong', /\*\*/], ['code', /`/]];
  var marks = function (el) { return FORMATS.filter(function (m) { return el.querySelector(m[0]); }); };
  // The reply to copy: the first with formatting, else the first.
  var said = function (kind) {
    var all = Array.prototype.slice.call(document.querySelectorAll('.ca-thread-said.is-' + kind));
    var shown = all.filter(function (el) { return el.querySelector('.ca-thread-copy'); });
    return shown.filter(function (el) { return kind !== 'agent' || marks(el).length; })[0] || shown[0] || null;
  };
  var button = function (kind) { var el = said(kind); return el && el.querySelector('.ca-thread-copy'); };
  var acts = function (kind) { return said(kind).querySelector('.ca-thread-acts'); };

  say('copy message check: opening a conversation');
  fetch('./transcript-key.json')
    .then(function (r) { return r.ok ? r.json() : null; })
    .catch(function () { return null; })
    .then(function (fixed) {
      if (!fixed) return null;
      return until(function () { return document.querySelector('[id$="::' + fixed.key + '"]'); }, 20000).then(function () {
        document.querySelector('[id$="::' + fixed.key + '"]').dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
      });
    })
    .then(function () { return until(function () { return button('agent') && button('you'); }, 20000); })
    .then(function () { return wait(300); })
    .then(function () {
      check('a reply has its copy button at its left', getComputedStyle(acts('agent')).justifyContent !== 'flex-end');
      check('what you wrote has its copy button at its right', getComputedStyle(acts('you')).justifyContent === 'flex-end');
      check('it says what it does', button('agent').getAttribute('aria-label') === 'Copy message', button('agent').getAttribute('aria-label'));
      button('agent').click();
      return wait(100);
    })
    .then(function () {
      var reply = said('agent');
      var text = copied[copied.length - 1] || '';
      check('a reply is copied', text.trim().length > 0, text.slice(0, 120));
      var found = marks(reply);
      if (found.length) check('as markdown, formatting and all', found.every(function (m) { return m[1].test(text); }), text.slice(0, 160));
      else note('the reply has no formatting here, so markdown went unchecked');
      check('and says it was copied', button('agent').classList.contains('is-copied'));
      button('you').click();
      return wait(100);
    })
    .then(function () {
      var mine = (said('you').querySelector('.ca-thread-you').textContent || '').trim();
      var text = (copied[copied.length - 1] || '').trim();
      check('what you wrote is copied', text.length > 0 && text.indexOf(mine.slice(0, 30)) >= 0, text.slice(0, 120));
    })
    .catch(function (e) { check('ran', false, String(e)); })
    .then(done);
})();
