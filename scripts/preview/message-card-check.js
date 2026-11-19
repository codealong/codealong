// Checks Codealong's own messages in a session's conversation (MessageCard.tsx `CodealongCard`): each is a line,
// what it's about as its icon, the PR it names as a link and what happened in a few words a part
// (glance.rs), each helper it names a link to its session, with none of the whole message's words
// for the agent showing; a click on the line or its button opens the whole message under it, and
// another closes it; a click on its link goes where the link does and leaves it closed. ⌘F
// finding a word only the whole message has opens it while the bar is open. One without a line
// from the engine (an older one's) is a card too, headed Codealong, its words under it.
//   scripts/preview.sh sessions '' scripts/preview/message-card-check.js
// As PREVIEW_INIT, after a line setting `window.__card_screenshot = true`, it leaves the cards as
// they are, the PR's open, for a screenshot.
(function () {
  localStorage.setItem('codealong.project', 'All projects');
  localStorage.setItem('codealong.view', 'sessions');
  var picked = null;
  var helpers = [];
  var news = 'On your PR #397 “Cursor file tools” (https://github.com/northwind/app/pull/397):\n- ana asked for changes: “Add a test for the quokka path.”\nTake it from there, unless you were told to leave it to someone else. This message doesn’t ask you to merge it: merge it only if you were asked to.';
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    var p = real(cmd, args, options);
    if (cmd === 'session_page' && picked && (args || {}).key === picked.key && (args || {}).before == null) {
      return p.then(function (page) {
        var at = Date.now() - 30e3;
        var item = function (kind, text, n, extra) { return Object.assign({ at: at + n * 1000, kind: kind, text: text, failed: false, cursor: 10 + n, n: 0 }, extra || {}); };
        var a = helpers[0];
        var b = helpers[1];
        page.items = page.items.concat([
          item('codealong', news, 0, { glance: { about: 'pr', pr: { number: 397, title: 'Cursor file tools', url: 'https://github.com/northwind/app/pull/397' }, parts: [{ line: 'ana asked for changes' }] } }),
          item('codealong', 'About your helpers:\n- The helper “' + a.title + '” (' + a.key + ') ended its turn: `codealong session wait ' + a.key + '` prints its reply.\n- The helper “' + b.title + '” (' + b.key + ') waits on your user.', 1, {
            glance: { about: 'helper', parts: [{ session: { key: a.key, title: a.title }, line: 'ended its turn' }, { session: { key: b.key, title: b.title }, line: 'waits on you' }] },
          }),
          item('codealong', 'An older message, with no line for its card.', 2),
          item('agent', 'On it.', 3),
        ]);
        page.after = 20;
        return page;
      });
    }
    if (cmd !== 'get_board') return p;
    return p.then(function (board) {
      board.lanes.forEach(function (l) {
        l.sessions.forEach(function (s) {
          if (l.hidden || l.device || s.visitor || s.archived) return;
          if (!picked) picked = { key: s.key, lane: l.id };
          else if (helpers.length < 2 && s.key !== picked.key) helpers.push({ key: s.key, title: s.title || 'A helper' });
        });
      });
      return board;
    });
  };

  var row = function () { return picked && document.getElementById('row-' + picked.lane + '::' + picked.key); };
  var cards = function () { return Array.prototype.slice.call(document.querySelectorAll('.ca-sessinsp .ca-msgcard')); };
  var open = function (card) { var whole = card.querySelector('.ca-msgcard-body'); return card.classList.contains('is-open') && !!whole && !whole.hidden; };
  // ⌘ on a Mac is Ctrl on Linux and Windows (platform.ts `held`).
  var linux = window.__CODEALONG_PLATFORM__ === 'linux' || window.__CODEALONG_PLATFORM__ === 'windows' || /\bLinux\b/.test(navigator.userAgent);
  var tick = function () { return new Promise(function (r) { setTimeout(r, 30); }); };
  var field = function () { return document.querySelector('.ca-findbar input'); };
  var type = function (input, value) {
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set.call(input, value);
    input.dispatchEvent(new Event('input', { bubbles: true }));
  };

  say('message card check: waiting for the list');
  until(function () { return row() && helpers.length === 2; }, 20000, 'no session to show, with two others as its helpers')
    .then(function () {
      row().dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
      return until(function () { return cards().length === 3; }, 8000, 'the three cards');
    })
    .then(function () {
      var pr = cards()[0];
      var helper = cards()[1];
      var line = pr.querySelector('.ca-msgcard-title').textContent;
      check('a PR’s news is its PR and what happened, in a line', /#397 Cursor file tools/.test(line) && /ana asked for changes/.test(line), line);
      check('with the PR as a link', !!pr.querySelector('.ca-msgcard-title a.ca-prlink'), pr.innerHTML.slice(0, 200));
      // What shows: the line; the whole message is there, hidden, until it's opened.
      var shows = function (card) { return card.querySelector('.ca-msgcard-head').textContent; };
      check('and none of what it says to the agent', !/Take it from there|merge it only|quokka/.test(shows(pr)) && pr.querySelector('.ca-msgcard-body').hidden, shows(pr));
      var links = Array.prototype.map.call(helper.querySelectorAll('.ca-msgcard-title a.ca-sesslink'), function (a) { return a.getAttribute('href'); });
      check('several helpers’ news names each helper as a link to its session', links.join(' ') === helpers.map(function (h) { return 'codealong://session/' + h.key; }).join(' '), links.join(' '));
      check('with what happened to each, and none of the commands', /ended its turn · .*waits on you/.test(shows(helper)) && !/codealong session wait/.test(shows(helper)), shows(helper));
      check('each closed until opened', !open(pr) && !open(helper));
      var older = cards()[2];
      check('one with no line from the engine is a card headed Codealong, its words under it', older.querySelector('.ca-msgcard-title').textContent === 'Codealong' && !older.querySelector('.ca-msgcard-body').hidden && /An older message/.test(older.querySelector('.ca-msgcard-body').textContent) && !older.querySelector('.ca-msgcard-open'), older.textContent);
      pr.querySelector('.ca-msgcard-head').click();
      return tick();
    })
    .then(function () {
      var pr = cards()[0];
      check('a click on the line opens the whole message under it', open(pr) && /Take it from there/.test(pr.querySelector('.ca-msgcard-body').textContent) && pr.querySelector('.ca-msgcard-open').getAttribute('aria-expanded') === 'true');
      if (window.__card_screenshot) return 'stay';
      pr.querySelector('.ca-msgcard-open').click();
      return tick();
    })
    .then(function (stay) {
      if (stay) return 'stay';
      check('and its button closes it', !open(cards()[0]));
      // A link on the line goes where it goes, and the card stays as it was.
      cards()[0].querySelector('.ca-msgcard-title a').click();
      return tick().then(function () { check('a click on its link leaves it closed', !open(cards()[0])); });
    })
    .then(function (stay) {
      if (stay) return;
      var box = document.querySelector('.ca-sessinsp .ca-composer textarea');
      if (!box) throw new Error('no box to press ⌘F from');
      box.focus();
      box.dispatchEvent(new KeyboardEvent('keydown', { key: 'f', metaKey: !linux, ctrlKey: linux, bubbles: true, cancelable: true }));
      return until(function () { return field(); }, 2000, 'the find bar')
        .then(function () {
          type(field(), 'quokka');
          return until(function () { return open(cards()[0]); }, 2000, 'the card to open for the find');
        })
        .then(function () {
          var whole = cards()[0].querySelector('.ca-msgcard-body');
          check('⌘F finding a word only the whole message has opens it, to show it', /quokka/.test(whole.textContent) && cards()[0].querySelector('.ca-msgcard-open').disabled);
          // A click on its line while the find holds it open changes nothing.
          cards()[0].querySelector('.ca-msgcard-line').click();
          // The one word is in the opened message and nowhere else: one match, which it's at.
          var count = function () { return ((document.querySelector('.ca-findbar-count') || {}).textContent || '').trim(); };
          return until(function () { return /^1 of 1\b/.test(count()); }, 3000)
            .catch(function () {})
            .then(function () { check('and the find counts it there, once', /^1 of 1\b/.test(count()), count()); });
        })
        .then(function () {
          field().dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true }));
          return until(function () { return !field(); }, 2000, 'the find bar to close');
        })
        .then(function () { check('closing the find closes it again', !open(cards()[0])); });
    })
    .then(function () { if (!window.__card_screenshot) done(); })
    .catch(stopped);
})();
