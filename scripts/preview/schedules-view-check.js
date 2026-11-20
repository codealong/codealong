// Checks the View menu over Schedules, as the other lists have one (SchedulesView.tsx
// `SCHEDULE_SORTS`, `SCHEDULE_GROUPS`, `SCHEDULE_ROWS`; schedules.ts `scheduleListing`), and that
// New schedule sits flush under the strip: each row on
// one line until two are picked; the ones that go on running by their next run, the ones with a
// time first, until Name sorts them by name; grouped by worktree, every schedule Codealong keeps
// under a heading, the ones in no worktree last in their section, the harnesses' own not grouped,
// the arrows going through the rows in the order they're drawn, past the headings, and a press on
// a worktree's heading showing its panel in the schedule's place, with the way back. Each choice
// is remembered, and the View button says what isn't at its default.
//   PREVIEW_WAIT=60000 PREVIEW_SCHEDULES=scripts/preview/schedules-sample.json scripts/preview.sh schedules '' scripts/preview/schedules-view-check.js
(function () {
  ['codealong.scheduleSort', 'codealong.scheduleGroup', 'codealong.scheduleRows'].forEach(function (k) { localStorage.removeItem(k); });
  var list = function () { return q('.ca-schedlist'); };
  var rows = function () { return all('.ca-schedrow', list()); };
  var name = function (r) { return q('.ca-schedrow-name', r).textContent; };
  // The ones that go on running: not under a section's heading (One-off, the harnesses', Finished).
  var going = function () { return rows().filter(function (r) { var b = r.closest('.ca-bayblock'); return !b || b.classList.contains('ca-pinned'); }); };
  var heads = function () { return all('.ca-grouphead', list()); };
  var choice = function (words, setting) { return viewChoice(words, setting); };
  var on = function (words, setting) { var b = viewChoice(words, setting); return !!b && b.getAttribute('aria-checked') === 'true'; };
  var viewSays = function () { return (q('.ca-viewmenu-btn') || {}).textContent || ''; };
  var press = function (el) {
    el.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, cancelable: true, button: 0, buttons: 1 }));
    el.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, cancelable: true, button: 0 }));
  };
  var selected = function () { return q('[role="option"][aria-selected="true"]', list()); };

  say('schedules view check: waiting for the schedules and the strip over them');
  until(function () { return rows().length >= 5 && q('.ca-board > .ca-listbar .ca-viewmenu-btn') && choice('Next run', 'Sort'); }, 20000, 'the schedules, with View over them')
    .then(function () {
      var bar = q('.ca-board > .ca-listbar');
      var add = q('.ca-schednew');
      var gap = Math.abs(add.getBoundingClientRect().top - bar.getBoundingClientRect().bottom);
      check('New schedule sits flush under the strip with no gap or second top line', gap <= 1 && getComputedStyle(add).borderTopWidth === '0px', 'gap=' + gap + ', borderTop=' + getComputedStyle(add).borderTopWidth);
      check('View has Sort, Group and Rows', viewChoices('Sort').join('|') === 'Next run|Last run|Added|Name' && viewChoices('Group').join('|') === 'No groups|Worktree|Repo' && viewChoices('Rows').join('|') === 'Two lines|One line', [viewChoices('Sort'), viewChoices('Group'), viewChoices('Rows')].join(' / '));
      check('each row is on one line until two are picked', on('One line', 'Rows') && rows().every(function (r) { return r.classList.contains('is-slim') && !q('.ca-lane-line2', r); }), rows().filter(function (r) { return !r.classList.contains('is-slim'); }).map(name).join(', '));
      var nexts = going().map(function (r) { var n = q('.ca-schedrow-next', r); return n ? n.textContent : ''; });
      var timed = nexts.map(function (n) { return /^(Next |Due now)/.test(n); });
      check('the ones that go on running come by their next run, the ones with a time first', on('Next run', 'Sort') && (timed.indexOf(false) < 0 || timed.lastIndexOf(true) < timed.indexOf(false)), going().map(name).map(function (n, i) { return n + ' (' + nexts[i] + ')'; }).join(', '));
      choice('Two lines', 'Rows').click();
      return until(function () { return rows().every(function (r) { return !r.classList.contains('is-slim') && q('.ca-lane-line2', r); }); }, 3000, 'two lines a row');
    })
    .then(function () {
      check('two lines a row once picked, remembered, and the View button says so', localStorage.getItem('codealong.scheduleRows') === 'two' && /two lines/.test(viewSays()), localStorage.getItem('codealong.scheduleRows') + ' / ' + viewSays());
      choice('Name', 'Sort').click();
      return until(function () { return localStorage.getItem('codealong.scheduleSort') === 'name' && going().length > 1; }, 3000, 'sorted by name');
    })
    .then(function () {
      var names = going().map(name);
      var sorted = names.slice().sort(function (a, b) { return a.localeCompare(b, undefined, { sensitivity: 'base', numeric: true }); });
      check('Name sorts them by name, and the View button says so', names.join('|') === sorted.join('|') && /by name/.test(viewSays()), names.join(', '));
      choice('Worktree', 'Group').click();
      return until(function () { return heads().length > 0; }, 3000, 'the worktrees’ headings');
    })
    .then(function () {
      check('Worktree groups them, and is remembered', on('Worktree', 'Group') && localStorage.getItem('codealong.scheduleGroup') === 'worktree' && /by worktree/.test(viewSays()), viewSays());
      var mine = rows().filter(function (r) { return !r.classList.contains('is-harness'); });
      var loose = mine.filter(function (r) { return !r.closest('.ca-group'); });
      check('every schedule Codealong keeps is under a heading', loose.length === 0, loose.map(name).join(', '));
      var theirs = rows().filter(function (r) { return r.classList.contains('is-harness'); });
      check('the harnesses’ own aren’t grouped', theirs.every(function (r) { return !r.closest('.ca-group'); }), theirs.length + ' of theirs');
      var none = heads().filter(function (h) { return h.classList.contains('is-none-head'); });
      if (none.length === 0) note('every schedule runs in a worktree here, so the heading over the rest went unchecked');
      else check('the ones in no worktree are under “No worktree”, last in their section', none.every(function (h) { var g = h.closest('.ca-group'); return h.textContent.trim() === 'No worktree' && !(g.nextElementSibling && g.nextElementSibling.classList.contains('ca-group')); }), none.map(function (h) { return h.textContent; }).join(', '));
      // The arrows go through the rows in the order they're drawn, each press read back on its own
      // render.
      var drawn = all('[role="option"]', list()).filter(function (el) { return !el.classList.contains('ca-grouphead'); }).map(function (el) { return el.id; });
      var seen = [];
      var walk = function (n) {
        return wait(100).then(function () {
          seen.push(selected() ? selected().id : '');
          if (n === 0) return { ids: seen, drawn: drawn };
          key(list(), { key: 'ArrowDown', code: 'ArrowDown' });
          return walk(n - 1);
        });
      };
      press(rows()[0]);
      return wait(200).then(function () {
        list().focus();
        return walk(Math.min(7, drawn.length - 1));
      });
    })
    .then(function (r) {
      check('the arrows go through the rows in the order they’re drawn, past the headings', r.ids.join('|') === r.drawn.slice(0, r.ids.length).join('|'), r.ids.join(', ') + ' / ' + r.drawn.slice(0, r.ids.length).join(', '));
      var h = heads().filter(function (x) { return x.classList.contains('is-worktree-head'); })[0];
      if (!h) {
        note('no schedule runs in a worktree here, so a heading’s press went unchecked');
        return null;
      }
      var wt = q('.ca-grouphead-name', h).textContent;
      // A schedule selected, not a session under one, whose own panel shows in its place.
      press(rows()[0]);
      return until(function () { return q('.ca-schedinsp'); }, 3000, 'a schedule’s panel')
        .then(function () {
          press(h);
          return until(function () { return q('.ca-laneinsp > .ca-reply-queue .ca-panelback.is-strip'); }, 3000, 'the worktree’s panel in the schedule’s place');
        })
        .then(function (back) {
          check('a press on a worktree’s heading shows its panel in the schedule’s place, with the way back on its strip', !q('.ca-schedinsp') && /details$/.test(q('.ca-laneinsp').getAttribute('aria-label') || ''), wt + ' / ' + back.textContent);
          back.click();
          return until(function () { return q('.ca-schedinsp'); }, 3000, 'the schedule’s panel again');
        })
        .then(function () {
          check('and the way back brings the schedule’s panel back', !q('.ca-laneinsp'));
          // Shown from a session under a schedule, the way back is to the session's panel.
          var nested = all('.ca-lane.is-nested[role="option"]', list())[0];
          if (!nested) return void note('no session under a schedule here, so a heading’s press from one went unchecked');
          press(nested);
          return until(function () { return q('.ca-sessinsp'); }, 3000, 'a session’s panel under its schedule')
            .then(function () {
              press(h);
              return until(function () { return q('.ca-laneinsp > .ca-reply-queue .ca-panelback.is-strip'); }, 3000, 'the worktree’s panel in the session’s place');
            })
            .then(function (back) {
              back.click();
              return until(function () { return q('.ca-sessinsp') && !q('.ca-laneinsp'); }, 3000, 'the session’s panel again');
            })
            .then(function () { check('from a session under a schedule, the way back is to the session’s panel', true); });
        });
    })
    .then(function () {
      // The press outside put the View menu away: it opens again as it's asked for.
      return until(function () { return choice('No groups', 'Group'); }, 3000, 'View open again');
    })
    .then(function (b) {
      b.click();
      return until(function () { return heads().length === 0; }, 3000, 'the list without groups');
    })
    .then(function () {
      check('No groups lists them alone again, and is remembered', localStorage.getItem('codealong.scheduleGroup') === 'none');
    })
    .then(done)
    .catch(stopped);
})();
