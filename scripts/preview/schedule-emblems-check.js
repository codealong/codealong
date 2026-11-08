// Checks a schedule's emblem (emblems.rs, SchedulesView.tsx, SchedulePanel.tsx): a schedule that
// starts sessions shows the emblem its sessions take before its name on its row, and larger in its
// panel; a session under it shows the same one; and one that only sends to a session shows none.
//   PREVIEW_SCHEDULES=scripts/preview/schedules-sample.json scripts/preview.sh schedules '' scripts/preview/schedule-emblems-check.js
(function () {
  var EMBLEM = { emoji: '🦉', name: 'owl' };
  var invoke = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    var p = invoke(cmd, args, options);
    if (cmd !== 'get_board') return p;
    return p.then(function (b) {
      (b.schedules || []).forEach(function (s) {
        if (s.run.to === 'session') delete s.emblem;
        else s.emblem = EMBLEM;
      });
      var theirs = {};
      (b.schedules || []).filter(function (s) { return s.run.to !== 'session'; }).forEach(function (s) {
        s.runs.forEach(function (r) { if (r.session) theirs[r.session] = true; });
      });
      b.lanes.forEach(function (l) {
        l.sessions.forEach(function (s) {
          if (theirs[s.key]) s.emblem = EMBLEM;
          else delete s.emblem;
        });
      });
      return b;
    });
  };

  var rowOf = function (name) {
    return all('.ca-schedrow').filter(function (r) { return r.querySelector('.ca-schedrow-name').textContent === name; })[0];
  };
  var run = async function () {
    await until(function () { return !!rowOf('Morning triage'); }, 20000, 'the schedules');
    await wait(300);
    var row = rowOf('Morning triage');
    var emblem = row.querySelector('.ca-lane-line1 > .ca-emblem');
    check('a schedule that starts sessions shows its emblem', !!emblem && emblem.textContent === '🦉' && emblem.title === 'owl', emblem && emblem.outerHTML);
    check('before its name', !!emblem && emblem.nextElementSibling.classList.contains('ca-schedrow-name'));
    check('the size a session’s row has', !!emblem && emblem.getBoundingClientRect().width === 16);
    var nested = all('.ca-lane.is-nested .ca-agentline-text > .ca-emblem');
    check('a session under it shows the same one', nested.length > 0 && nested.every(function (e) { return e.textContent === '🦉'; }), nested.length + ' sessions');
    var sends = all('.ca-schedrow').filter(function (r) { return /Sends? to|to “/.test(r.querySelector('.ca-schedrow-does').textContent); })[0];
    if (sends) check('one that sends to a session shows none', !sends.querySelector('.ca-lane-line1 > .ca-emblem'), sends.textContent);
    row.click();
    await until(function () { return !!q('.ca-schedinsp-name .ca-emblem'); }, 5000, 'its panel’s emblem');
    var big = q('.ca-schedinsp-name .ca-emblem');
    check('its panel shows it larger, before its name', big.getBoundingClientRect().width === 20 && big.nextElementSibling.textContent === 'Morning triage', q('.ca-schedinsp-name').outerHTML);
    check('on the name’s line', Math.abs(big.getBoundingClientRect().top + 10 - (big.nextElementSibling.getBoundingClientRect().top + big.nextElementSibling.getBoundingClientRect().height / 2)) <= 2);
    done();
  };
  setTimeout(function () { run().catch(stopped); }, 1500);
})();
