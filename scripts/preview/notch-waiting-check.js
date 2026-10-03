// Checks the shortcut that goes through what waits on you with the bar docked in the notch
// (quick.rs `work_through`, Bar.tsx): pressed, the notch opens on the first session that waits,
// with the keyboard, as a click on the notch does; pressed again, it puts it away and gives the
// keyboard back; and with nothing on the bar to show, the quick panel opens on what waits.
//   PREVIEW_SIZE=1512,980 PREVIEW_WAIT=60000 scripts/preview.sh notch '' scripts/preview/notch-waiting-check.js
// Every step should say ok.
(function () {
  var out = [];
  var say = function (t) { window.__t = t; };
  setInterval(function () { if (window.__t) document.title = window.__t; }, 100);
  var wait = function (ms) { return new Promise(function (r) { setTimeout(r, ms); }); };
  var until = function (f, ms, what) {
    var s = Date.now();
    return new Promise(function (res, rej) {
      (function p() { if (f()) return res(); if (Date.now() - s > ms) return rej(new Error('timeout: ' + what)); setTimeout(p, 10); })();
    });
  };
  var check = function (name, ok, why) { out.push(name + (ok ? ' ok' : ' FAIL' + (why ? ' (' + why + ')' : ''))); };
  var calls = function (cmd) { return window.__calls.filter(function (c) { return c[0] === cmd; }).map(function (c) { return c[1]; }); };
  var notch = function () { return document.querySelector('.ca-notch'); };
  var is = function (state) { return notch() && notch().classList.contains('is-' + state); };
  var title = function () { var t = document.querySelector('.ca-notch-session .ca-sessinsp-title, .ca-notch-session .ca-sessinsp-head h2'); return t ? t.textContent.trim() : ''; };
  var shows = function (item) { return function () { return is('session') && title().indexOf(item.title.slice(0, 20)) === 0; }; };
  var keysCalls = function () { return calls('notch_keys').map(function (a) { return a.on; }); };
  var press = function () { window.__emit('waiting-shortcut', null); };

  // Two of the board's sessions wait on you and one runs: the feed's own first three, said to be so.
  var items;
  var feed = fetch('./bar-feed.json').then(function (r) { return r.json(); }).then(function (f) {
    items = f.items.slice(0, 3).map(function (i, n) {
      var group = n === 0 ? 'working' : n === 1 ? 'needs' : 'unread';
      return Object.assign({}, i, { group: group, status: group, snoozedUntil: null, since: 1000 + n, hidden: false });
    });
    return Object.assign({}, f, { loading: false, items: items, recent: [], snoozed: [], reviews: [], todos: [] });
  });

  say('notch waiting check: waiting for it');
  until(function () { return notch(); }, 20000, 'the notch')
    .then(function () { return feed; })
    .then(function (f) {
      check('has three sessions to try', items.length === 3, String(items.length));
      window.__emit('bar-feed', f);
      return wait(500);
    })
    .then(function () {
      check('starts closed', !is('session'));
      press();
      return until(shows(items[1]), 3000, 'the first that waits');
    })
    .then(function () { return wait(300); })
    .then(function () {
      check('the shortcut opens the notch on the first that waits, not the one running', is('session') && is('open'), title());
      check('with the keyboard', keysCalls().join() === 'true', keysCalls().join());
      press();
      return until(function () { return !is('open'); }, 3000, 'put away');
    })
    .then(function () { return wait(500); })
    .then(function () {
      check('pressed again it puts it away and gives the keyboard back', !is('open') && keysCalls().join() === 'true,false', keysCalls().join());
      check('the quick panel stays out of it', calls('show_quick').length === 0, JSON.stringify(calls('show_quick')));
      // Nothing on the bar to show.
      return feed.then(function (f) { window.__emit('bar-feed', Object.assign({}, f, { items: [] })); return wait(400); });
    })
    .then(function () {
      press();
      return wait(400);
    })
    .then(function () {
      var asked = calls('show_quick');
      check('with nothing on the bar the quick panel opens on what waits', asked.length === 1 && asked[0].context.waiting === true, JSON.stringify(asked));
    })
    .catch(function (e) { check('ran', false, String(e)); })
    .then(function () { say('done: ' + out.join(' · ')); });
})();
