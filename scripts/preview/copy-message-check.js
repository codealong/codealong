// Checks that a session's conversation keeps what's around its messages quiet. Who said a
// message and when heads a run of them, not each one: your prompt, then the reply, whose later
// messages between its calls go on under that heading. Nothing sits under a message to copy it:
// a right-click on one gives Copy message, which copies it as it was written (a reply's markdown,
// formatting and all, and what you wrote) and marks it for a moment, with Copy first for what
// you'd selected in it. In a card (the waiting window) the session's own menu follows. With
// PREVIEW_TRANSCRIPT it opens that session's real conversation, whose replies have formatting to
// copy and runs of messages to head once; without, the first session's stand-in conversation.
//   scripts/preview.sh sessions '' scripts/preview/copy-message-check.js
//   scripts/preview.sh waiting '' scripts/preview/copy-message-check.js
//   PREVIEW_TRANSCRIPT=<key> scripts/preview.sh sessions '' scripts/preview/copy-message-check.js
(function () {
  // Every project's sessions are listed, so PREVIEW_TRANSCRIPT's is wherever it works.
  localStorage.setItem('codealong.project', 'All projects');
  // Headless Chrome may refuse the clipboard; the check wants what was asked of it.
  var copied = [];
  Object.defineProperty(navigator, 'clipboard', { configurable: true, value: { writeText: function (t) { copied.push(t); return Promise.resolve(); } } });
  // Read once the page has set its window's class.
  var card = function () { return document.documentElement.classList.contains('is-waiting'); };

  // What formatting shows in a reply, and how its markdown says it: a link as written or a bare URL.
  var FORMATS = [
    ['a', function (el, text) { return /\]\(/.test(text) || text.indexOf(el.querySelector('a').getAttribute('href')) >= 0; }],
    ['strong', function (el, text) { return /\*\*/.test(text); }],
    ['code', function (el, text) { return /`/.test(text); }],
  ];
  var marks = function (el) { return FORMATS.filter(function (m) { return el.querySelector(m[0]); }); };
  var saids = function () { return Array.prototype.slice.call(document.querySelectorAll('.ca-thread-said')); };
  // The message to copy: a reply with formatting, else the first of its kind.
  var said = function (kind) {
    var all = saids().filter(function (el) { return el.classList.contains('is-' + kind) && !el.classList.contains('is-ahead') && !el.querySelector('.ca-thread-ran'); });
    return all.filter(function (el) { return kind !== 'agent' || marks(el).length; })[0] || all[0] || null;
  };
  var headed = function (el) { return !!el.querySelector(':scope > .ca-thread-meta'); };
  var kindOf = function (el) { return el.classList.contains('is-you') ? 'you' : 'agent'; };
  var sessionItems = function (labels) { return labels.some(function (l) { return /^(Archive|Snooze|Rename|Show in Codealong)/.test(l); }); };

  say('copy message check: opening a conversation');
  fetch('./transcript-key.json')
    .then(function (r) { return r.ok ? r.json() : null; })
    .catch(function () { return null; })
    .then(function (fixed) {
      if (!fixed || card()) return null;
      return until(function () { return document.querySelector('[id$="::' + fixed.key + '"]'); }, 20000).then(function () {
        document.querySelector('[id$="::' + fixed.key + '"]').dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
      });
    })
    .then(function () { return until(function () { return said('agent') && said('you'); }, 20000, 'no conversation with a prompt and a reply'); })
    .then(function () { return wait(300); })
    .then(function () {
      // A code block in a message keeps its own Copy.
      var buttons = Array.prototype.slice.call(document.querySelectorAll('.ca-thread-said .ca-copybtn')).filter(function (b) { return !b.closest('.ca-codeblock'); });
      check('nothing sits under a message to copy it', buttons.length === 0, buttons.length + ' buttons');
      // Each message heads itself where the speaker changes; a reply's later messages don't.
      var all = saids().filter(function (el) { return !el.classList.contains('is-ahead'); });
      var wrong = [];
      var runs = 0;
      all.forEach(function (el, i) {
        var before = all[i - 1];
        var changed = !before || kindOf(before) !== kindOf(el);
        if (changed && !headed(el)) wrong.push(i + ' (' + kindOf(el) + ') has no heading after the other speaker');
        if (!changed && kindOf(el) === 'agent') runs++;
      });
      check('a message after the other speaker is headed', wrong.length === 0, wrong.join('; '));
      var unheaded = all.filter(function (el) { return !headed(el); }).length;
      if (runs > 0) check('a reply’s later messages go on under its heading', unheaded > 0, runs + ' later messages, ' + unheaded + ' without a heading');
      else note('no reply here has more than one message, so going on under a heading went unchecked');
      return menuOf(said('agent'));
    })
    .then(function (labels) {
      check('a reply’s right-click copies it', labels[0] === 'Copy message', labels.join(' | '));
      check('with nothing selected, no Copy', labels.indexOf('Copy') < 0, labels.join(' | '));
      if (card()) check('and the session’s own menu follows in a card', sessionItems(labels), labels.join(' | '));
      else check('and only that in a panel, whose header has the session’s menu', labels.length === 1, labels.join(' | '));
      menuItem('Copy message')();
      return wait(100);
    })
    .then(function () {
      var reply = said('agent');
      var text = copied[copied.length - 1] || '';
      check('a reply is copied', text.trim().length > 0, text.slice(0, 120));
      var found = marks(reply);
      if (found.length) check('as markdown, formatting and all', found.every(function (m) { return m[1](reply, text); }), text.slice(0, 160));
      else note('the reply has no formatting here, so markdown went unchecked');
      check('and is marked for a moment', reply.classList.contains('is-copied') && getComputedStyle(reply).animationName === 'ca-thread-copied', getComputedStyle(reply).animationName);
      // Some of what you wrote selected, as you'd select it before a right-click.
      var mine = said('you').querySelector('.ca-thread-you');
      var walker = document.createTreeWalker(mine, NodeFilter.SHOW_TEXT, { acceptNode: function (n) { return n.textContent.trim().length > 3 ? NodeFilter.FILTER_ACCEPT : NodeFilter.FILTER_SKIP; } });
      var words = walker.nextNode();
      var range = document.createRange();
      range.setStart(words, 0);
      range.setEnd(words, Math.min(words.textContent.length, 12));
      getSelection().removeAllRanges();
      getSelection().addRange(range);
      var picked = getSelection().toString();
      said('you').dispatchEvent(new MouseEvent('mousedown', { bubbles: true, button: 2 }));
      return menuOf(said('you')).then(function (labels) { return { labels: labels, picked: picked }; });
    })
    .then(function (r) {
      check('with text selected, Copy comes first', r.labels[0] === 'Copy' && r.labels[1] === 'Copy message', r.labels.join(' | '));
      menuItem('Copy')();
      return wait(100).then(function () {
        check('and copies what’s selected', copied[copied.length - 1] === r.picked, JSON.stringify(copied[copied.length - 1]) + ' for ' + JSON.stringify(r.picked));
        // A right-click that selected a word of its own (macOS does, outside a selection) copies
        // the message, and lets the word go.
        said('you').dispatchEvent(new MouseEvent('mousedown', { bubbles: true, button: 2 }));
        var words = said('you').querySelector('.ca-thread-you');
        var range = document.createRange();
        range.selectNodeContents(words);
        getSelection().removeAllRanges();
        getSelection().addRange(range);
        return menuOf(said('you'));
      });
    })
    .then(function (labels) {
      check('a word the right-click selected gets no Copy', labels[0] === 'Copy message', labels.join(' | '));
      check('and is let go', getSelection().isCollapsed || getSelection().toString() === '');
      menuItem('Copy message')();
      return wait(100);
    })
    .then(function () {
      var mine = (said('you').querySelector('.ca-thread-you').textContent || '').trim();
      var text = (copied[copied.length - 1] || '').trim();
      check('what you wrote is copied', text.length > 0 && text.indexOf(mine.slice(0, 30)) >= 0, text.slice(0, 120));
    })
    .catch(stopped)
    .then(done);
})();
