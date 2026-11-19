// Checks another session's message in a session's conversation (MessageCard.tsx `PeerCard`): a
// card, as Codealong's own messages are, whose line says whose it is, apart from yours and the
// agent's, and it's there whole. A long one shows its first lines, fading out, with the card's
// button to open it, which opens it (and closes it again); ⌘F finding a word in its cut-off part
// opens it while the bar is open, closed or not. A file and an image it was sent with show as in a
// prompt of yours. A short one shows whole with nothing to open, and the agent's reply after it
// heads itself again.
//   scripts/preview.sh sessions '' scripts/preview/peer-message-check.js
// As PREVIEW_INIT, after a line setting `window.__peer_screenshot = true`, it leaves the long
// message cut and in view for a screenshot.
(function () {
  localStorage.setItem('codealong.project', 'All projects');
  localStorage.setItem('codealong.view', 'sessions');
  // The first session's stand-in conversation goes on with two messages from other sessions.
  var picked = null;
  var points = [];
  for (var i = 1; i <= 24; i++) points.push('- Finding ' + i + ': the reader at `apps/server/src/issue/IssueTools.ts` sends this sheet to Astra.');
  var long = 'Wes asks: is there a rule that sends about **10%** or less of the reads to Astra? The log is [run.log](files/0123456789abcdef/run.log):\n\n![image](images/0123456789abcdef.png)\n\n' + points.join('\n') + '\n\nThe last line says zeppelin and `__init__.py`, which only the whole message shows.';
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    var p = real(cmd, args, options);
    if (cmd === 'session_page' && picked && (args || {}).key === picked.key && (args || {}).before == null) {
      return p.then(function (page) {
        var at = Date.now() - 30e3;
        var item = function (kind, text, n, extra) { return Object.assign({ at: at + n, kind: kind, text: text, failed: false, cursor: 10 + n, n: 0 }, extra || {}); };
        page.items = page.items.concat([
          item('peer', long, 0, { from: 'Review 1021' }),
          item('peer', 'Thanks, that answers it.', 1),
          item('agent', 'Both read. Nothing to change on my side.', 2),
        ]);
        page.after = 20;
        return page;
      });
    }
    if (cmd !== 'get_board') return p;
    return p.then(function (board) {
      board.lanes.forEach(function (l) {
        l.sessions.forEach(function (s) {
          if (!picked && !l.hidden && !l.device && !s.visitor && !s.archived) picked = { key: s.key, lane: l.id };
        });
      });
      return board;
    });
  };

  var row = function () { return picked && document.getElementById('row-' + picked.lane + '::' + picked.key); };
  var peers = function () { return Array.prototype.slice.call(document.querySelectorAll('.ca-sessinsp .ca-msgcard')).filter(function (c) { return !!c.querySelector('.ca-thread-said.is-peer'); }); };
  var heading = function (el) { var m = el && el.querySelector('.ca-msgcard-title'); return m ? m.textContent : null; };
  var text = function (el) { return el.querySelector('.ca-msgcard-body'); };
  var toggle = function (el) { return el.querySelector('.ca-msgcard-open'); };
  var cut = function (el) { return text(el).classList.contains('is-cut'); };
  var opened = function (el) { return toggle(el).getAttribute('aria-expanded') === 'true'; };
  // ⌘ on a Mac is Ctrl on Linux and Windows (platform.ts `held`).
  var linux = window.__CODEALONG_PLATFORM__ === 'linux' || window.__CODEALONG_PLATFORM__ === 'windows' || /\bLinux\b/.test(navigator.userAgent);
  var field = function () { return document.querySelector('.ca-findbar input'); };
  var type = function (input, value) {
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set.call(input, value);
    input.dispatchEvent(new Event('input', { bubbles: true }));
  };

  say('peer message check: waiting for the list');
  until(function () { return row(); }, 20000)
    .then(function () {
      row().dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
      row().click();
      return until(function () { return peers().length === 2; }, 5000);
    })
    .then(function () {
      var both = peers();
      var a = both[0];
      var b = both[1];
      check('says whom the long one is from', heading(a) === 'From “Review 1021”', heading(a));
      check('and the one whose record names no session', heading(b) === 'From another session', heading(b));
      check('the whole message is there, past its first line', text(a).textContent.indexOf('zeppelin') >= 0 && text(a).querySelectorAll('li').length === 24, text(a).textContent.slice(-80));
      check('the files it was sent with show as in a prompt of yours', !!text(a).querySelector('.ca-filechip') && !!text(a).querySelector('.ca-desc-img, .ca-desc-noimg') && text(a).textContent.indexOf('![') < 0, text(a).innerHTML.slice(0, 400));
      var t = text(a);
      check('the long one is cut, fading out', cut(a) && t.classList.contains('is-long') && t.clientHeight < t.scrollHeight && getComputedStyle(t).maskImage !== 'none', t.clientHeight + ' of ' + t.scrollHeight);
      check('with the card’s button to open it', !!toggle(a) && !opened(a) && toggle(a).getAttribute('aria-label') === 'Show the whole message', toggle(a) && toggle(a).outerHTML);
      check('under a line like Codealong’s own messages’, its icon first', !!a.querySelector('.ca-msgcard-head > .ca-msgcard-icon') && a.querySelector('.ca-msgcard-head > .ca-meta') !== null);
      check('the short one shows whole, with nothing to open', !toggle(b) && !text(b).classList.contains('is-long') && text(b).clientHeight === text(b).scrollHeight);
      var reply = b.nextElementSibling;
      check('the reply after them heads itself', !!reply && reply.classList.contains('is-agent') && !!reply.querySelector(':scope > .ca-thread-meta'), reply && reply.className);
      // A right-click on its line is one on the message.
      return menuOf(a.querySelector('.ca-msgcard-head > .ca-meta')).then(function (labels) {
        check('a right-click on its line offers Copy message', labels.indexOf('Copy message') >= 0, labels.join(' | '));
        toggle(a).click();
        return until(function () { return !cut(a); }, 2000).then(function () {
          check('its button opens it', text(a).clientHeight === text(a).scrollHeight && !text(a).classList.contains('is-long'), text(a).clientHeight + ' of ' + text(a).scrollHeight);
          check('and offers Show less', opened(a) && toggle(a).getAttribute('aria-label') === 'Show less', toggle(a).outerHTML);
          // A click on its line closes it as the button does.
          a.querySelector('.ca-msgcard-head').click();
          return until(function () { return cut(a); }, 2000);
        });
      });
    })
    .then(function () {
      check('a click on its line cuts it again', cut(peers()[0]) && !opened(peers()[0]));
      if (window.__peer_screenshot) {
        var thread = document.querySelector('.ca-sessinsp .ca-thread');
        thread.scrollTop = Math.max(0, peers()[0].offsetTop - 120);
        return 'stay';
      }
      var box = document.querySelector('.ca-sessinsp .ca-composer textarea');
      if (!box) throw new Error('no box to press ⌘F from');
      box.focus();
      box.dispatchEvent(new KeyboardEvent('keydown', { key: 'f', metaKey: !linux, ctrlKey: linux, bubbles: true, cancelable: true }));
      return until(function () { return field(); }, 2000)
        .then(function () {
          // Cut again, it opens all the same: the find counts what the cut would hide,
          // as it's laid out (Markdown made plain would drop the underscores).
          type(field(), '__init__.py');
          return until(function () { return !cut(peers()[0]); }, 2000);
        })
        .then(function () {
          check('⌘F finding a word in its cut-off part opens it', !cut(peers()[0]) && toggle(peers()[0]).disabled);
          field().dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true }));
          return until(function () { return !field(); }, 2000);
        })
        .then(function () {
          check('and closing the find cuts it again', cut(peers()[0]) && !opened(peers()[0]));
        });
    })
    .then(function (stay) {
      if (stay) return;
      done();
    })
    .catch(stopped);
})();
