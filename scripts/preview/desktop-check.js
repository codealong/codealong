// Checks cards pinned to the desktop (DesktopPin.tsx) and the menu items that pin them (App.tsx).
// In the desktop view, on a worktree's card: it sizes its window, a click on a session line shows
// that session in the peek and a second click puts it away, a double-click opens the worktree or
// the line's session where it lives without the peek opening first, its menu (the worktree menu
// every window has, Remove from desktop with only copying after it) shows it in Codealong and
// takes it off the desktop, and once the board has lost it only Remove is left. On a
// session's card, a click shows it in the peek. In the sessions view: a session's menu pins it,
// and once it's pinned takes it off.
//   PREVIEW_WAIT=60000 PREVIEW_PIN=worktree scripts/preview.sh desktop '' scripts/preview/desktop-check.js
//   PREVIEW_WAIT=60000 PREVIEW_PIN=session scripts/preview.sh desktop '' scripts/preview/desktop-check.js
//   PREVIEW_WAIT=60000 scripts/preview.sh sessions '' scripts/preview/desktop-check.js
(function () {
  var texts = function (items) { return items.map(function (i) { return i.item === 'Separator' ? '-' : i.text; }); };
  var find = function (items, text) { return items.filter(function (i) { return i.text === text; })[0]; };
  // A click as the n-th of a run, as WebKit counts them.
  var click = function (el, n) { el.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true, detail: n })); };
  var twice = function (el) { click(el, 1); click(el, 2); };

  if (window.__CODEALONG_DESKTOP__ && window.__CODEALONG_PIN__ === 'session') {
    say('desktop check: waiting for the card');
    var key = null;
    until(function () { return document.querySelector('.ca-desk .ca-cv-card.is-session'); }, 20000)
      .then(function () { return window.__TAURI_INTERNALS__.invoke('desktop_feed'); })
      .then(function (f) {
        key = f.pin.key;
        click(document.querySelector('.ca-desk .ca-cv-l1'), 1);
        check('a click waits a moment for a second one', calls('peek_open').length === 0);
        return wait(500);
      })
      .then(function () {
        var p = last('peek_open');
        check("then shows the card's session in the peek, ready to answer", p && p.key === key && p.focus === true && p.pill && p.pill.w > 200, JSON.stringify(p));
        window.__emit('peek', { key: key, engaged: true });
        return wait(100);
      })
      .then(function () {
        check('the card shows it', document.querySelector('.ca-desk-card').classList.contains('is-peeked'));
        var opens = calls('open_from_bar').length;
        twice(document.querySelector('.ca-desk .ca-cv-l1'));
        check('a double-click puts the peek away and opens it where it lives', calls('peek_hide').length === 1 && calls('open_from_bar').length === opens + 1 && last('open_from_bar').targetId.indexOf(key) >= 0, JSON.stringify(last('open_from_bar')));
        return wait(500);
      })
      .then(function () {
        check('without the peek opening again', calls('peek_open').length === 1, String(calls('peek_open').length));
        done();
      })
      .catch(stopped);
    return;
  }

  if (window.__CODEALONG_DESKTOP__) {
    var feed = null;
    var line = null;
    var s = null;
    say('desktop check: waiting for the card');
    until(function () { return document.querySelector('.ca-desk .ca-desk-session') && calls('fit_desktop').length; }, 20000)
      .then(function () { return window.__TAURI_INTERNALS__.invoke('desktop_feed'); })
      .then(function (f) {
        feed = f;
        var lane = f.lanes[0];
        var fit = last('fit_desktop');
        check('the window fits the card and its shadow', fit && fit.width === 304 && fit.height > 100, JSON.stringify(fit));
        twice(document.querySelector('.ca-desk .ca-cv-l1'));
        var opened = lane.defaultTarget ? last('open_from_bar') : last('show_main');
        check('a double-click opens the worktree', lane.defaultTarget ? opened && opened.laneId === lane.id && opened.targetId === lane.defaultTarget : opened && opened.reveal.kind === 'worktree', JSON.stringify(opened));
        line = document.querySelector('.ca-desk .ca-desk-session');
        s = lane.sessions.find(function (x) { return x.key === line.dataset.session; });
        click(line.querySelector('.ca-cvp-session-title'), 1);
        return wait(500);
      })
      .then(function () {
        var p = last('peek_open');
        var r = line.getBoundingClientRect();
        check('a click on a session line shows that session in the peek, by the line', p && p.key === s.key && p.focus === true && p.pill && Math.round(p.pill.y) === Math.round(r.top), JSON.stringify(p));
        check('a click on the worktree itself shows nothing', calls('peek_open').length === 1, String(calls('peek_open').length));
        window.__emit('peek', { key: s.key, engaged: true });
        return wait(100);
      })
      .then(function () {
        check('the line shows it', line.classList.contains('is-peeked'));
        click(line, 1);
        return wait(500);
      })
      .then(function () {
        check('a second click puts it away', calls('peek_hide').length === 1 && calls('peek_open').length === 1, String(calls('peek_hide').length));
        window.__emit('peek', { key: null, engaged: false });
        var opens = calls('open_from_bar').length;
        twice(line);
        var o = last('open_from_bar');
        check('a double-click on a line opens that session where it lives', calls('open_from_bar').length === opens + 1 && o.laneId === feed.lanes[0].id && o.targetId === s.openTarget, JSON.stringify(o));
        rightClick(document.querySelector('.ca-desk-card'));
        return wait(500);
      })
      .then(function () {
        check('and the peek stays shut', calls('peek_open').length === 1, String(calls('peek_open').length));
        var menu = lastMenu();
        var t = texts(menu);
        // A worktree's menu is the one every window has (menus.ts LANE_MENU): Remove from desktop
        // is its pins group, after what opens, shows, starts in and reuses it, and only copying
        // its path, branch and links comes after.
        var at = t.indexOf('Remove from desktop');
        var after = t.slice(at + 2);
        check('its menu has Remove from desktop after the rest, with only copying below it', at > t.indexOf('Show in Codealong') && t.indexOf('Show in Codealong') >= 0 && t.lastIndexOf('Remove from desktop') === at && t[at - 1] === '-' && t[at + 1] === '-' && after.length > 0 && after.every(function (x) { return x.indexOf('Copy ') === 0; }), t.join('|'));
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
        rightClick(document.querySelector('.ca-desk-card'));
        return wait(200);
      })
      .then(function () {
        check('and its menu only removes it', texts(lastMenu()).join('|') === 'Remove from desktop', texts(lastMenu()).join('|'));
        done();
      })
      .catch(stopped);
    return;
  }

  var row = function () { return document.querySelector('.ca-board-list [role="option"][id^="row-"]'); };
  var key = null;
  say('desktop check: waiting for sessions');
  until(function () { return !!row() && calls('desktop_pins').length; }, 20000)
    .then(function () {
      rightClick(row());
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
      rightClick(row());
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
    .catch(stopped);
})();
