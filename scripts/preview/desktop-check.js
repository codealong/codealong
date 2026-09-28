// Checks cards pinned to the desktop (DesktopPin.tsx) and the menu items that pin them (App.tsx).
// In the desktop view, on a worktree's card: it sizes its window, a click opens the worktree and
// a session line that session, its menu shows it in Codealong and takes it off the desktop, and
// once the board has lost it only Remove is left. In the sessions view: a session's menu pins it,
// and once it's pinned takes it off.
//   PREVIEW_WAIT=60000 PREVIEW_PIN=worktree scripts/preview.sh desktop '' scripts/preview/desktop-check.js
//   PREVIEW_WAIT=60000 scripts/preview.sh sessions '' scripts/preview/desktop-check.js
// Every step should say ok.
(function () {
  var out = [];
  var say = function (t) { window.__t = t; };
  setInterval(function () { if (window.__t) document.title = window.__t; }, 100);
  var wait = function (ms) { return new Promise(function (r) { setTimeout(r, ms); }); };
  var until = function (f, ms) {
    var s = Date.now();
    return new Promise(function (res, rej) {
      (function p() { if (f()) return res(); if (Date.now() - s > ms) return rej(new Error('timeout')); setTimeout(p, 10); })();
    });
  };
  var check = function (name, ok, why) { out.push(name + (ok ? ' ok' : ' FAIL') + (why ? ' (' + why + ')' : '')); };
  var calls = function (cmd) { return window.__calls.filter(function (c) { return c[0] === cmd; }); };
  var last = function (cmd) { var c = calls(cmd); return c.length ? c[c.length - 1][1] : null; };
  var lastMenu = function () { var m = calls('plugin:menu|new'); return m.length ? m[m.length - 1][1].options.items : []; };
  var texts = function (items) { return items.map(function (i) { return i.item === 'Separator' ? '-' : i.text; }); };
  var find = function (items, text) { return items.filter(function (i) { return i.text === text; })[0]; };
  var pick = function (item) { window['_' + item.handler.id]({ index: 0, message: item.id }); };
  var right = function (el) { el.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true })); };
  var done = function () { say('done: ' + out.join(' · ')); };
  var failed = function (e) { out.push('FAIL ' + e); done(); };

  if (window.__CODEALONG_DESKTOP__) {
    var feed = null;
    say('desktop check: waiting for the card');
    until(function () { return document.querySelector('.ca-desk .ca-desk-session') && calls('fit_desktop').length; }, 20000)
      .then(function () { return window.__TAURI_INTERNALS__.invoke('desktop_feed'); })
      .then(function (f) {
        feed = f;
        var lane = f.lanes[0];
        var fit = last('fit_desktop');
        check('the window fits the card and its shadow', fit && fit.width === 304 && fit.height > 100, JSON.stringify(fit));
        document.querySelector('.ca-desk .ca-cv-l1').click();
        var opened = lane.defaultTarget ? last('open_from_bar') : last('show_main');
        check('a click opens the worktree', lane.defaultTarget ? opened && opened.laneId === lane.id && opened.targetId === lane.defaultTarget : opened && opened.reveal.kind === 'worktree', JSON.stringify(opened));
        var line = document.querySelector('.ca-desk .ca-desk-session');
        var s = lane.sessions.find(function (x) { return x.key === line.dataset.session; });
        line.querySelector('.ca-cvp-session-title').click();
        var o = last('open_from_bar');
        check('a session line opens that session', o && o.laneId === lane.id && o.targetId === s.openTarget, JSON.stringify(o));
        right(document.querySelector('.ca-desk-card'));
        return wait(200);
      })
      .then(function () {
        var menu = lastMenu();
        var t = texts(menu);
        check('its menu ends in Remove from desktop', t[t.length - 1] === 'Remove from desktop' && t[t.length - 2] === '-', t.join('|'));
        pick(find(menu, 'Show in Codealong'));
        var shown = last('show_main');
        check('Show in Codealong reveals the worktree', shown && shown.reveal.kind === 'worktree' && shown.reveal.laneId === feed.pin.key, JSON.stringify(shown));
        pick(find(menu, 'Remove from desktop'));
        var off = last('unpin_from_desktop');
        check('Remove takes it off the desktop', off && off.pin.ref === 'worktree' && off.pin.key === feed.pin.key, JSON.stringify(off));
        window.__emit('desktop-feed', Object.assign({}, feed, { lanes: [], title: 'Old name' }));
        return wait(200);
      })
      .then(function () {
        var card = document.querySelector('.ca-desk .ca-cv-card');
        check('a card the board lost says so, with its last title', card.classList.contains('is-gone') && /Old name/.test(card.textContent) && /No longer on the board/.test(card.textContent), card.textContent);
        right(document.querySelector('.ca-desk-card'));
        return wait(200);
      })
      .then(function () {
        check('and its menu only removes it', texts(lastMenu()).join('|') === 'Remove from desktop', texts(lastMenu()).join('|'));
        done();
      })
      .catch(failed);
    return;
  }

  var row = function () { return document.querySelector('.ca-board-list [role="option"][id^="row-"]'); };
  var key = null;
  say('desktop check: waiting for sessions');
  until(function () { return !!row() && calls('desktop_pins').length; }, 20000)
    .then(function () {
      right(row());
      return wait(200);
    })
    .then(function () {
      var menu = lastMenu();
      var item = find(menu, 'Pin to desktop');
      check("a session's menu pins it to the desktop", !!item, texts(menu).join('|'));
      pick(item);
      var pinned = last('pin_to_desktop');
      check('as a session', pinned && pinned.pin.ref === 'session' && !!pinned.pin.key, JSON.stringify(pinned));
      key = pinned && pinned.pin.key;
      window.__emit('desktop-pins', [{ ref: 'session', key: key }]);
      return wait(200);
    })
    .then(function () {
      right(row());
      return wait(200);
    })
    .then(function () {
      var menu = lastMenu();
      var item = find(menu, 'Remove from desktop');
      check('once pinned, its menu takes it off', !!item && !find(menu, 'Pin to desktop'), texts(menu).join('|'));
      pick(item);
      var off = last('unpin_from_desktop');
      check('the same session', off && off.pin.ref === 'session' && off.pin.key === key, JSON.stringify(off));
      done();
    })
    .catch(failed);
})();
