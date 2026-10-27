// Checks the floating bar's snooze menus (Bar.tsx, snooze.ts): a session pill's Snooze submenu
// snoozes it, a folded pill's snoozes all of its sessions, each leaving the bar for the snoozed
// pill at once (ahead.ts), and the snoozed pill opens, wakes or wakes all. The engine lists only
// sessions you snoozed in the feed's `snoozed`, and folds pills only on a busy day, so the bar
// shows sessions of the check's own in place of the feed's: one that needs you, enough whose turn
// it is to fold, and three snoozed. They're copies of a session of this Mac's board, added to its
// lane, so a pill's menu (session_at) is the one a real session gets.
//   PREVIEW_WAIT=60000 scripts/preview.sh bar '' scripts/preview/snooze-check.js
(function () {
  var texts = function (items) { return items.map(function (i) { return i.item === 'Separator' ? '-' : i.text; }); };
  var find = function (items, text) { return items.filter(function (i) { return i.text && i.text.indexOf(text) === 0; })[0]; };
  var snoozes = function () { return calls('snooze_sessions').map(function (c) { return c[1]; }); };

  var UNREAD = 12;
  var SNOOZED = 3;
  var now = Date.now();
  var own = function (s) { return !s.visitor && !s.archived && s.key.charAt(0) !== '@'; };
  // The copies, made once from the board's first session of this Mac: [lane id, session] each.
  var made = null;
  var copies = function (board) {
    if (made) return made;
    var lane = board.lanes.filter(function (l) { return !l.hidden && !l.device && l.sessions.some(own); })[0];
    if (!lane) return (made = []);
    var from = lane.sessions.filter(own)[0];
    made = [];
    for (var n = 0; n < 1 + UNREAD + SNOOZED; n++) {
      var key = from.key + ':snooze-check-' + n;
      var status = n === 0 ? 'needs' : 'unread';
      var until = n > UNREAD ? now + (n - UNREAD + 1) * 3600e3 : null;
      // Titles long enough that a pill is as wide as one gets, so the bar folds; none starts as
      // a menu item does, which the menus are searched by.
      made.push([lane.id, Object.assign({}, from, { key: key, title: 'Copied session ' + (n + 1) + ', with a title as long as they get', name: null, status: status, statusSince: now - (n + 1) * 60e3, archived: false, snoozedUntil: until, openTarget: 's|' + key + '|resume' })]);
    }
    return made;
  };
  // A copy as the feed lists it (barfeed.rs).
  var pill = function (laneId, s, group) {
    return { key: s.key, group: group, status: s.status, asks: false, title: s.title, account: s.account, laneId: laneId, callsign: 'snooze-check', project: '', hidden: false, detail: null, since: s.statusSince, openTarget: s.openTarget, target: null, snoozedUntil: s.snoozedUntil, stoppable: false };
  };
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    var p = real(cmd, args, options);
    // The board has the copies in their lane, for session_at, which reads it.
    if (cmd === 'get_board') {
      return p.then(function (board) {
        copies(board).forEach(function (c) {
          board.lanes.filter(function (l) { return l.id === c[0]; })[0].sessions.push(c[1]);
        });
        return board;
      });
    }
    if (cmd === 'bar_feed') {
      return Promise.all([p, fetch('./board.json').then(function (r) { return r.json(); })]).then(function (all) {
        var list = copies(all[1]);
        var items = list.filter(function (c) { return !c[1].snoozedUntil; }).map(function (c) { return pill(c[0], c[1], c[1].status); });
        var snoozed = list.filter(function (c) { return c[1].snoozedUntil; }).map(function (c) { return pill(c[0], c[1], 'snoozed'); });
        return Object.assign({}, all[0], { loading: false, items: items, recent: [], snoozed: snoozed, reviews: [], todos: [] });
      });
    }
    return p;
  };

  // How many the snoozed pill holds: the feed's, and those snoozed from the bar since.
  var asleep = SNOOZED;
  say('snooze check: waiting for pills');
  until(function () { return made && (made.length === 0 || (document.querySelector('.ca-hud [data-key]') && document.querySelector('.ca-hud .is-snoozed'))); }, 20000)
    .then(function () {
      if (made.length === 0) throw new Error('no session of this Mac on the board to copy');
      var pill = document.querySelector('.ca-hud [data-key]');
      rightClick(pill);
      return wait(200).then(function () {
        var menu = lastMenu();
        var group = find(menu, 'Snooze');
        check('a session pill has a Snooze submenu', !!group && group.items, JSON.stringify(texts(menu)));
        check('with six times', group && texts(group.items).join('|').indexOf('For 15 minutes|For 30 minutes|For 1 hour|For 3 hours|Until ') === 0, group && texts(group.items).join('|'));
        var before = Date.now();
        pick(find(group.items, 'For 1 hour'));
        return wait(100).then(function () {
          var s = snoozes().pop();
          check('it snoozes that session for an hour', s && s.keys.join() === pill.dataset.key && Math.abs(s.until - before - 3600000) < 60000, JSON.stringify(s));
          asleep += 1;
          var snoozedPill = document.querySelector('.ca-hud .is-snoozed');
          check('and it leaves its group for the snoozed pill at once', !document.querySelector('.ca-hud [data-key="' + CSS.escape(pill.dataset.key) + '"]') && snoozedPill.textContent === asleep + ' snoozed', snoozedPill.textContent);
        });
      });
    })
    .then(function () {
      var fold = document.querySelector('.ca-hud .ca-hud-fold.is-unread') || document.querySelector('.ca-hud .ca-hud-fold.is-needs');
      if (!fold) throw new Error('the bar folded none of its ' + UNREAD + ' pills whose turn it is');
      fold.click();
      return wait(200).then(function () {
        var menu = lastMenu();
        var group = find(menu, 'Snooze these');
        check('a folded pill snoozes its sessions together', !!group, JSON.stringify(texts(menu)));
        var n = menu.filter(function (i) { return i.text && !i.items && i.text.indexOf('Mark ') !== 0; }).length;
        pick(group.items[0]);
        return wait(100).then(function () {
          var s = snoozes().pop();
          check('all of them, for 15 minutes', s && n > 0 && s.keys.length === n && s.until - Date.now() > 14 * 60000, JSON.stringify(s));
          asleep += n;
        });
      });
    })
    .then(function () {
      var pill = document.querySelector('.ca-hud .is-snoozed');
      check('the snoozed pill says how many and until when', new RegExp('^' + asleep + ' sessions snoozed, back on the bar when their time comes:\\n.+ · until .+').test(pill.title), pill.title);
      pill.click();
      return wait(200).then(function () {
        var menu = lastMenu();
        var entries = menu.filter(function (i) { return i.items; });
        check('its menu lists each with when it is back', entries.length === asleep && entries.every(function (e) { return / · until .+/.test(e.text); }), JSON.stringify(texts(menu)));
        check('and wakes them all', texts(menu).slice(-2).join('|') === '-|Wake all', JSON.stringify(texts(menu)));
        var first = entries[0];
        check('each shows in Codealong first, then opens, wakes or snoozes again', /^Show in Codealong\|(Show|Attach|Resume|Open|Respond).*\|-\|Wake now\|Snooze instead$/.test(texts(first.items).join('|')), texts(first.items).join('|'));
        pick(find(first.items, 'Wake now'));
        return wait(100).then(function () {
          var s = snoozes().pop();
          check('Wake now wakes that one', s && s.keys.length === 1 && s.until === null, JSON.stringify(s));
          pick(first.items[1]);
          return wait(300);
        }).then(function () {
          var s = snoozes().pop();
          var opened = calls('open_from_bar').pop();
          check('opening wakes it, then opens it', s && s.until === null && s.keys.length === 1 && !!opened && window.__calls.indexOf(opened) > window.__calls.lastIndexOf(calls('snooze_sessions').pop()), JSON.stringify([s, opened && opened[1]]));
          pick(find(menu, 'Wake all'));
          return wait(100);
        }).then(function () {
          var s = snoozes().pop();
          check('Wake all wakes them all', s && s.keys.length === asleep && s.until === null, JSON.stringify(s));
        });
      });
    })
    .then(done)
    .catch(stopped);
})();
