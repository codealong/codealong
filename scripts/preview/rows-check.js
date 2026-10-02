// Checks a board list across pushes, now that unchanged rows skip rendering (memoRow.ts). It
// works on a session's row: an agent's line on its worktree's row in Worktrees, a row of its own
// in Sessions, either way with the id `row-<lane id>::<session key>`, where a worktree without
// agent lines has its lane id alone. A push that changes the session shows on its row and renders
// that worktree's rows again while every other row, in the list and the sidebar, skips rendering;
// a todo linked later appears; a double-click opens where the latest push says the session lives;
// the arrow keys move the selection; and a hidden window catches up when it's shown.
//   PREVIEW_WAIT=120000 scripts/preview.sh worktrees '' scripts/preview/rows-check.js
//   PREVIEW_WAIT=120000 scripts/preview.sh sessions '' scripts/preview/rows-check.js
(function () {
  var hidden = false;
  Object.defineProperty(document, 'hidden', { configurable: true, get: function () { return hidden; } });

  // Which rows render. This script runs as the page is parsed, before the app's module script,
  // so React finds this devtools hook and reports every commit to it. A row memoRow made rendered
  // when its inner component holds props it didn't hold at the last commit: memoRow passes new
  // ones each time it lets the row render, and a row that skips keeps its last. A row whose `now`
  // changed rendered for the clock, which renders every row when it ticks.
  var rows = new Map();
  var kinds = new Map();
  var ticks = 0;
  var hookError = null;
  var countRenders = function (top) {
    var stack = [top];
    while (stack.length) {
      var f = stack.pop();
      var name = typeof f.type === 'function' ? String(f.type.displayName || '') : '';
      if (name.indexOf('memoRow(') === 0) {
        // The sidebar's rows have lane ids for keys too, so a row is known by its kind and key.
        if (!kinds.has(f.type)) kinds.set(f.type, kinds.size);
        var id = kinds.get(f.type) + ' ' + f.key;
        var r = rows.get(id);
        if (!r) rows.set(id, (r = { key: f.key, props: null, fiber: null, count: 0 }));
        // memoRow's row renders a memo of the component, and the memo the component itself.
        var inner = f.child && f.child.child;
        if (inner && inner.memoizedProps !== r.props) {
          if (r.props && r.props.now !== inner.memoizedProps.now) ticks++;
          r.props = inner.memoizedProps;
          r.fiber = inner;
          r.count++;
        }
        continue;
      }
      for (var c = f.child; c; c = c.sibling) stack.push(c);
    }
  };
  window.__REACT_DEVTOOLS_GLOBAL_HOOK__ = {
    supportsFiber: true,
    inject: function () { return 1; },
    onCommitFiberRoot: function (id, root) {
      try { countRenders(root.current); } catch (e) { hookError = hookError || e; }
    },
    onCommitFiberUnmount: function () {},
    onPostCommitFiberRoot: function () {},
    checkDCE: function () {},
  };
  var counts = function () {
    var m = new Map();
    rows.forEach(function (r, id) { m.set(id, r.count); });
    return m;
  };
  // A row's element: the first its component renders.
  var elementOf = function (r) {
    var f = r.fiber;
    while (f && typeof f.type !== 'string') f = f.child;
    return f ? f.stateNode : null;
  };
  // The rows that rendered since `counts()` said `from`, and where they are.
  var renderedSince = function (from) {
    var hit = [];
    rows.forEach(function (r, id) {
      if (r.count === (from.get(id) || 0)) return;
      var el = elementOf(r);
      hit.push({ key: r.key, place: !el || !el.closest ? '' : el.closest('.ca-board-list') ? 'list' : el.closest('.ca-side') ? 'sidebar' : '' });
    });
    return hit;
  };
  var shownRows = function () {
    var n = 0;
    rows.forEach(function (r) { var el = elementOf(r); if (el && el.isConnected) n++; });
    return n;
  };
  // The worktree a row belongs to, from its key, as App.tsx reads the selection's.
  var laneOf = function (key) { return String(key).split('::')[0]; };

  // The board as this check last pushed it. The page only ever gets copies, as IPC hands it one:
  // it keeps parts of what it's given, and a change made to those would never reach it as one.
  var base = null;
  var push = function (change) {
    change(base);
    window.__emit('board', JSON.parse(JSON.stringify(base)));
  };
  var laneIn = function (b, id) { return b.lanes.find(function (l) { return l.id === id; }); };
  var options = function () { return document.querySelectorAll('.ca-board-list [role="option"][id^="row-"]'); };
  // The first session row listed. A row's id is `row-` and its key, which for a session is
  // `<lane id>::<session key>` (worktreeRow in lanes.ts, the Sessions view's items in App.tsx).
  var pickRow = function () {
    var keys = {};
    base.lanes.forEach(function (l) {
      l.sessions.forEach(function (s) { if (!s.visitor) keys[l.id + '::' + s.key] = { lane: l.id, session: s.key }; });
    });
    var all = options();
    for (var i = 0; i < all.length; i++) {
      var hit = keys[all[i].id.slice(4)];
      if (hit) return { id: all[i].id, key: all[i].id.slice(4), lane: hit.lane, session: hit.session };
    }
    return null;
  };
  // Worktrees lists a session only while it needs you, works or has news. With none doing any of
  // that, the newest session of the first worktree listed is pushed as unread, which gives it a line.
  var made = false;
  var makeRow = function () {
    var all = options();
    for (var i = 0; i < all.length; i++) {
      var lane = laneIn(base, all[i].id.slice(4));
      var own = lane ? lane.sessions.filter(function (s) { return !s.visitor && !s.archived; }) : [];
      if (own.length === 0) continue;
      var newest = own.reduce(function (a, s) { return s.lastActivity > a.lastActivity ? s : a; });
      made = true;
      push(function () { newest.status = 'unread'; });
      return until(pickRow, 5000).catch(function () {});
    }
    return Promise.resolve();
  };
  var pick = null;
  var row = function () { return document.getElementById(pick.id); };
  var sessionIn = function (b) {
    return laneIn(b, pick.lane).sessions.find(function (s) { return s.key === pick.session && !s.visitor; });
  };

  // 1. A push that changes the session shows on its row and renders that worktree's rows alone:
  // every other row, in the list and the sidebar, skips rendering. The clock renders every row
  // when it ticks (every 15 s), so a push it ticked through is tried again.
  var target = null;
  var update = function (attempt) {
    // A session's row leads with its title, so a new title is what shows.
    var title = 'Pushed title ' + attempt;
    var from = counts();
    var ticked = ticks;
    target = 'rows-check-target-' + attempt;
    push(function (b) {
      var s = sessionIn(b);
      s.title = title;
      s.openTarget = target;
    });
    return until(function () { var el = row(); return el && el.textContent.indexOf(title) >= 0; }, 5000).then(
      function () {
        return wait(500).then(function () {
          if (ticks !== ticked && attempt < 5) return update(attempt + 1);
          check('update', true);
          var hit = renderedSince(from);
          var own = hit.filter(function (r) { return r.place === 'list' && (r.key === pick.key || r.key === pick.lane); });
          var others = hit.filter(function (r) { return laneOf(r.key) !== pick.lane; });
          if (hookError || rows.size === 0) {
            check('renders', false, hookError ? 'hook: ' + hookError.message : 'React reported no commits');
            return;
          }
          check('renders', own.length > 0);
          var why = hit.length + ' of ' + shownRows() + ' rows rendered';
          if (others.length > 0) {
            why += ', ' + others.length + ' of other worktrees: ' + others.slice(0, 3).map(function (r) { return r.place + ' ' + r.key; }).join(', ');
            if (ticks !== ticked) why += '; the clock ticked';
          }
          check('skips', others.length === 0, why);
        });
      },
      function () { check('update', false, 'the row never showed it'); },
    );
  };

  fetch('./board.json').then(function (r) { return r.json(); }).then(function (b) {
    base = b;
    return until(function () { return options().length > 0; }, 20000);
  }).then(function () {
    // What the app loads after the board (accounts, devices…) lands first.
    return wait(1000);
  }).then(function () {
    return pickRow() ? null : makeRow();
  }).then(function () {
    pick = pickRow();
    check('session row', !!pick, pick ? (made ? 'one pushed as unread' : '') : made ? 'one pushed as unread got no line' : 'none on the board');
    if (!pick) throw new Error('no session row');
    return update(1);
  }).then(function () {
    // 2. A todo linked to the session afterwards shows on its row. It goes first: a row shows its
    // first open todo, and the rest only in the tooltip of a "+1".
    var at = Date.now();
    push(function (b) {
      b.todos = [{ id: 'rows-check', project: laneIn(b, pick.lane).project, title: 'Linked todo check', description: '', status: 'todo', labels: [], worktrees: [], sessions: [pick.session], createdAt: at, updatedAt: at, doneAt: null, pending: [], dependsOn: [], waiting: null }].concat(b.todos || []);
    });
    return until(function () { var el = row(); return el && el.textContent.indexOf('Linked todo check') >= 0; }, 5000)
      .then(function () { check('todo chip', true); }, function () { check('todo chip', false); });
  }).then(function () {
    // 3. A double-click opens the session where the latest push says it lives.
    var n = window.__calls.length;
    row().dispatchEvent(new MouseEvent('dblclick', { bubbles: true, detail: 2 }));
    return wait(200).then(function () {
      var calls = window.__calls.slice(n).filter(function (c) { return c[0] === 'open_target'; });
      var sent = calls.length === 1 ? calls[0][1] : {};
      check('open', calls.length === 1 && sent.laneId === pick.lane && sent.targetId === target, calls.length === 1 ? '' : calls.length + ' opens');
    });
  }).then(function () {
    // 4. A mouse-down selects the row, and an arrow key moves the selection to another row.
    var list = document.querySelector('.ca-board-list');
    var all = options();
    var key = all[all.length - 1] === row() ? 'ArrowUp' : 'ArrowDown';
    row().dispatchEvent(new MouseEvent('mousedown', { bubbles: true, detail: 1 }));
    return wait(100).then(function () {
      var before = list.getAttribute('aria-activedescendant');
      window.dispatchEvent(new KeyboardEvent('keydown', { key: key, bubbles: true }));
      return wait(200).then(function () {
        var after = list.getAttribute('aria-activedescendant');
        var el = after && document.getElementById(after);
        check('arrow', before === pick.id && after !== before && !!el && el.getAttribute('aria-selected') === 'true');
      });
    });
  }).then(function () {
    // 5. A hidden window holds a push and shows it when it's seen again.
    hidden = true;
    push(function (b) { laneIn(b, pick.lane).callsign = 'Seen when shown'; });
    // The worktree's name shows beside its agents' lines in Worktrees and in a chip on the row in
    // Sessions: either way inside the row's `.ca-lane`.
    var shows = function () { var el = row(); var lane = el && el.closest('.ca-lane'); return !!lane && lane.textContent.indexOf('Seen when shown') >= 0; };
    return wait(400).then(function () {
      check('held while hidden', document.body.textContent.indexOf('Seen when shown') < 0);
      hidden = false;
      document.dispatchEvent(new Event('visibilitychange'));
      return until(shows, 3000).then(function () { check('shown', true); }, function () {
        check('shown', false);
        push(function () {});
        return until(shows, 3000).then(function () { check('shown on a later push', true); }, function () { check('shown on a later push', false); });
      });
    });
  }).then(done, stopped);
})();
