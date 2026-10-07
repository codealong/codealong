// Checks what clicks on the floating bar do (Bar.tsx): a click on a session's pill shows the
// session in Codealong's window once it's clear no second click follows, a double-click opens it
// where it lives without showing it in Codealong first, and a session picked from a folded pill's
// menu or the recent pill's shows in Codealong too. The same pills and menus serve the bar in the
// notch. The engine folds pills only on a busy day, and lists recent sessions only after a day of
// work, so the bar shows sessions of the check's own in place of the feed's: one that needs you,
// enough whose turn it is to fold, and three recent ones. Clicks and menu picks ask nothing of the
// engine that reads the board, so the sessions need be on no board.
//   PREVIEW_WAIT=30000 scripts/preview.sh bar '' scripts/preview/bar-clicks-check.js
(function () {
  var UNREAD = 12;
  var RECENT = 3;
  var now = Date.now();
  // Titles long enough that a pill is as wide as one gets, so the bar folds.
  var made = function (n, group) {
    var key = 'claude:claude:bar-clicks-check-' + n;
    return { key: key, group: group, status: group === 'recent' ? 'idle' : group, asks: false, title: 'Bar clicks session ' + (n + 1) + ', with a title as long as they get', account: 'claude', laneId: '/tmp/bar-clicks-check', callsign: 'bar-clicks-check', project: '', hidden: false, detail: null, since: now - (n + 1) * 60e3, openTarget: 's|' + key + '|focus', target: null, snoozedUntil: null, stoppable: false };
  };
  var items = [made(0, 'needs')];
  for (var n = 1; n <= UNREAD; n++) items.push(made(n, 'unread'));
  var recent = [];
  for (var r = 0; r < RECENT; r++) recent.push(made(1 + UNREAD + r, 'recent'));
  var all = items.concat(recent);
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    var p = real(cmd, args, options);
    if (cmd !== 'bar_feed') return p;
    return p.then(function (feed) { return Object.assign({}, feed, { loading: false, items: items, recent: recent, snoozed: [], reviews: [], todos: [] }); });
  };

  var first = function (menu) { return menu.filter(function (i) { return i.text && i.handler; })[0]; };
  var byTitle = function (text) { return all.filter(function (i) { return text.indexOf(i.title + ' · ') === 0; })[0]; };

  var tryMenu = function (name, selector, group) {
    var pill = document.querySelector(selector);
    check(name + ': the bar has the pill', !!pill);
    if (!pill) return Promise.resolve();
    var opens = calls('open_from_bar').length;
    var shows = calls('show_main').length;
    pill.click();
    return wait(200).then(function () {
      var item = first(lastMenu());
      var session = item && byTitle(item.text);
      check(name + ' lists the check’s sessions', !!session && session.group === group, item ? item.text : 'no menu');
      if (!item) return;
      pick(item);
      return wait(100).then(function () {
        var shown = calls('show_main')[shows];
        check(name + ' shows the session in Codealong', !!shown && !!shown[1].reveal && shown[1].reveal.kind === 'session' && !!session && shown[1].reveal.key === session.key, JSON.stringify(shown && shown[1]));
        check(name + ' doesn’t open it where it lives', calls('open_from_bar').length === opens);
      });
    });
  };

  var clickPill = function () {
    var pill = document.querySelector('.ca-hud [data-key]');
    var key = pill.getAttribute('data-key');
    check('the bar shows the check’s sessions', key === items[0].key, key);
    var opens = calls('open_from_bar').length;
    var shows = calls('show_main').length;
    pill.dispatchEvent(new MouseEvent('click', { bubbles: true, detail: 1 }));
    return wait(100)
      .then(function () {
        check('a click waits for a second', calls('show_main').length === shows);
        return wait(300);
      })
      .then(function () {
        var shown = calls('show_main')[shows];
        check('a click shows the session in Codealong', !!shown && shown[1].reveal && shown[1].reveal.key === key, JSON.stringify(shown && shown[1]));
        check('a click doesn’t open it where it lives', calls('open_from_bar').length === opens);
        shows = calls('show_main').length;
        pill.dispatchEvent(new MouseEvent('click', { bubbles: true, detail: 1 }));
        return wait(60);
      })
      .then(function () {
        pill.dispatchEvent(new MouseEvent('click', { bubbles: true, detail: 2 }));
        return wait(400);
      })
      .then(function () {
        check('a double-click opens it where it lives', calls('open_from_bar').length === opens + 1);
        check('a double-click doesn’t show it in Codealong', calls('show_main').length === shows);
      });
  };

  say('bar clicks check: waiting for pills');
  until(function () { return document.querySelector('.ca-hud [data-key]'); }, 20000)
    .then(function () { return wait(300); })
    .then(clickPill)
    // Not the copies drawn out of sight to measure them, which carry data-fold and data-recent.
    .then(function () { return tryMenu('a folded pill’s menu', '.ca-hud .ca-hud-fold:not([data-fold])', 'unread'); })
    .then(function () { return tryMenu('the recent pill’s menu', '.ca-hud .is-recent:not([data-recent])', 'recent'); })
    .then(done)
    .catch(stopped);
})();
