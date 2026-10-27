// Checks Pick a time… in the main window (ui/SnoozeDialog.tsx): asked for by any window's Snooze
// menu (snooze.ts `timesMenu`, whose asking snooze-check.js checks), it names what it snoozes,
// opens on the next morning, says when it would come back or that the time has passed, and
// snoozes sessions, or your review of a PR, until the day and time picked.
//   scripts/preview.sh sessions '' scripts/preview/snooze-pick-check.js
(function () {
  var req = new XMLHttpRequest();
  req.open('GET', './board.json', false);
  req.send();
  var board = JSON.parse(req.responseText);
  var session = null;
  board.lanes.forEach(function (l) {
    l.sessions.forEach(function (s) {
      if (!session && !l.hidden && !l.device && !s.visitor && !s.archived) session = s;
    });
  });

  var dialog = function () { return q('.ca-snoozedlg'); };
  var field = function (type) { return q('.ca-snoozedlg input[type="' + type + '"]'); };
  var said = function () { return (q('.ca-snoozedlg-when') || {}).textContent; };
  var submit = function () { return q('.ca-snoozedlg button[type="submit"]'); };
  // A field set as typing sets it, so React hears it.
  var type = function (input, value) {
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set.call(input, value);
    input.dispatchEvent(new Event('input', { bubbles: true }));
  };
  var pad = function (n) { return (n < 10 ? '0' : '') + n; };
  var day = function (d) { return d.getFullYear() + '-' + pad(d.getMonth() + 1) + '-' + pad(d.getDate()); };
  var asked = function (cmd) { return calls(cmd).map(function (c) { return c[1]; }); };

  say('snooze pick check: waiting for the board');
  until(function () { return q('[role="listbox"] [id^="row-"]'); }, 20000, 'no rows')
    .then(function () {
      if (!session) throw new Error('no session of this Mac on the board');
      window.__emit('reveal', { kind: 'snooze', keys: [session.key], reviews: false });
      return until(dialog, 3000, 'no dialog for Pick a time…');
    })
    .then(function () {
      var now = new Date();
      var morning = new Date(now.getFullYear(), now.getMonth(), now.getDate() + (now.getHours() < 5 ? 0 : 1), 9);
      check('it names the session', q('.ca-snoozedlg h2').textContent === 'Snooze “' + session.title + '”', q('.ca-snoozedlg h2').textContent);
      check('and opens on the next morning', field('date').value === day(morning) && field('time').value === '09:00', field('date').value + ' ' + field('time').value);
      check('saying when it would come back', /^Until (tomorrow )?9 AM$/.test(said()), said());
      type(field('date'), day(now));
      type(field('time'), '00:00');
      return wait(50).then(function () {
        check('a time gone by says so, and snoozes nothing', said() === 'That time has passed.' && submit().disabled, said());
        var later = new Date(now.getFullYear(), now.getMonth(), now.getDate() + 9, 15, 30);
        type(field('date'), day(later));
        type(field('time'), '15:30');
        return wait(50).then(function () {
          check('a later one says its date', /^Until [A-Z][a-z]{2} \d+, 3:30 PM$/.test(said()) && !submit().disabled, said());
          submit().click();
          return wait(100).then(function () {
            var s = asked('snooze_sessions').pop();
            check('Snooze snoozes the session until then', !!s && s.keys.join() === session.key && s.until === later.getTime(), JSON.stringify(s));
            check('and closes', !dialog());
          });
        });
      });
    })
    .then(function () {
      window.__emit('reveal', { kind: 'snooze', keys: ['northwind/app#12'], reviews: true });
      return until(dialog, 3000, 'no dialog for a review');
    })
    .then(function () {
      check('a review’s names the PR', q('.ca-snoozedlg h2').textContent === 'Snooze your review of #12', q('.ca-snoozedlg h2').textContent);
      var until = new Date(field('date').value + 'T09:00').getTime();
      submit().click();
      return wait(100).then(function () {
        var s = asked('snooze_reviews').pop();
        check('and snoozes the review, not a session', !!s && s.keys.join() === 'northwind/app#12' && s.until === until, JSON.stringify(s));
      });
    })
    .then(done)
    .catch(stopped);
})();
