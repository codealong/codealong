// Checks Pick a time… in the main window (ui/SnoozeDialog.tsx): asked for from a Snooze menu in any
// window (snooze.ts `timesMenu`, whose asking from the bar snooze-check.js checks), it names what it
// snoozes, opens on the next morning, says when it would come back or that the time has passed,
// and snoozes sessions, or your review of a PR, until the day and time picked; and the window it
// was asked from goes on to the next that waits on you, as a time from the menu does (ahead.ts
// `snoozedFrom`). Two sessions of the board wait on you here, Queue A and Queue B, and no other.
//   scripts/preview.sh sessions '' scripts/preview/snooze-pick-check.js
(function () {
  localStorage.setItem('codealong.age', 'all');
  localStorage.setItem('codealong.project', 'All projects');
  var now = Date.now();
  var picked = [];
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    var p = real(cmd, args, options);
    if (cmd !== 'get_board') return p;
    return p.then(function (board) {
      var seen = {};
      board.lanes.forEach(function (l) {
        l.sessions.forEach(function (s) {
          if (s.status === 'needs' || s.status === 'error' || s.status === 'unread') s.status = 'idle';
          s.snoozedUntil = undefined;
          if (l.hidden || l.device || s.visitor || s.archived || seen[s.key] || picked.length >= 2) return;
          seen[s.key] = true;
          s.status = 'unread';
          s.statusSince = now - (10 - picked.length * 5) * 60e3;
          s.lastActivity = s.statusSince;
          s.asks = false;
          s.hosted = false;
          s.stoppable = false;
          s.title = 'Queue ' + 'AB'[picked.length];
          picked.push({ key: s.key, lane: l.id });
        });
      });
      return board;
    });
  };

  var title = function () { var t = q('.ca-sessinsp-title'); return t ? t.textContent : null; };
  var row = function (i) { return document.getElementById('row-' + picked[i].lane + '::' + picked[i].key); };
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

  say('snooze pick check: waiting for the list');
  until(function () { return picked.length === 2 && row(0) && row(1); }, 20000, 'no two sessions of this Mac to wait on you')
    .then(function () {
      row(0).click();
      return until(function () { return title() === 'Queue A'; }, 3000, 'Queue A never showed');
    })
    .then(function () { return menuOf(row(0)); })
    .then(function (labels) {
      check('its Snooze menu ends with Pick a time…', labels.indexOf('Snooze › Pick a time…') > 0, labels.join(' | '));
      menuItem('Snooze › Pick a time…')();
      return wait(100);
    })
    .then(function () {
      var reveal = (asked('show_main').pop() || {}).reveal;
      check('which asks this window, as any window asks it', !!reveal && reveal.kind === 'snooze' && reveal.keys.join() === picked[0].key && reveal.reviews === false, JSON.stringify(reveal));
      // What the engine does for show_main: the main window hears what it's to show.
      window.__emit('reveal', reveal);
      return until(dialog, 3000, 'no dialog for Pick a time…');
    })
    .then(function () {
      var now = new Date();
      var morning = new Date(now.getFullYear(), now.getMonth(), now.getDate() + (now.getHours() < 5 ? 0 : 1), 9);
      check('it names the session', q('.ca-snoozedlg h2').textContent === 'Snooze “Queue A”', q('.ca-snoozedlg h2').textContent);
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
            check('Snooze snoozes the session until then', !!s && s.keys.join() === picked[0].key && s.until === later.getTime(), JSON.stringify(s));
            check('and closes', !dialog());
            return until(function () { return title() === 'Queue B'; }, 3000).catch(function () {});
          });
        });
      });
    })
    .then(function () {
      check('and the panel goes on to the next that waits on you, as for a time from the menu', title() === 'Queue B', title());
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
        check('leaving the session shown as it was', title() === 'Queue B', title());
      });
    })
    .then(done)
    .catch(stopped);
})();
