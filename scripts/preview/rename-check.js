// Checks Rename… in a session's right-click menu, in every window that has one, and where the
// name is typed: the main window's sheet (RenameDialog.tsx, and `r` on the list, ⌃⌘R in
// Sessions, where `r` alone does nothing), and the name field the peek and the waiting window
// show in the session header (SessionHeader.tsx), the quick panel's Rename… opening the waiting
// window on it. Run it in each:
//   scripts/preview.sh sessions '' scripts/preview/rename-check.js
//   scripts/preview.sh bar '' scripts/preview/rename-check.js
//   PREVIEW_PIN=session scripts/preview.sh desktop '' scripts/preview/rename-check.js
//   scripts/preview.sh quick '' scripts/preview/rename-check.js
//   scripts/preview.sh waiting '' scripts/preview/rename-check.js
//   scripts/preview.sh peek '' scripts/preview/rename-check.js
// The board needs a Claude Code, Codex or OpenCode session.
(function () {
  var calls = sent;
  var labels = menuLabels;
  var choose = function (label) { var m = window.__menu().filter(function (x) { return x[0] === label; })[0]; if (m) m[1](); return !!m; };
  var key = function (el, k) { el.dispatchEvent(new KeyboardEvent('keydown', { key: k, bubbles: true, cancelable: true })); };
  var renames = function () { return calls('rename_session'); };
  var NAME = 'Renamed in a preview';
  var surface = window.__CODEALONG_PEEK__ ? 'peek' : window.__CODEALONG_QUICK__ ? 'quick' : window.__CODEALONG_WAITING__ ? 'waiting' : window.__CODEALONG_DESKTOP__ ? 'desktop' : window.__CODEALONG_BAR__ ? 'bar' : 'main';
  say('rename check (' + surface + '): waiting');

  // The main window: the menu's Rename… opens the sheet on the name; Rename sends it and the row
  // shows it at once; `r` on the list opens it too (⌃⌘R in Sessions), and Escape leaves the name
  // alone.
  if (surface === 'main') {
    var sessions = (localStorage.getItem('codealong.view') || 'sessions') === 'sessions';
    var rows = function () { return Array.prototype.slice.call(document.querySelectorAll('.ca-lane.is-agent')); };
    var row;
    until(function () { return rows().length > 0; }, 20000)
      .then(function () {
        row = rows()[0];
        rightClick(row);
        return wait(200);
      })
      .then(function () {
        check('the session menu has Rename…', labels().indexOf('Rename…') >= 0, labels().join('|'));
        choose('Rename…');
        return until(function () { return !!document.querySelector('.ca-renamedlg input'); }, 2000);
      })
      .then(function () {
        var input = document.querySelector('.ca-renamedlg input');
        check('the sheet has the name, selected', input.value.length > 0 && input.selectionStart === 0 && input.selectionEnd === input.value.length, input.value);
        check('and says where it is kept', /keeps it with the (session|thread)/.test(document.querySelector('.ca-renamedlg .ca-dialog-lede').textContent), document.querySelector('.ca-renamedlg .ca-dialog-lede').textContent);
        type(input, '  ' + NAME + '\n ');
        document.querySelector('.ca-renamedlg button[type=submit]').click();
        return until(function () { return !document.querySelector('.ca-renamedlg'); }, 2000);
      })
      .then(function () {
        var r = renames().pop();
        check('Rename sends the name, one line', r && r.name === NAME && /^(claude|codex|opencode):/.test(r.key), JSON.stringify(r));
        return until(function () { return document.body.textContent.indexOf(NAME) >= 0; }, 2000).then(
          function () { check('every list shows it at once', true); },
          function () { check('every list shows it at once', false); },
        );
      })
      .then(function () {
        // `r` on the selected session. A click on a row puts the cursor in its box; Escape there
        // gives the list its keys back.
        row = rows()[0];
        row.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, cancelable: true }));
        return wait(100);
      })
      .then(function () {
        var box = document.activeElement && document.activeElement.closest('.ca-composer');
        if (box) key(document.activeElement, 'Escape');
        return wait(100);
      })
      .then(function () {
        // Sessions takes no letters alone, so a word typed on its list renames nothing; ⌃⌘R
        // renames there (mainKeys.ts). The other views take `r`.
        if (!sessions) return;
        key(window, 'r');
        return wait(300).then(function () {
          check('r alone opens nothing in Sessions', !document.querySelector('.ca-renamedlg'));
        });
      })
      .then(function () {
        window.dispatchEvent(new KeyboardEvent('keydown', { key: 'r', ctrlKey: sessions, metaKey: sessions, bubbles: true, cancelable: true }));
        return until(function () { return !!document.querySelector('.ca-renamedlg input'); }, 2000);
      })
      .then(function () {
        var input = document.querySelector('.ca-renamedlg input');
        check((sessions ? '⌃⌘R' : 'r') + ' opens it on the selected session', input.value.length > 0 && input.selectionEnd === input.value.length, input.value);
        var before = renames().length;
        key(input, 'Escape');
        return until(function () { return !document.querySelector('.ca-renamedlg'); }, 2000).then(function () {
          check('Escape leaves the name alone', renames().length === before);
        });
      })
      .then(done)
      .catch(stopped);
    return;
  }

  // The bar and a card on the desktop: Rename… opens the peek with the name ready to change.
  if (surface === 'bar' || surface === 'desktop') {
    var target = function () { return surface === 'bar' ? document.querySelector('.ca-hud [data-key]') : document.querySelector('.ca-desk'); };
    // A card's menu is its session's once its feed has come.
    var menuUp = function () {
      if (!target()) return false;
      rightClick(target());
      return window.__menu().length > 1;
    };
    until(menuUp, 20000)
      .then(function () {
        check('the menu has Rename…', labels().indexOf('Rename…') >= 0, labels().join('|'));
        choose('Rename…');
        return wait(100);
      })
      .then(function () {
        var p = calls('peek_open').pop();
        check('it opens the peek with the keyboard, on the name', p && p.focus === true && p.rename === true, JSON.stringify(p));
      })
      .then(done)
      .catch(stopped);
    return;
  }

  // The quick panel: Rename… on a session's row hands it to the waiting window, with its name
  // ready to change, and the panel puts itself away.
  if (surface === 'quick') {
    var sessionRow = function () { return document.querySelector('.ca-quick-row:not(.is-suggest):not(.is-todo):not(.is-more):not(.is-pr)'); };
    until(function () { return calls('quick_ready').length > 0 && !!sessionRow(); }, 20000)
      .then(function () {
        rightClick(sessionRow());
        return wait(200);
      })
      .then(function () {
        check('a session row’s menu has Rename…', labels().indexOf('Rename…') >= 0, labels().join('|'));
        choose('Rename…');
        return until(function () { return calls('show_waiting').length > 0; }, 2000);
      })
      .then(function () {
        var asked = calls('show_waiting').pop().context;
        check('Rename… opens the waiting window on its name', !!asked.key && asked.rename === true, JSON.stringify(asked));
        check('and the panel puts itself away', calls('hide_quick').length > 0);
      })
      .then(done)
      .catch(stopped);
    return;
  }

  // The waiting window, asked to show a session with its name ready to change: the name is a
  // field with the keyboard; ⏎ renames it and the session stays.
  if (surface === 'waiting') {
    var title = function () { var t = document.querySelector('.ca-sessinsp-title'); return t ? t.textContent : null; };
    var shown;
    until(function () { return !!title(); }, 20000)
      .then(function () { return window.__TAURI_INTERNALS__.invoke('get_board'); })
      .then(function (b) {
        var lane = b.lanes.find(function (l) { return !l.hidden && l.sessions.some(function (s) { return !s.visitor && /^(claude|codex|opencode):/.test(s.key) && !s.archived; }); });
        shown = lane.sessions.find(function (s) { return !s.visitor && /^(claude|codex|opencode):/.test(s.key) && !s.archived; });
        window.__emit('waiting-open', { key: shown.key, rename: true });
        return until(function () { return !!document.querySelector('.ca-sessinsp-name'); }, 2000);
      })
      .then(function () {
        var input = document.querySelector('.ca-sessinsp-name');
        check('its name is a field with the keyboard', document.activeElement === input && input.value.length > 0, input.value);
        check('the box below doesn’t take the keyboard', !document.activeElement.closest('.ca-composer'));
        type(input, NAME);
        key(input, 'Enter');
        return until(function () { return !document.querySelector('.ca-sessinsp-name'); }, 2000);
      })
      .then(function () {
        var r = renames().pop();
        check('⏎ renames it', r && r.key === shown.key && r.name === NAME, JSON.stringify(r));
        check('and the session stays', title() === NAME, title());
      })
      .then(done)
      .catch(stopped);
    return;
  }

  // The peek, opened for Rename…: the name is a field; Escape leaves it and the peek goes; ⏎ with
  // a new name renames it and the peek goes.
  var board;
  var open = function (id) {
    var lane = board.lanes.find(function (l) { return !l.hidden && l.sessions.some(function (s) { return !s.visitor && /^(claude|codex|opencode):/.test(s.key) && !s.archived; }); });
    var session = lane.sessions.find(function (s) { return !s.visitor && /^(claude|codex|opencode):/.test(s.key) && !s.archived; });
    window.__emit('peek-open', { id: id, lane: lane, session: session, terminal: board.apps.terminal, theme: board.theme, room: 600, below: true, focus: true, rename: true });
    return session;
  };
  var session;
  window.__TAURI_INTERNALS__.invoke('get_board')
    .then(function (b) {
      board = b;
      return until(function () { return !!document.querySelector('.ca-peek'); }, 20000);
    })
    .then(function () {
      session = open(7);
      return until(function () { return !!document.querySelector('.ca-sessinsp-name'); }, 2000);
    })
    .then(function () {
      var input = document.querySelector('.ca-sessinsp-name');
      check('opened for Rename…, its name is a field', input.value === session.title, input.value + ' vs ' + session.title);
      check('with the keyboard, not the box', document.activeElement === input);
      check('the line under it says how', /renames it/.test(document.querySelector('.ca-sessinsp-meta').textContent), document.querySelector('.ca-sessinsp-meta').textContent);
      var hides = calls('peek_hide').length;
      key(input, 'Escape');
      return wait(100).then(function () {
        check('Escape leaves the name and puts the peek away', renames().length === 0 && calls('peek_hide').length === hides + 1);
        // As the engine says once it has.
        window.__emit('peek-hidden', null);
        return wait(100);
      });
    })
    .then(function () {
      session = open(8);
      return until(function () { return !!document.querySelector('.ca-sessinsp-name'); }, 2000);
    })
    .then(function () {
      var input = document.querySelector('.ca-sessinsp-name');
      var hides = calls('peek_hide').length;
      type(input, NAME);
      key(input, 'Enter');
      return wait(100).then(function () {
        var r = renames().pop();
        check('⏎ renames it', r && r.key === session.key && r.name === NAME, JSON.stringify(r));
        check('and puts the peek away', calls('peek_hide').length === hides + 1);
      });
    })
    .then(done)
    .catch(stopped);
})();
