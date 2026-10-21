// Checks Group by in Sessions (SessionsView.tsx `SessionsBar`, `groupSessions`; LaneRow.tsx
// `WorktreeHead`, `RepoHead`, `GroupFoldRow`): its three buttons in the bar over the list, No
// groups, Worktree and Repo, switch the list and the choice is remembered. Grouped by worktree,
// each worktree's heading is listed once, a row of its own, with its sessions after it and no
// worktree chip on their rows; a click on a heading selects it, with the worktree's panel beside
// the list; ⌘-click on another picks both, in the worktrees' pick panel, and a click on its PR
// chip then shows its PR in that panel's place; a reusable worktree's
// heading says whether it can take new work, and only a reusable one's does. A group of more than
// five sessions lists five and folds the rest, which its fold row lists, and folds again. Grouped
// by repo, each repo heads its sessions, a heading that isn't a row to select, and the rows say
// their worktrees. A worktree shown from elsewhere with no heading listed (App.tsx `focusLane`)
// is its panel in place of the row selected, with the way back; from another project too, once
// the window has gone to that project (`laneAsked`), and in a project with no session listed, its
// panel alone.
//   PREVIEW_WAIT=60000 scripts/preview.sh sessions '' scripts/preview/group-by-check.js
// The check gives the latest worktree with a session six more, made from that session, so it has
// a group to fold, and adds a worktree with no sessions beside it, in another project when there
// is one, and another in a project of its own with nothing else in it.
(function () {
  localStorage.setItem('codealong.project', 'All projects');
  localStorage.setItem('codealong.view', 'sessions');
  localStorage.removeItem('codealong.sessionGroup');
  var EXTRA = 6;
  var board = null;
  var folding = null;
  var empty = null;
  var alone = null;
  var NOTHING = 'Group by nothing';
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    var p = real(cmd, args, options);
    if (cmd !== 'get_board') return p;
    return p.then(function (b) {
      var own = function (l) { return l.sessions.filter(function (s) { return !s.visitor && !s.archived; }); };
      if (!folding) {
        var lane = b.lanes.filter(function (l) { return !l.hidden && !l.device && own(l).length > 0; }).sort(function (x, y) { return y.lastActivity - x.lastActivity; })[0];
        if (lane) folding = lane.id;
      }
      var at = b.lanes.find(function (l) { return l.id === folding; });
      if (at && !at.sessions.some(function (s) { return /-group-by-\d$/.test(s.key); })) {
        var from = own(at).sort(function (x, y) { return y.lastActivity - x.lastActivity; })[0];
        for (var i = 0; i < EXTRA; i++) {
          var s = JSON.parse(JSON.stringify(from));
          s.key = from.key + '-group-by-' + i;
          s.id = from.id + '-group-by-' + i;
          s.title = 'Group by check ' + i;
          s.status = 'idle';
          s.live = false;
          s.snoozedUntil = undefined;
          s.helper = undefined;
          s.headless = undefined;
          s.origin = undefined;
          s.lastActivity = from.lastActivity - (i + 1) * 60e3;
          s.lastPromptAt = s.lastActivity;
          at.sessions.push(s);
        }
      }
      if (at && !b.lanes.some(function (l) { return l.id === at.id + '-group-by-empty'; })) {
        var bare = JSON.parse(JSON.stringify(at));
        // Not the first project, which ⌥⌘1 shows, so it's shown from another.
        var elsewhere = b.projects.filter(function (x, i) { return i > 0; })[0];
        bare.id = at.id + '-group-by-empty';
        bare.path = bare.id;
        bare.callsign = 'group-by-empty';
        bare.sessions = [];
        bare.pr = null;
        bare.reuse = undefined;
        bare.preview = undefined;
        bare.project = elsewhere ? elsewhere.name : at.project;
        b.lanes.push(bare);
        empty = bare;
        // A project with nothing in it but a worktree with no sessions: not the first, nor after
        // Elsewhere.
        var end = b.projects.length > 0 && b.projects[b.projects.length - 1].name === 'Elsewhere' ? b.projects.length - 1 : b.projects.length;
        b.projects.splice(end, 0, { name: NOTHING, lanes: 1, repos: [] });
        var lone = JSON.parse(JSON.stringify(bare));
        lone.id = at.id + '-group-by-nothing';
        lone.path = lone.id;
        lone.callsign = 'group-by-nothing';
        lone.project = NOTHING;
        b.lanes.push(lone);
        alone = lone;
      }
      board = b;
      return b;
    });
  };

  var button = function (label) { return q('.ca-listtools [role="radiogroup"][aria-label="Group by"] button[aria-label="' + label + '"]'); };
  var on = function (label) { var b = button(label); return !!b && b.classList.contains('is-on') && b.getAttribute('aria-checked') === 'true'; };
  var heads = function () { return all('.ca-sessionlist .ca-grouphead.is-worktree-head'); };
  var repoHeads = function () { return all('.ca-sessionlist .ca-grouphead.is-repo-head'); };
  var laneOf = function (id) { return board && board.lanes.find(function (l) { return l.id === id; }); };
  var groupOf = function (laneId) { var h = document.getElementById('row-' + laneId); return h && h.closest('.ca-group'); };
  // A group's sessions: its rows but its folds, and the ones listed at its top level.
  var sessionsIn = function (g) { return all('.ca-lane.is-agent:not(.is-fold)', g); };
  var topIn = function (g) { return sessionsIn(g).filter(function (r) { return !r.classList.contains('is-nested'); }); };
  var press = function (el, init) { el.dispatchEvent(new MouseEvent('mousedown', Object.assign({ bubbles: true, cancelable: true, button: 0 }, init))); };
  var keyDown = function (init) { (document.activeElement || document.body).dispatchEvent(new KeyboardEvent('keydown', Object.assign({ bubbles: true, cancelable: true }, init))); };
  var selectedId = function () { var s = q('.ca-board-list [role="option"][aria-selected="true"]'); return s ? s.id : ''; };

  say('group by check: waiting for Sessions');
  until(function () { return board && button('No groups') && q('.ca-sessionlist .ca-lane.is-agent'); }, 20000, 'Sessions with Group by in its bar')
    .then(function () { return wait(300); })
    .then(function () {
      var labels = all('.ca-listtools [role="radiogroup"][aria-label="Group by"] button').map(function (b) { return b.getAttribute('aria-label'); });
      check('Group by has its three buttons', labels.join('|') === 'No groups|Worktree|Repo', labels.join('|'));
      check('each says what it does', ['No groups', 'Worktree', 'Repo'].every(function (l) { return button(l) && button(l).title.length > 10; }));
      check('the list starts with no groups', on('No groups') && heads().length === 0 && repoHeads().length === 0);
      button('Worktree').click();
      return until(function () { return heads().length > 0; }, 3000, 'the worktrees’ headings');
    })
    .then(function () {
      check('Worktree groups the list, and is remembered', on('Worktree') && localStorage.getItem('codealong.sessionGroup') === 'worktree', localStorage.getItem('codealong.sessionGroup'));
      var ids = heads().map(function (h) { return h.id; });
      var once = ids.filter(function (id, i) { return ids.indexOf(id) === i; });
      check('each worktree’s heading is listed once', once.length === ids.length && ids.length > 0, ids.length + ' headings, ' + once.length + ' worktrees');
      check('a heading is a row, by its worktree’s id', heads().every(function (h) { return h.getAttribute('role') === 'option' && !!laneOf(h.id.slice(4)); }));
      // Its sessions follow it: rows of that worktree, or ones nested under them from elsewhere.
      var strays = [];
      var chipped = [];
      heads().forEach(function (h) {
        var lane = h.id.slice(4);
        sessionsIn(h.closest('.ca-group')).forEach(function (r) {
          var nested = r.classList.contains('is-nested');
          if (!nested && r.id.indexOf('row-' + lane + '::') !== 0) strays.push(r.id);
          if (!nested && r.querySelector('.ca-wtchip')) chipped.push(r.id);
        });
      });
      check('its sessions follow it', strays.length === 0, strays.slice(0, 3).join(' | '));
      check('with no worktree chip on their rows', chipped.length === 0, chipped.slice(0, 3).join(' | '));
      // Every session listed is under a heading, but for the pinned sessions, which come first.
      var loose = all('.ca-sessionlist .ca-lane.is-agent:not(.is-fold)').filter(function (r) { return !r.closest('.ca-group') && !r.classList.contains('is-pinned') && !r.closest('.ca-pinned'); });
      check('every other session is under its worktree’s heading', loose.length === 0, loose.slice(0, 3).map(function (r) { return r.id; }).join(' | '));

      // Reusable worktrees say whether new work can start there.
      var words = { free: 'Free', landed: 'Landed', open: 'Reusable', review: 'Reusable' };
      var wrong = heads().filter(function (h) {
        var lane = laneOf(h.id.slice(4));
        var chip = q('.ca-reusechip', h);
        return lane.reuse ? !chip || chip.textContent !== words[lane.reuse] || (lane.reuse === 'free') !== chip.classList.contains('is-ready') : !!chip;
      });
      var reusable = heads().filter(function (h) { return !!laneOf(h.id.slice(4)).reuse; }).length;
      if (reusable === 0) note('no reusable worktree listed, so only the chips’ absence was checked');
      check('a reusable worktree’s heading says whether it takes new work, and only theirs', wrong.length === 0, wrong.slice(0, 3).map(function (h) { return h.id; }).join(' | '));

      // A click on a heading: it's selected, and the panel is the worktree's.
      var h = heads().filter(function (x) { return x.getAttribute('aria-selected') !== 'true'; })[0];
      press(h);
      return wait(300).then(function () { return h; });
    })
    .then(function (h) {
      var lane = laneOf(h.id.slice(4));
      var panel = q('.ca-insp:not(.ca-sessinsp)');
      check('a click on a heading selects it', selectedId() === h.id && h.classList.contains('is-selected'), selectedId());
      check('with the worktree’s panel beside the list', !!panel && panel.getAttribute('aria-label') === lane.callsign + ' details', panel ? panel.getAttribute('aria-label') : 'no worktree panel');
      // Another, one with a PR when there's one, for its chip below.
      var others = heads().filter(function (x) { return x !== h; });
      var other = others.filter(function (x) { return !!q('.ca-pr', x); })[0] || others[0];
      if (!other) return void note('one worktree listed, so picking two went unchecked');
      press(other, { metaKey: true });
      return until(function () { return q('.ca-pick'); }, 3000, 'the pick panel').then(function (pick) {
        check('⌘-click on another heading picks both worktrees', pick.getAttribute('aria-label') === 'Selected worktrees' && q('.ca-pick-head').textContent === '2 worktrees', pick.getAttribute('aria-label') + ': ' + q('.ca-pick-head').textContent);
        // A click on the PR chip of the one picked last shows its PR, not the pick panel.
        var chip = q('.ca-pr', other);
        if (!chip) {
          note('no heading with a PR, so its chip on a pick went unchecked');
          keyDown({ key: 'Escape', code: 'Escape' });
          return wait(150);
        }
        press(chip);
        chip.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true, detail: 1 }));
        return until(function () { return q('.ca-prpanel'); }, 3000, 'the PR’s panel').then(function () {
          check('a click on a picked heading’s PR chip shows its PR in place of the pick panel', !q('.ca-pick') && selectedId() === other.id, selectedId());
        });
      });
    })
    .then(function () {
      // The worktree given more sessions than a group lists.
      var g = groupOf(folding);
      if (!g) throw new Error('no group for the worktree the check gave more sessions');
      var fold = q('.ca-groupfold button', g);
      var before = sessionsIn(g).length;
      var tops = topIn(g).length;
      var n = fold && Number((fold.textContent.match(/^Show (\d+) more$/) || [])[1]);
      check('a group of more than five sessions lists five, and says how many more it folds', !!fold && n > 0 && tops === 5, fold ? fold.textContent + ' with ' + tops + ' listed' : 'no fold row, ' + tops + ' listed');
      if (!fold) return;
      check('its fold row isn’t a row to select', !q('.ca-groupfold', g).getAttribute('role') || q('.ca-groupfold', g).getAttribute('role') === 'presentation');
      fold.click();
      return until(function () { return sessionsIn(groupOf(folding)).length === before + n; }, 3000, 'the rest listed').then(function () {
        var again = q('.ca-groupfold button', groupOf(folding));
        check('a click lists the rest', true);
        check('and then folds them again', !!again && again.textContent === 'Show fewer', again ? again.textContent : 'no fold row');
        again.click();
        return until(function () { return sessionsIn(groupOf(folding)).length === before; }, 3000, 'folded again').then(function () { check('which it does', true); });
      });
    })
    .then(function () {
      button('Repo').click();
      return until(function () { return repoHeads().length > 0; }, 3000, 'the repos’ headings');
    })
    .then(function () {
      check('Repo groups the list by repo, and is remembered', on('Repo') && heads().length === 0 && localStorage.getItem('codealong.sessionGroup') === 'repo', localStorage.getItem('codealong.sessionGroup'));
      var r = repoHeads()[0];
      check('a repo’s heading is a heading, not a row to select', r.getAttribute('role') === 'heading' && !r.hasAttribute('aria-selected') && !r.id, r.getAttribute('role'));
      var before = selectedId();
      press(r);
      return wait(200).then(function () {
        check('and a click on it selects nothing', selectedId() === before, selectedId() + ' was ' + before);
        var rows = sessionsIn(r.closest('.ca-group')).filter(function (x) { return !x.classList.contains('is-nested'); });
        check('its sessions say their worktrees', rows.length > 0 && rows.every(function (x) { return !!x.querySelector('.ca-wtchip'); }), rows.length + ' rows');
        return menuOf(r);
      });
    })
    .then(function (labels) {
      check('its menu pins it', labels.indexOf('Pin repo') >= 0 || labels.indexOf('Unpin repo') >= 0, labels.join(' | '));
      button('No groups').click();
      return until(function () { return heads().length === 0 && repoHeads().length === 0; }, 3000, 'the list without groups');
    })
    .then(function () {
      check('No groups lists the sessions alone again, and is remembered', on('No groups') && localStorage.getItem('codealong.sessionGroup') === 'none', localStorage.getItem('codealong.sessionGroup'));
      // A worktree with no heading listed, shown from elsewhere: its panel in the selected row's place.
      if (!empty) throw new Error('no worktree with no sessions to show');
      var row = selectedId();
      window.__emit('reveal', { kind: 'worktree', laneId: empty.id });
      return until(function () { var p = q('.ca-insp:not(.ca-sessinsp)'); return p && p.getAttribute('aria-label') === 'group-by-empty details' && p; }, 3000, 'its panel').then(function (p) {
        check('a worktree with no heading shows its panel in the selected row’s place', selectedId() === row && !!q('.ca-panelback', p), selectedId() + ' was ' + row);
      });
    })
    .then(function () {
      // From another project, once the window has gone there: ⌥⌘1 shows the first project.
      keyDown({ key: '1', code: 'Digit1', metaKey: true, altKey: true });
      return until(function () { return localStorage.getItem('codealong.project') !== 'All projects' && !q('.ca-insp:not(.ca-sessinsp)'); }, 3000, 'a project of its own, with a session’s panel')
        .then(function () {
          if (localStorage.getItem('codealong.project') === empty.project) {
            note('one project, so showing a worktree from another went unchecked');
            return null;
          }
          window.__emit('reveal', { kind: 'worktree', laneId: empty.id });
          return until(function () { var p = q('.ca-insp:not(.ca-sessinsp)'); return localStorage.getItem('codealong.project') === empty.project && p && p.getAttribute('aria-label') === 'group-by-empty details' && p; }, 3000, 'its project, and its panel');
        })
        .then(function (p) {
          if (p) check('from another project, the window goes there and shows its panel, with the way back', !!q('.ca-panelback', p) && !!selectedId(), selectedId());
        });
    })
    .then(function () {
      // To a project with no session listed: its panel alone, with no row to show it from.
      window.__emit('reveal', { kind: 'worktree', laneId: alone.id });
      return until(function () { var p = q('.ca-insp:not(.ca-sessinsp)'); return localStorage.getItem('codealong.project') === NOTHING && p && p.getAttribute('aria-label') === 'group-by-nothing details' && p; }, 3000, 'its project, and its panel');
    })
    .then(function () {
      check('in a project with no session listed, it shows its panel alone', !q('.ca-board-list .ca-lane.is-agent') && !selectedId(), selectedId());
    })
    .then(done)
    .catch(stopped);
})();
