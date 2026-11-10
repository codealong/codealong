// Checks the links to a session and to a todo in a session's conversation (SessionLink.tsx), as
// an agent writes them (`[its title](codealong://session/<key>)`, codealongLinks.ts). The
// conversation is a stand-in, on the first session, and names another session of this Mac's
// board four ways (a link with words, a bare link, its key as code, and a session there isn't)
// and a todo the check adds to the board two ways. A link with words keeps them and wears the
// session's status ahead of them; a bare one reads as its title; the key stays code; the one
// that names nothing is its words. Hovering one shows the session's card, a right-click has its
// menu, a ⌘-click opens it where it runs, and a click shows it in the panel, with Back to the
// session the link was in. A todo's link shows the todo. A message from another session names
// its sender as a link to it, and one from a session the board doesn't list by its name alone.
//   PREVIEW_WAIT=40000 scripts/preview.sh sessions '' scripts/preview/session-links-check.js
// As PREVIEW_INIT (with PREVIEW_NAME=session-links) it leaves the panel open on the stand-in
// with a link's card up, for a screenshot.
(function () {
  localStorage.setItem('codealong.project', 'All projects');
  localStorage.setItem('codealong.age', 'all');
  var TODO = 't-linkcheck0001';
  var GONE = 'claude:claude:no-such-session-0000';
  var picked = null;
  var other = null;
  var real = window.__TAURI_INTERNALS__.invoke;
  var said = function () {
    return [
      'Two things are waiting on you.',
      '',
      '- **[The other one](codealong://session/' + other.key + '):** asks which way to go. Its todo is [the link check](codealong://todo/' + TODO + ').',
      '- Bare, it reads codealong://session/' + other.key + ' and codealong://todo/' + TODO + '.',
      '- I sent `' + other.key + '` a note about `' + TODO + '`.',
      '- [An old one](codealong://session/' + GONE + ') is gone, and so is `' + GONE + '`.',
    ].join('\n');
  };
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    if (cmd === 'session_page' && picked && (args || {}).key === picked.key) {
      var at = Date.now() - 9 * 60e3;
      var item = function (kind, text, n, extra) { return Object.assign({ at: at + n * 1000, kind: kind, text: text, failed: false, cursor: n, n: 0 }, extra || {}); };
      return Promise.resolve({ before: null, after: 6, items: [
        item('you', 'Where do things stand?', 0),
        item('agent', said(), 1),
        item('peer', 'Which way should I go?', 2, { from: other.title, fromKey: other.key }),
        item('peer', 'Mine is done.', 3, { from: 'A session long gone', fromKey: GONE }),
      ] });
    }
    var p = real(cmd, args, options);
    if (cmd !== 'get_board') return p;
    return p.then(function (board) {
      board.lanes.forEach(function (l) {
        l.sessions.forEach(function (s) {
          if (l.hidden || s.visitor || s.archived || s.origin || !s.openTarget) return;
          if (!picked) picked = { key: s.key, lane: l.id, title: s.title };
          else if (!other && s.key !== picked.key && s.title !== picked.title) other = { key: s.key, lane: l.id, title: s.title, target: s.openTarget, status: s.status };
        });
      });
      var now = Date.now();
      board.todos = (board.todos || []).filter(function (t) { return t.id !== TODO; }).concat([
        { id: TODO, project: '', title: 'Ship the link check', description: '', status: 'in-progress', labels: [], worktrees: [], sessions: [], createdAt: now, updatedAt: now, doneAt: null, pending: [], dependsOn: [], waiting: null, review: null, due: null },
      ]);
      return board;
    });
  };

  var row = function () { return picked && document.getElementById('row-' + picked.lane + '::' + picked.key); };
  var title = function () { var t = q('.ca-sessinsp .ca-sessinsp-title'); return t ? t.textContent : null; };
  var reply = function () { return q('.ca-sessinsp .ca-thread-agent'); };
  var sessions = function () { return all('.ca-sessinsp .ca-thread-agent a.ca-sesslink'); };
  var heads = function () { return all('.ca-sessinsp .ca-thread-said.is-peer .ca-thread-meta > span:first-child'); };
  var todos = function () { return all('.ca-sessinsp .ca-thread-agent a.ca-todolink'); };
  var card = function () { return q('.ca-sesshover'); };
  var hover = function (el) { el.dispatchEvent(new MouseEvent('mouseover', { bubbles: true, relatedTarget: document.body })); };
  var leave = function (el) { el.dispatchEvent(new MouseEvent('mouseout', { bubbles: true, relatedTarget: document.body })); };
  var open = function () {
    row().dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
    row().click();
    return until(function () { return title() === picked.title && sessions().length === 3; }, 10000, 'the stand-in conversation, with its three links to the other session');
  };

  say('session links check: waiting for the list');
  var ready = until(function () { return row(); }, 20000)
    .catch(function () { throw new Error('no session on the board to stand in for'); })
    .then(function () {
      if (!other) throw new Error('the board has no second session to link to');
      return open();
    });

  if (!window.__PREVIEW_SIZE__) {
    // As PREVIEW_INIT, for a screenshot: the conversation, with the first link's card up.
    ready.then(function () { hover(sessions()[0]); }).catch(function () {});
    return;
  }

  ready
    .then(function () {
      var s = sessions();
      check('a link with words keeps them', s[0].textContent === 'The other one', s[0].textContent);
      check('and wears the session’s status ahead of them', !!s[0].querySelector('.ca-sesslink-marks .ca-glyph'));
      check('a bare link reads as the session’s title', s[1].textContent === other.title, s[1].textContent);
      check('a key written as code stays code, and is a link', s[2].textContent === other.key && s[2].classList.contains('is-plain') && !!s[2].querySelector('code.ca-ic') && !s[2].querySelector('.ca-sesslink-marks'));
      check('each is the session’s own link', s.every(function (a) { return a.getAttribute('href') === 'codealong://session/' + other.key; }));
      var t = todos();
      check('a todo’s link keeps its words, a bare one reads as its title, and its id stays code', t.length === 3 && t[0].textContent === 'the link check' && t[1].textContent === 'Ship the link check' && t[2].textContent === TODO, t.map(function (a) { return a.textContent; }).join(' | '));
      check('with the todo’s box ahead of the words', !!t[0].querySelector('.ca-todoglyph') && !t[2].querySelector('.ca-todoglyph'));
      var gone = all('li', reply())[3];
      check('one that names no session is its words', gone.textContent === 'An old one is gone, and so is ' + GONE + '.' && !gone.querySelector('a'), gone.textContent);
      var h = heads();
      var from = h[0] && h[0].querySelector('a.ca-sesslink');
      check('a message from another session names its sender as a link to it', h.length === 2 && h[0].textContent === 'From “' + other.title + '”' && !!from && from.getAttribute('href') === 'codealong://session/' + other.key, h.map(function (x) { return x.textContent; }).join(' | '));
      check('and one from a session the board doesn’t list by its name', h[1] && h[1].textContent === 'From “A session long gone”' && !h[1].querySelector('a'), h[1] && h[1].textContent);
      hover(s[0]);
      check('no card at once', !card());
      return until(card, 3000);
    })
    .then(function () {
      var c = card();
      check('hovering one shows the session', q('.ca-psess-title', c).textContent === other.title, q('.ca-psess-title', c).textContent);
      check('and where it works', !!q('.ca-wtchip', c));
      var r = c.getBoundingClientRect();
      check('inside the window', r.left >= 0 && r.top >= 0 && r.right <= window.innerWidth && r.bottom <= window.innerHeight, [r.left, r.top, r.right, r.bottom].map(Math.round).join(','));
      leave(sessions()[0]);
      return until(function () { return !card(); }, 2000);
    })
    .then(function () {
      var made = menus();
      rightClick(sessions()[0]);
      return until(function () { return menus() > made; }, 2000, 'the session’s menu');
    })
    .then(function () {
      var labels = menuLabels();
      check('a right-click has the session’s menu', labels.some(function (l) { return /^Copy/.test(l); }) && labels.length > 4, labels.join(' | '));
      check('and selects nothing', title() === picked.title);
      var opens = calls('open_target').length;
      sessions()[0].dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true, metaKey: true }));
      return until(function () { return calls('open_target').length > opens; }, 3000, 'the session opened where it runs');
    })
    .then(function () {
      var o = last('open_target');
      check('a ⌘-click opens it where it runs', o.laneId === other.lane && o.targetId === other.target, JSON.stringify(o));
      check('and leaves the panel on the session the link is in', title() === picked.title);
      // A session counts as looked at once it has shown a second (history.ts `DWELL_MS`): Back
      // goes to one that has.
      return until(function () { return (localStorage.getItem('codealong.sessionHistory') || '').indexOf(picked.key) >= 0; }, 4000, 'the session the link is in counted as looked at');
    })
    .then(function () {
      sessions()[1].click();
      return until(function () { return title() === other.title; }, 5000, 'the linked session in the panel');
    })
    .then(function () {
      check('a click shows the session in the panel', true);
      q('.ca-histnav button[aria-label="Back"]').click();
      return until(function () { return title() === picked.title && todos().length === 3; }, 5000, 'back on the session the link was in');
    })
    .then(function () {
      check('and Back returns to the session the link was in', true);
      todos()[0].click();
      return until(function () { return q('.ca-todoinsp-title'); }, 5000, 'the todo’s panel');
    })
    .then(function () {
      var t = q('.ca-todoinsp-title');
      check('a click on a todo’s link shows the todo', (t.value || t.textContent) === 'Ship the link check', t.value || t.textContent);
    })
    .then(done)
    .catch(stopped);
})();
