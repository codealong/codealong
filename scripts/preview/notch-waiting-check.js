// Checks the shortcut that goes through what waits on you with the bar docked in the notch
// (quick.rs `work_through`, Bar.tsx): pressed, the notch opens on the first session that waits,
// with the keyboard, as a click on the notch does; pressed again, it puts it away and gives the
// keyboard back; and with nothing on the bar to show, the notch opens on the end of what waits,
// not the waiting window (notch-end-check.js has the rest of it).
//   PREVIEW_SIZE=1512,980 PREVIEW_WAIT=60000 scripts/preview.sh notch '' scripts/preview/notch-waiting-check.js
(function () {
  var calls = sent;
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
      check('the waiting window stays out of it', calls('show_waiting').length === 0, JSON.stringify(calls('show_waiting')));
      // Nothing on the bar to show.
      return feed.then(function (f) { window.__emit('bar-feed', Object.assign({}, f, { items: [] })); return wait(400); });
    })
    .then(function () {
      press();
      return wait(400);
    })
    .then(function () {
      check('with nothing on the bar the notch opens on the end of what waits', is('end') && /Nothing waits on you/.test((document.querySelector('.ca-notch-session .ca-queue-end') || {}).textContent || ''), notch().className);
      check('and the waiting window stays out of it', calls('show_waiting').length === 0, JSON.stringify(calls('show_waiting')));
      check('and the quick panel too', calls('show_quick').length === 0, JSON.stringify(calls('show_quick')));
    })
    .then(done)
    .catch(function (e) { stopped(e.message + ' (' + (notch() ? notch().className : 'no notch') + ')'); });
})();
