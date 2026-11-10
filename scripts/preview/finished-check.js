// Checks a finished session (board.rs `finished_of`, insights.ts `finishedLabel` and
// `endingLabel`, lanes.ts `byStatusTime`): its row says Finished, with why in the chip's tooltip,
// in place of how far its work got; one only its reply says is done says Looks finished; a turn
// that hands you a next step says so, and one that only reports says nothing more; in Your turn
// the finished ones come after the turns that want something of you, however long they've waited;
// the session's header has the same chip; and Your turn's heading picks the finished ones, which
// the panel then lists, each saying it's finished, to mark as done together.
//   scripts/preview.sh sessions '' scripts/preview/finished-check.js
(function () {
  // This runs before the app's script: the board it gets has five turns that ended, made from
  // real sessions on this device that aren't pinned (which list apart). The finished ones have
  // waited longest, so only their being finished puts them after the others.
  localStorage.setItem('codealong.project', 'All projects');
  localStorage.setItem('codealong.view', 'sessions');
  localStorage.removeItem('codealong.sessionPanelMax');
  var now = Date.now();
  var plan = [
    { title: 'Finished A', status: 'unread', waited: 50, insight: { landing: 'landed', finished: 0.95, asksYou: 0.02, ending: 'report' }, finished: { sure: true, why: ['#12 merged', 'its todo is done', 'its reply reads as done'] } },
    { title: 'Finished B', status: 'unread', waited: 30, insight: { landing: 'pushed', finished: 0.6, asksYou: 0.05, ending: 'handoff' } },
    { title: 'Finished C', status: 'unread', waited: 20, insight: { landing: 'none', finished: 0.3, asksYou: 0.05, ending: 'report' } },
    { title: 'Finished D', status: 'idle', waited: 40, insight: { landing: 'none', finished: 0.9, asksYou: 0.02 }, finished: { sure: false, why: ['its reply reads as done'] } },
    { title: 'Finished E', status: 'unread', waited: 45, insight: { landing: 'none', finished: 0.9, asksYou: 0.02, ending: 'offer' }, finished: { sure: false, why: ['its reply reads as done'] } },
  ];
  var picked = [];
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    var p = real(cmd, args, options);
    if (cmd !== 'get_board') return p;
    return p.then(function (board) {
      var seen = {};
      var pinned = board.pinned || [];
      board.lanes.forEach(function (l) {
        l.sessions.forEach(function (s) {
          s.finished = undefined;
          if (l.hidden || l.device || s.visitor || s.archived || pinned.indexOf(s.key) >= 0 || seen[s.key] || picked.length >= plan.length) return;
          seen[s.key] = true;
          var p = plan[picked.length];
          s.status = p.status;
          s.statusSince = now - p.waited * 60e3;
          s.lastActivity = now - p.waited * 60e3;
          s.lastPromptAt = now - (p.waited + 5) * 60e3;
          s.asks = false;
          s.stalledSince = null;
          s.snoozedUntil = undefined;
          s.raised = undefined;
          s.quiet = false;
          s.helper = undefined;
          s.headless = false;
          s.agentSeen = undefined;
          s.connectionLost = undefined;
          s.limit = undefined;
          s.cutOff = undefined;
          s.background = undefined;
          s.origin = undefined;
          s.title = p.title;
          s.insight = p.insight;
          s.finished = p.finished;
          picked.push({ key: s.key, lane: l.id });
        });
      });
      return board;
    });
  };

  var title = function () { var t = document.querySelector('.ca-sessinsp-title'); return t ? t.textContent : null; };
  var row = function (i) { return document.getElementById('row-' + picked[i].lane + '::' + picked[i].key); };
  var chips = function (el) { return el ? [].map.call(el.querySelectorAll('.ca-insight'), function (c) { return c.textContent; }) : []; };
  var chip = function (el, label) { return el ? [].filter.call(el.querySelectorAll('.ca-insight'), function (c) { return c.textContent === label; })[0] : null; };
  var top = function (i) { return row(i).getBoundingClientRect().top; };
  var pick = function () { return [].filter.call(document.querySelectorAll('.ca-bay--unread .ca-linkbtn'), function (b) { return /finished$/.test(b.textContent); })[0]; };
  var pickRows = function () { return [].slice.call(document.querySelectorAll('.ca-pick-row')); };

  say('finished check: waiting for the list');
  until(function () { return picked.length === plan.length && [0, 1, 2, 3, 4].every(row); }, 20000, 'five unpinned sessions of this Mac to stand in')
    .then(function () { return wait(300); })
    .then(function () {
      var a = chip(row(0), 'Finished');
      check('a finished session’s row says Finished', !!a && a.classList.contains('is-good'), chips(row(0)).join(' | '));
      check('whose tooltip says why', !!a && a.title === 'Its work is done and nothing is left for you: #12 merged, its todo is done, its reply reads as done. Mark it as done once you’ve read it.', a ? a.title : '');
      check('in place of how far its work got', chips(row(0)).indexOf('Landed') < 0, chips(row(0)).join(' | '));
      check('a turn that hands you a next step says so, beside how far its work got', !!chip(row(1), 'Next step is yours') && !!chip(row(1), 'Pushed'), chips(row(1)).join(' | '));
      check('a turn that only reports says nothing more', chips(row(2)).length === 0, chips(row(2)).join(' | '));
      var d = chip(row(3), 'Looks finished');
      check('one only its reply says is done looks finished, seen or not', !!d && d.classList.contains('is-quiet') && /^Nothing seems left for you: its reply reads as done\./.test(d.title) && !!chip(row(4), 'Looks finished'), chips(row(3)).join(' | '));
      check('and says that in place of how its reply ends', chips(row(4)).indexOf('Offers more') < 0, chips(row(4)).join(' | '));
      // B has waited 30 minutes and C 20: the longest first. A has waited 50 and E 45, and they
      // come after both.
      check('Your turn lists the turns that want something first, the longest wait first', top(1) < top(2), top(1) + ' ' + top(2));
      check('and the finished ones after them, though they’ve waited longest', top(2) < top(0) && top(0) < top(4), [top(2), top(0), top(4)].join(' '));
      row(0).dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
      return until(function () { return title() === 'Finished A'; }, 3000, 'the session shown').then(function () { return wait(50); });
    })
    .then(function () {
      var head = document.querySelector('.ca-sessinsp-head') || document.querySelector('.ca-sessinsp');
      var a = chip(head, 'Finished');
      check('its header says Finished too', !!a && /#12 merged/.test(a.title), chips(head).join(' | '));
      check('Your turn’s heading offers to select the finished ones', !!pick() && pick().textContent === 'Select 2 finished', pick() ? pick().textContent : 'no button');
      var offers = [].filter.call(document.querySelectorAll('.ca-bay .ca-linkbtn'), function (b) { return /finished$/.test(b.textContent); });
      check('and no other heading does', offers.length === 1, offers.map(function (b) { return b.textContent; }).join(' | '));
      row(1).dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
      return until(function () { return title() === 'Finished B'; }, 3000, 'another session shown');
    })
    .then(function () {
      pick().click();
      return until(function () { return pickRows().length === 2; }, 3000, 'the finished ones picked').then(function () { return wait(50); });
    })
    .then(function () {
      var names = pickRows().map(function (r) { return r.querySelector('.ca-pick-name').textContent; });
      check('which picks just them, in the list’s order', names.join(' | ') === 'Finished A | Finished E', names.join(' | '));
      check('each saying it’s finished, with why', !!chip(pickRows()[0], 'Finished') && /#12 merged/.test(chip(pickRows()[0], 'Finished').title) && !!chip(pickRows()[1], 'Looks finished'), pickRows().map(function (r) { return chips(r).join(','); }).join(' | '));
      var listed = [0, 1, 2, 3, 4].filter(function (i) { return row(i).classList.contains('is-selected'); });
      check('and their rows show as picked', listed.join(',') === '0,4', listed.join(','));
    })
    .catch(function (e) { check('ran', false, String(e) + ' at ' + title()); })
    .then(done);
})();
