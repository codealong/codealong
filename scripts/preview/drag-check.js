// Checks dragging rows in the lists (useRowDrag.tsx): a row dragged up among the pinned ones is
// pinned there, a pinned one dragged down to the list is unpinned, the drag tells the engine what
// its card would be (drag_to_desktop, which pins it once the pointer leaves the window), and a
// pinned row let go on the desktop leaves the list's pins. Grouped by worktree, a worktree's
// heading is dragged the same way among the pinned worktrees. Todos reorder as before and go to
// the desktop too.
//   PREVIEW_WAIT=60000 scripts/preview.sh sessions '' scripts/preview/drag-check.js
//   PREVIEW_WAIT=60000 PREVIEW_GROUP=worktree scripts/preview.sh sessions '' scripts/preview/drag-check.js
//   PREVIEW_WAIT=60000 scripts/preview.sh prs '' scripts/preview/drag-check.js
//   PREVIEW_WAIT=60000 PREVIEW_TODOS=scripts/preview/todos-sample.json scripts/preview.sh todo '' scripts/preview/drag-check.js
// Sessions, worktrees' headings and PRs need three rows or more. The pins are the check's, whatever
// this Mac has pinned.
(function () {
  // The pins the board comes with (as many as you keep) can push the rest of the list below the
  // window, where a drop lands on the desktop, so the check starts from its own: in Sessions, and
  // grouped by worktree, the shown project's latest session and worktree, so the first drop lands among pins
  // and the last drag has a pinned row; in PRs none, so the first drop pins the first. This runs
  // before the app's script, so the first board the page reads has them.
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    var p = real(cmd, args, options);
    if (cmd !== 'get_board') return p;
    return p.then(function (b) {
      // The project the window shows, as boardLists.ts `currentProject` picks it.
      var picked = localStorage.getItem('codealong.project');
      var project = picked === 'All projects' || b.projects.some(function (x) { return x.name === picked; }) ? picked : (b.projects.find(function (x) { return x.name !== 'Elsewhere'; }) || { name: 'All projects' }).name;
      var lanes = b.lanes.filter(function (l) { return !l.hidden && !l.device && (project === 'All projects' || l.project === project); });
      var latest = function (list) { return list.slice().sort(function (x, y) { return y.lastActivity - x.lastActivity; })[0]; };
      var lane = latest(lanes);
      var session = latest([].concat.apply([], lanes.map(function (l) { return l.sessions.filter(function (x) { return !x.visitor && !x.archived; }); })));
      return Object.assign(b, { pinned: session ? [session.key] : [], pinnedWorktrees: lane ? [lane.id] : [], pinnedPrs: [] });
    });
  };
  // Calls since the last `mark()`. The stub finds listeners in __calls, so it's never emptied.
  var from = 0;
  var mark = function () { from = window.__calls.length; };
  var calls = function (cmd) { return window.__calls.slice(from).filter(function (c) { return c[0] === cmd; }); };
  var last = function (cmd) { var c = calls(cmd); return c.length ? c[c.length - 1][1] : null; };
  var view = localStorage.getItem('codealong.view') || 'sessions';
  // Sessions grouped by worktree drags the worktrees' headings.
  if (view === 'sessions' && localStorage.getItem('codealong.sessionGroup') === 'worktree') view = 'worktrees';
  var pointer = function (target, type, x, y) {
    target.dispatchEvent(new PointerEvent(type, { bubbles: true, cancelable: true, button: 0, buttons: type === 'pointerup' ? 0 : 1, clientX: x, clientY: y, pointerId: 1, isPrimary: true, pointerType: 'mouse' }));
  };
  var mid = function (el) { var r = el.getBoundingClientRect(); return { x: r.left + r.width / 2, y: r.top + r.height / 2, top: r.top, bottom: r.bottom, h: r.height }; };
  var ghost = function () { return document.querySelector('.ca-dragghost'); };
  /** Presses on `el`, moves through `points`, and lets go at the last unless `hold`. */
  var drag = function (el, points, hold) {
    var m = mid(el);
    pointer(el, 'pointerdown', m.x, m.top + 8);
    points.forEach(function (p) { pointer(window, 'pointermove', p.x == null ? m.x : p.x, p.y); });
    var end = points[points.length - 1];
    if (!hold) pointer(window, 'pointerup', end.x == null ? m.x : end.x, end.y);
  };

  var lists = function () {
    // The rows that can be pinned, the pinned ones among them, the command that saves the pins
    // and what a row's key is.
    if (view === 'sessions') {
      return {
        rows: function () { return all('.ca-sessionlist .ca-lane'); },
        key: function (r) { return r.id.split('::').pop(); },
        save: 'set_pinned', arg: 'keys', ref: 'session',
      };
    }
    if (view === 'prs') {
      return {
        rows: function () { return all('.ca-board-list > .ca-bayblock .ca-prrow'); },
        key: null,
        save: 'set_pinned_prs', arg: 'keys', ref: 'pr',
      };
    }
    return {
      rows: function () { return all('.ca-sessionlist .ca-grouphead.is-worktree-head'); },
      key: null,
      save: 'set_pinned_worktrees', arg: 'ids', ref: 'worktree',
    };
  };

  var pins = function () {
    var L = lists();
    var saved = function () { var s = last(L.save); return s ? s[L.arg] : null; };
    var pinned = function () { return L.rows().filter(function (r) { return r.hasAttribute('data-pin-index'); }); };
    var loose = function () { return L.rows().filter(function (r) { return !r.hasAttribute('data-pin-index'); }); };
    var before, key;
    return until(function () { return L.rows().length >= 3; }, 20000)
      .then(function () {
        check('every row can be dragged', L.rows().every(function (r) { return !!r.querySelector(':scope > .ca-grip'); }));
        // A press that barely moves is a click.
        var row = loose()[1] || loose()[0];
        var m = mid(row);
        pointer(row, 'pointerdown', m.x, m.y);
        pointer(window, 'pointermove', m.x + 1, m.y + 1);
        pointer(window, 'pointerup', m.x + 1, m.y + 1);
        check('a press that barely moves drags nothing', calls('drag_to_desktop').length === 0 && calls(L.save).length === 0 && !ghost());

        // Dragged to the top of the list, a row is pinned first.
        before = pinned().length;
        var top = (pinned()[0] || L.rows()[0]).getBoundingClientRect();
        var into = before > 0 ? top.top + 4 : top.top - 6;
        drag(row, [{ y: m.y - 20 }, { y: into }], true);
        return wait(50).then(function () {
          var d = last('drag_to_desktop');
          key = d && d.pin.key;
          check('a drag tells the engine what its card would be', calls('drag_to_desktop').length === 1 && d && d.pin.ref === L.ref && !!key, JSON.stringify(d));
          check('the pointer holds the row and says Pin', !!ghost() && /Pin/.test(ghost().textContent) && ghost().textContent.length > 3, ghost() && ghost().textContent);
          check('a line shows where it lands', before > 0 ? pinned()[0].classList.contains('is-drop-before') : !!document.querySelector('.ca-dropline'));
          check('the row dims', row.classList.contains('is-drag-source'));
          check('and the pointer grabs', document.documentElement.classList.contains('ca-dragging'));
          pointer(window, 'pointerup', m.x, into);
          return wait(50);
        });
      })
      .then(function () {
        var s = saved();
        check('dropped there, it is pinned first', s && s[0] === key, JSON.stringify(s && s.slice(0, 3)));
        check('and the drag is over', !ghost() && !document.documentElement.classList.contains('ca-dragging') && !document.querySelector('.is-drag-source'));
        return until(function () { return pinned().length === before + 1; }, 2000);
      })
      .then(function () {
        check('the list shows it pinned', pinned()[0].hasAttribute('data-pin-index'));
        // Dragged after the last pinned row, another is pinned last.
        var row = loose()[1] || loose()[0];
        var end = pinned()[pinned().length - 1].getBoundingClientRect();
        mark();
        drag(row, [{ y: mid(row).y - 10 }, { y: end.bottom - 3 }]);
        return wait(50).then(function () {
          var s = saved();
          var d = last('drag_to_desktop');
          check('a row dropped under the pins is pinned last', s && d && s.indexOf(d.pin.key) === s.indexOf(key) + 1 || (s && d && s[s.length - 1] === d.pin.key), JSON.stringify(s && s.slice(0, 4)));
          return until(function () { return pinned().length === before + 2; }, 2000);
        });
      })
      .then(function () {
        // A pinned row dragged down to the list is unpinned: onto a row below the pins, or as far
        // down the window as the list goes, where a long group puts the rows below it.
        var row = pinned()[0];
        var target = loose()[Math.min(2, loose().length - 1)];
        var down = Math.min(mid(target).y, window.innerHeight - 10);
        mark();
        drag(row, [{ y: mid(row).y + 10 }, { y: down }], true);
        return wait(50).then(function () {
          check('over the list a pinned row says Unpin', !!ghost() && /Unpin/.test(ghost().textContent), ghost() && ghost().textContent);
          check('with no line', !document.querySelector('.is-drop-before, .is-drop-after, .ca-dropline'));
          // The engine says when the pointer has left the window, which the page may not hear of.
          window.__emit('desktop-drag-away', true);
          return wait(50);
        }).then(function () {
          check('the engine saying the pointer left puts away what it holds', getComputedStyle(ghost()).display === 'none');
          window.__emit('desktop-drag-away', false);
          return wait(50);
        }).then(function () {
          check('and saying it is back shows it again', getComputedStyle(ghost()).display !== 'none' && /Unpin/.test(ghost().textContent));
          pointer(window, 'pointerup', mid(row).x, down);
          return wait(50);
        }).then(function () {
          var s = saved();
          check('dropped there, it is unpinned', s && s.indexOf(key) < 0, JSON.stringify(s && s.slice(0, 3)));
          return until(function () { return pinned().length === before + 1; }, 2000);
        });
      })
      .then(function () {
        // Off the list but in the window (the sidebar): nothing.
        var row = pinned()[0];
        mark();
        drag(row, [{ y: mid(row).y + 10 }, { x: 20, y: mid(row).y + 40 }]);
        return wait(50).then(function () {
          check('dropped elsewhere in the window, nothing changes', calls(L.save).length === 0);
        });
      })
      .then(function () {
        // Out of the window: the engine's card takes over, and a pinned row leaves the pins
        // once the engine says the card stayed.
        var row = pinned()[0];
        var k = null;
        mark();
        drag(row, [{ y: mid(row).y + 10 }, { x: -40, y: mid(row).y }], true);
        return wait(50).then(function () {
          k = last('drag_to_desktop').pin.key;
          check('outside the window the pointer holds nothing here', !!ghost() && getComputedStyle(ghost()).display === 'none');
          pointer(window, 'pointerup', -40, mid(row).y);
          return wait(50);
        }).then(function () {
          check('let go out there, the list waits for the engine', calls(L.save).length === 0);
          window.__emit('desktop-drag-end', { kept: true });
          return wait(50);
        }).then(function () {
          var s = saved();
          check('and a pinned row that stayed on the desktop is unpinned', s && s.indexOf(k) < 0, JSON.stringify(s && s.slice(0, 3)));
        });
      })
      .then(function () {
        // A row that wasn't pinned changes no pins by going to the desktop.
        var row = loose()[0];
        mark();
        drag(row, [{ y: mid(row).y + 10 }, { x: -40, y: mid(row).y }]);
        window.__emit('desktop-drag-end', { kept: true });
        return wait(50).then(function () {
          check('a row that was not pinned only gets its card', calls('drag_to_desktop').length === 1 && calls(L.save).length === 0);
        });
      })
      .then(function () {
        // The engine speaks first when the page saw no pointerup (a drop outside its window).
        var row = pinned()[0];
        if (!row) return;
        mark();
        drag(row, [{ y: mid(row).y + 10 }, { y: mid(row).y + 12 }], true);
        window.__emit('desktop-drag-end', { kept: true });
        return wait(50).then(function () {
          check('the engine ends a drag the page lost sight of', !ghost() && calls(L.save).length === 1);
        });
      });
  };

  var todos = function () {
    var rows = function () { return all('.ca-board-list .ca-todorow'); };
    return until(function () { return rows().length >= 2; }, 20000)
      .then(function () {
        // The first section with two todos: which section has them goes by this Mac's PRs.
        var sections = all('.ca-board-list .ca-bay-lanes').map(function (s) { return Array.prototype.slice.call(s.querySelectorAll('.ca-todorow')); });
        var inSection = sections.filter(function (list) { return list.length >= 2; })[0] || [];
        check('todos can be dragged', rows().every(function (r) { return !!r.querySelector(':scope > .ca-grip'); }));
        if (inSection.length < 2) { check('a section with two todos to reorder', false, 'none in the sample'); return; }
        var first = inSection[0];
        var end = inSection[inSection.length - 1].getBoundingClientRect();
        drag(first, [{ y: mid(first).y + 10 }, { y: end.bottom - 3 }], true);
        return wait(50).then(function () {
          var d = last('drag_to_desktop');
          check('a todo is a todo on the desktop', d && d.pin.ref === 'todo', JSON.stringify(d));
          check('the pointer holds its title, with nothing to pin', !!ghost() && !/Pin/.test(ghost().textContent) && ghost().textContent.length > 0, ghost() && ghost().textContent);
          pointer(window, 'pointerup', mid(first).x, end.bottom - 3);
          return wait(50);
        }).then(function () {
          var op = calls('todo_op').map(function (c) { return c[1].op; }).filter(function (o) { return o.op === 'order'; })[0];
          check('dropped at the end of its section, it moves there', !!op, JSON.stringify(calls('todo_op').map(function (c) { return c[1]; })).slice(0, 200));
        });
      });
  };

  // A PR's menu pins it too, in the list and to the desktop, and P pins the one selected.
  var prMenu = function () {
    if (view !== 'prs') return;
    var row = all('.ca-board-list .ca-prrow').filter(function (r) { return !r.hasAttribute('data-pin-index'); })[0];
    row.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, cancelable: true, button: 0 }));
    rightClick(row);
    return wait(200).then(function () {
      var menus = window.__calls.filter(function (c) { return c[0] === 'plugin:menu|new'; });
      var items = menus.length ? menus[menus.length - 1][1].options.items.map(function (i) { return i.text; }) : [];
      check("a PR's menu pins it", items.indexOf('Pin PR') >= 0 && items.indexOf('Pin to desktop') >= 0, items.join(', '));
      mark();
      document.querySelector('.ca-board-list').dispatchEvent(new KeyboardEvent('keydown', { key: 'p', code: 'KeyP', bubbles: true, cancelable: true }));
      return wait(100);
    }).then(function () {
      var s = last('set_pinned_prs');
      check('and P pins the PR selected', !!s && s.keys.length > 0, JSON.stringify(s));
    });
  };

  say('drag check: waiting for the ' + (view === 'worktrees' ? 'worktrees’ headings' : view + ' view'));
  (view === 'todo' ? todos() : pins().then(prMenu))
    .then(done)
    .catch(stopped);
})();
