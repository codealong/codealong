// Checks ⌃`, a new terminal in the folder of what a window shows (menus.ts `CHORDS.terminal`,
// mainKeys.ts, SessionCard.tsx): in Sessions from the session's box, a new terminal in the
// session's folder (`s|<key>|terminal`), shown beside that item in its header's Open menu and its
// right-click menu; grouped by worktree, on a worktree's heading, one in the worktree
// (`l|terminal`); in Todo from a todo's title, one in the todo's worktree; and in the peek from
// its box, in the session's folder.
//   PREVIEW_WAIT=60000 PREVIEW_TODOS=scripts/preview/todos-sample.json scripts/preview.sh sessions '' scripts/preview/terminal-key-check.js
//   PREVIEW_WAIT=60000 scripts/preview.sh peek '' scripts/preview/terminal-key-check.js
(function () {
  var peek = !!window.__CODEALONG_PEEK__;
  // This runs before the app's script: a session on this Mac to show.
  var xhr = new XMLHttpRequest();
  xhr.open('GET', './board.json', false);
  xhr.send();
  var board = JSON.parse(xhr.responseText);
  var picked = null;
  var pickedLane = null;
  board.lanes.forEach(function (l) {
    l.sessions.forEach(function (s) {
      if (!picked && !l.device && !l.hidden && !s.archived && !s.visitor && !s.hosted && l.targets.some(function (t) { return t.id === 'l|editor'; })) {
        picked = s;
        pickedLane = l;
      }
    });
  });
  if (!picked) { stopped('no session on this Mac to show'); return; }
  if (peek) localStorage.setItem('codealong.preview.peek', picked.key);
  else {
    localStorage.setItem('codealong.project', 'All projects');
    localStorage.setItem('codealong.view', 'sessions');
    localStorage.removeItem('codealong.sessionPanelMax');
  }

  var opens = function () { return calls(peek ? 'open_from_bar' : 'open_target').map(function (c) { return c[1]; }); };
  var lastOpen = function () { var o = opens(); return o[o.length - 1] || null; };
  var ctrlTick = function (el, over) { key(el || document.body, Object.assign({ key: '`', code: 'Backquote', ctrlKey: true }, over)); };
  var opened = function (n, what) { return until(function () { return opens().length > n; }, 3000, what).then(lastOpen); };
  var box = function () { return q((peek ? '.ca-peek' : '.ca-sessinsp') + ' .ca-composer textarea'); };
  var down = function (el) { el.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, cancelable: true, button: 0 })); };
  var nav = function (name) { return all('.ca-side .ca-nav').filter(function (b) { return b.textContent.indexOf(name) === 0; })[0]; };
  var terminalTarget = 's|' + picked.key + '|terminal';

  say('terminal key check: waiting for the session');
  var start = peek ? until(box, 20000, 'the peek’s box') : until(function () { return document.getElementById('row-' + pickedLane.id + '::' + picked.key); }, 20000, 'the session’s row');
  start
    .then(function () {
      if (peek) return;
      var row = document.getElementById('row-' + pickedLane.id + '::' + picked.key);
      down(row);
      row.click();
      return until(box, 5000, 'its box');
    })
    .then(function () { return wait(300); })
    .then(function () {
      var n = opens().length;
      box().focus();
      ctrlTick(box());
      return opened(n, '⌃` from the box').then(function (o) {
        check('⌃` from the box opens a terminal in the session’s folder', o && o.targetId === terminalTarget && o.laneId === pickedLane.id, JSON.stringify(o));
        check('and types nothing into it', box().value.indexOf('`') < 0, box().value);
      });
    })
    .then(function () {
      var n = opens().length;
      ctrlTick(box(), { shiftKey: true });
      ctrlTick(box(), { ctrlKey: false, metaKey: true });
      return wait(300).then(function () { check('⌃⇧` and ⌘` don’t', opens().length === n, JSON.stringify(opens().slice(n))); });
    })
    .then(function () {
      // The header's Open menu says the key beside the terminal.
      var root = peek ? '.ca-peek' : '.ca-sessinsp';
      var caret = q(root + ' .ca-split-caret');
      if (!caret) { check('the header has an Open menu', false, 'no caret'); return; }
      caret.click();
      return wait(150).then(function () {
        var item = all(root + ' .ca-openin .ca-menu-item').filter(function (e) { return e.textContent.indexOf('Open folder in ' + board.apps.terminal) >= 0; })[0];
        var keys = item ? all('.ca-kbd', item).map(function (k) { return k.textContent; }).join('') : '';
        check('the header’s Open menu says ⌃` beside the terminal', keys === '⌃`', item ? item.textContent : 'no terminal item');
        caret.click();
      });
    })
    .then(function () {
      // So does the session's menu, beside the folder's terminal.
      menuOf(q(peek ? '.ca-peek .ca-sessinsp-head' : '.ca-sessinsp-head'));
      return wait(200).then(function () {
        var m = calls('plugin:menu|new').pop();
        var items = m ? m[1].options.items : [];
        var folder = items.filter(function (i) { return /^Open folder in/.test(i.text || ''); })[0];
        var first = folder && folder.items ? folder.items[0] : folder;
        check('the session’s menu has ⌃` beside the folder’s terminal', first && first.accelerator === 'Ctrl+`', JSON.stringify(first && { text: first.text, accelerator: first.accelerator }));
      });
    })
    .then(function () {
      if (peek) return;
      // Grouped by worktree, a worktree's heading selected: a terminal in the worktree.
      var group = q('.ca-listtools [aria-label="Group by"] button[aria-label="Worktree"]');
      if (!group) { check('Sessions can be grouped by worktree', false, 'no Group by in the bar'); return; }
      group.click();
      return until(function () { return all('.ca-board .ca-grouphead.is-worktree-head[id^="row-"]').filter(function (h) { return !h.classList.contains('is-away'); })[0]; }, 5000, 'a worktree’s heading')
        .then(function (head) {
          var id = head.id.slice(4);
          down(head);
          return wait(300).then(function () {
            var n = opens().length;
            ctrlTick(document.body);
            return opened(n, '⌃` on ' + head.id);
          }).then(function (o) {
            check('⌃` on a worktree’s heading opens a terminal in the worktree', o && o.targetId === 'l|terminal' && o.laneId === id, JSON.stringify(o) + ' for ' + id);
          });
        });
    })
    .then(function () {
      if (peek) return;
      // A todo, from its title's field: a terminal in its worktree.
      nav('Todo').click();
      return until(function () { return q('.ca-todorow-title'); }, 5000, 'a todo').then(function () {
        var row = all('.ca-todorow').filter(function (r) { return r.textContent.indexOf('sidebar’s scroll position') >= 0 || r.textContent.indexOf("sidebar's scroll position") >= 0; })[0] || q('.ca-todorow');
        down(row);
        row.click();
        return wait(400).then(function () {
          var field = q('.ca-todoinsp-title');
          if (field) field.focus();
          var n = opens().length;
          ctrlTick(field || document.body);
          return opened(n, '⌃` on a todo').then(function (o) {
            check('⌃` on a todo opens a terminal in its worktree, from its title’s field', !!field && o && o.targetId === 'l|terminal', JSON.stringify(o) + (field ? '' : ' with no title field'));
          });
        });
      });
    })
    .then(done)
    .catch(stopped);
})();
