// Checks another device's session in the bar docked in the notch (Bar.tsx, NotchSession.tsx): its
// pill carries its device's color and says the device in its tooltip, a click opens the notch on
// it with its conversation read from that device (session_at, which finds it among your devices'
// worktrees) and its header saying the device, still so after going on to another of that
// device's, and it can be kept on the desktop and its pill dragged there, as this Mac's can.
//   PREVIEW_DEVICES=scripts/preview/devices-sample.json PREVIEW_SIZE=1512,980 PREVIEW_WAIT=60000 scripts/preview.sh notch '' scripts/preview/notch-devices-check.js
(function () {
  var notch = function () { return document.querySelector('.ca-notch'); };
  var shape = function () { return document.querySelector('.ca-notch-shape'); };
  var is = function (state) { return notch() && notch().classList.contains('is-' + state); };
  var pill = function (key) { return document.querySelector('.ca-notch-body .ca-hud-pill[data-key="' + key + '"]'); };
  var title = function () { var t = document.querySelector('.ca-notch-session .ca-sessinsp-title, .ca-notch-session .ca-sessinsp-head h2'); return t ? t.textContent.trim() : ''; };
  var keep = function () { return document.querySelector('.ca-notch-session button[aria-label="Keep on the desktop"]'); };
  var badge = function () { var b = document.querySelector('.ca-notch-session .ca-devbadge'); return b ? b.textContent.trim() : ''; };
  var enter = function () { shape().dispatchEvent(new PointerEvent('pointerover', { bubbles: true })); };
  var pointer = function (el, type, x, y) {
    el.dispatchEvent(new PointerEvent(type, { bubbles: true, cancelable: true, button: type === 'pointerdown' ? 0 : -1, buttons: 1, screenX: x, screenY: y, clientX: x, clientY: y, pointerType: 'mouse' }));
  };
  var invoke = function (cmd, args) { return window.__TAURI_INTERNALS__.invoke(cmd, args || {}); };

  // One of this Mac's sessions on the bar and one of a connected device's, made from what the
  // board and your devices have, whatever those sessions are doing.
  var mine = null;
  var theirs = null;
  var next = null;
  say('notch devices check: waiting for it');
  until(function () { return notch() && calls('notch_shape').length > 0; }, 15000, 'the notch')
    .then(function () { return Promise.all([invoke('bar_feed'), invoke('get_board'), invoke('devices')]); })
    .then(function (all) {
      var feed = all[0];
      var board = all[1];
      var view = all[2];
      var here = null;
      board.lanes.forEach(function (l) {
        l.sessions.forEach(function (s) { if (!here && !s.visitor && !s.archived) here = { lane: l, session: s }; });
      });
      view.lanes.forEach(function (l) {
        l.sessions.forEach(function (s) {
          if (s.visitor || s.archived || !l.device.online) return;
          if (!theirs) theirs = { lane: l, session: s };
          else if (!next && s.title !== theirs.session.title) next = { lane: l, session: s };
        });
      });
      check('has a session here and two on a connected device', !!here && !!theirs && !!next, JSON.stringify({ here: !!here, there: !!theirs, next: !!next }));
      var item = function (at, group) {
        return { key: at.session.key, group: group, status: group, waiting: false, asks: false, title: at.session.title, account: at.session.account, laneId: at.lane.id, callsign: at.lane.callsign, project: at.lane.project, hidden: false, detail: null, since: 1000, openTarget: '', target: null, snoozedUntil: null, device: at.lane.device };
      };
      mine = item(here, 'needs');
      theirs = item(theirs, 'unread');
      next = item(next, 'unread');
      window.__emit('bar-feed', Object.assign({}, feed, { loading: false, items: [mine, theirs, next], recent: [], snoozed: [], reviews: [], todos: [], devices: [theirs.device.short] }));
      enter();
      return until(function () { return is('pills') && pill(mine.key) && pill(theirs.key); }, 3000, 'its pills');
    })
    .then(function () {
      var dot = pill(theirs.key).querySelector('.ca-labeldot');
      check('another device’s pill has its color', !!dot && dot.classList.contains('is-' + theirs.device.color), dot ? dot.className : 'none');
      check('and says the device', (pill(theirs.key).title || '').indexOf('On ' + theirs.device.name) >= 0, pill(theirs.key).title);
      check('this Mac’s has none', !pill(mine.key).querySelector('.ca-labeldot') && (pill(mine.key).title || '').indexOf(' · On ') < 0, pill(mine.key).title);
      pill(theirs.key).dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true, detail: 1 }));
      return until(function () { return is('session') && title().indexOf(theirs.title.slice(0, 20)) === 0; }, 5000, 'its session');
    })
    .then(function () { return wait(400); })
    .then(function () {
      check('a click opens the notch on it', is('session') && is('open'), title());
      check('read through session_at', sent('session_at').some(function (a) { return a.key === theirs.key; }));
      check('it can be kept on the desktop', !!keep());
      check('its header says the device', badge() === 'On ' + theirs.device.name, badge() || 'none');
      pill(next.key).click();
      return until(function () { return is('session') && title().indexOf(next.title.slice(0, 20)) === 0; }, 5000, 'the device’s next session');
    })
    .then(function () { return wait(400); })
    .then(function () {
      check('going on to another of the device’s, it still says the device', badge() === 'On ' + next.device.name, badge() || 'none');
      pill(mine.key).click();
      return until(function () { return is('session') && title().indexOf(mine.title.slice(0, 20)) === 0; }, 5000, 'this Mac’s session');
    })
    .then(function () { return wait(400); })
    .then(function () {
      check('as this Mac’s can', !!keep());
      pointer(pill(theirs.key), 'pointerdown', 100, 20);
      pointer(pill(theirs.key), 'pointermove', 104, 22);
      pointer(pill(theirs.key), 'pointermove', 140, 80);
      var d = last('drag_to_desktop');
      check('its pill is dragged onto the desktop', !!d && d.pin.ref === 'session' && d.pin.key === theirs.key, JSON.stringify(d));
      done();
    })
    .catch(stopped);
})();
