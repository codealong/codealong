// Checks the button beside the notch's pin that pops out into the waiting window (Notch.tsx,
// Bar.tsx): closed, it's out of sight and a click on it only opens the notch; open on its pills,
// it opens the waiting window asking for nothing in particular (where it was, else the first that
// waits); open on a session, it opens the window on that session and puts the notch's away,
// giving the keyboard up.
//   PREVIEW_SIZE=1512,980 PREVIEW_WAIT=60000 scripts/preview.sh notch '' scripts/preview/notch-popout-check.js
(function () {
  var calls = sent;
  var notch = function () { return document.querySelector('.ca-notch'); };
  var shape = function () { return document.querySelector('.ca-notch-shape'); };
  var popOut = function () { return document.querySelector('.ca-notch-popout'); };
  var is = function (state) { return notch() && notch().classList.contains('is-' + state); };
  var title = function () { var t = document.querySelector('.ca-notch-session .ca-sessinsp-title, .ca-notch-session .ca-sessinsp-head h2'); return t ? t.textContent.trim() : ''; };
  var shows = function (item) { return function () { return is('session') && title().indexOf(item.title.slice(0, 20)) === 0; }; };
  var keysCalls = function () { return calls('notch_keys').map(function (a) { return a.on; }); };
  var opens = function () { return calls('show_waiting').map(function (a) { return a.context; }); };
  var escape = function () { window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', cancelable: true })); };

  // Two of the board's sessions wait on you and one runs: the feed's own first three, said to be
  // so, none moved to the top, so the one that needs you is the first that waits.
  var items;
  var feed = fetch('./bar-feed.json').then(function (r) { return r.json(); }).then(function (f) {
    items = f.items.slice(0, 3).map(function (i, n) {
      var group = n === 0 ? 'working' : n === 1 ? 'needs' : 'unread';
      return Object.assign({}, i, { group: group, status: group, snoozedUntil: null, since: 1000 + n, hidden: false, raised: undefined });
    });
    return Object.assign({}, f, { loading: false, items: items, recent: [], snoozed: [], reviews: [], todos: [] });
  });

  say('notch pop-out check: waiting for it');
  until(function () { return notch() && popOut(); }, 20000, 'the notch')
    .then(function () { return feed; })
    .then(function (f) {
      check('has three sessions to try', items.length === 3, String(items.length));
      window.__emit('bar-feed', f);
      return wait(500);
    })
    .then(function () {
      check('closed, the button is out of sight', getComputedStyle(popOut()).opacity === '0');
      shape().dispatchEvent(new PointerEvent('pointerover', { bubbles: true }));
      popOut().click();
      return wait(300);
    })
    .then(function () {
      check('closed, a click on it only opens the notch', is('open') && opens().length === 0, JSON.stringify(opens()));
      // It opened on a session, as a click anywhere on it does: put away, the pills stay under
      // the pointer.
      escape();
      return until(function () { return !is('session') && is('open'); }, 3000, 'the pills alone');
    })
    .then(function () { return wait(400); })
    .then(function () {
      check('open, the button shows', getComputedStyle(popOut()).opacity === '1');
      check('it says what it does', /waiting window/.test(popOut().getAttribute('aria-label') || ''), popOut().getAttribute('aria-label'));
      popOut().click();
      return wait(200);
    })
    .then(function () {
      var o = opens();
      check('on the pills, it opens the waiting window asking for nothing in particular', o.length === 1 && !o[0].key, JSON.stringify(o));
      window.__emit('waiting-shortcut', null);
      return until(shows(items[1]), 3000, 'the first that waits');
    })
    .then(function () { return wait(300); })
    .then(function () {
      check('the notch shows a session with the keyboard', is('session') && keysCalls().slice(-1)[0] === true, keysCalls().join());
      popOut().click();
      return until(function () { return !is('session'); }, 3000, 'put away');
    })
    .then(function () { return wait(300); })
    .then(function () {
      var o = opens();
      check('on a session, it opens the waiting window on that session', o.length === 2 && o[1].key === items[1].key, JSON.stringify(o));
      check('and the notch puts it away, giving the keyboard up', !is('session') && keysCalls().slice(-1)[0] === false, keysCalls().join());
    })
    .then(done)
    .catch(stopped);
})();
