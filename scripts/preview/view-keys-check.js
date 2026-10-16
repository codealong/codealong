// Checks the views' keys (mainKeys.ts `VIEW_CHORDS`): ⇧⌘S, ⇧⌘W, ⇧⌘P, ⇧⌘T, ⇧⌘K and ⇧⌘B go to
// Sessions, Worktrees, PRs, Todo, Kanban and Canvas from the list, the search field and a
// session's box, and ⇧⌘C, which clipboard managers take, goes nowhere; ⌥⌘C copies the selected worktree's path. The
// sidebar shows each view's keys in its count's place while the pointer is on it, and the rail it
// folds to says them.
//   scripts/preview.sh sessions '' scripts/preview/view-keys-check.js
//   PREVIEW_SCHEME=dark scripts/preview.sh sessions '' scripts/preview/view-keys-check.js
(function () {
  localStorage.setItem('codealong.project', 'All projects');
  localStorage.setItem('codealong.view', 'sessions');
  localStorage.removeItem('codealong.sidebarCollapsed');
  var copied = [];
  navigator.clipboard.writeText = function (text) { copied.push(text); return Promise.resolve(); };

  var VIEWS = ['Sessions', 'Worktrees', 'PRs', 'Todo', 'Kanban', 'Canvas'];
  var LETTERS = ['S', 'W', 'P', 'T', 'K', 'B'];
  var shown = function () { var m = document.querySelector('main.ca-board'); return m ? m.getAttribute('aria-label') : null; };
  var navs = function () { return all('.ca-side .ca-nav.has-kbd'); };
  var key = function (el, init) { (el || document.activeElement || document.body).dispatchEvent(new KeyboardEvent('keydown', Object.assign({ bubbles: true, cancelable: true }, init))); };
  // macOS reports a letter with ⇧ held as the capital.
  var go = function (el, letter) { key(el, { key: letter, code: 'Key' + letter, metaKey: true, shiftKey: true }); };
  var on = function (name) { return until(function () { return shown() === name; }, 3000).catch(function () {}); };
  var shows = function (el) { return !!el && getComputedStyle(el).display !== 'none'; };

  // Pointing can't be done from a page, so the rows' :hover rules are copied under a class.
  var hoverAs = function (cls) {
    var added = 0;
    Array.prototype.forEach.call(document.styleSheets, function (sheet) {
      var rules;
      try { rules = sheet.cssRules; } catch (e) { return; }
      Array.prototype.slice.call(rules).forEach(function (r) {
        if (r.selectorText && r.selectorText.indexOf('has-kbd:is(:hover') >= 0) {
          sheet.insertRule(r.cssText.replace(/:is\(:hover, :focus-visible\)/g, '.' + cls), sheet.cssRules.length);
          added++;
        }
      });
    });
    return added;
  };

  say('view keys check: waiting for the sidebar');
  until(function () { return navs().length === VIEWS.length && shown() === 'Sessions'; }, 20000, 'the sidebar’s views')
    .then(function () { return wait(500); })
    .then(function () {
      var rows = navs();
      check('each view has its keys, ⇧⌘ and a letter', rows.every(function (r, i) {
        return r.querySelector('.ca-nav-label').textContent === VIEWS[i] && r.querySelector('.ca-kbds').textContent === '⇧⌘' + LETTERS[i];
      }), rows.map(function (r) { return r.querySelector('.ca-nav-label').textContent + ' ' + r.querySelector('.ca-kbds').textContent; }).join(', '));
      check('the keys are hidden until pointed at', rows.every(function (r) { return !shows(r.querySelector('.ca-kbds')); }));
      check('the rows say ⌃Tab goes through them', rows.every(function (r) { return r.title === '⌃Tab goes to the next view'; }), rows[0].title);
      check('the hover rules are there', hoverAs('ca-test-hover') === 2);
      // The longest label with the most keys: three caps beside Worktrees.
      return rows.reduce(function (p, row) {
        return p.then(function () {
          var height = row.getBoundingClientRect().height;
          row.classList.add('ca-test-hover');
          var name = row.querySelector('.ca-nav-label').textContent;
          var kbd = row.querySelector('.ca-kbds');
          var count = row.querySelector('.ca-nav-count');
          var box = row.getBoundingClientRect();
          var caps = kbd.getBoundingClientRect();
          var label = row.querySelector('.ca-nav-label');
          check(name + ': pointed at, its keys show in its count’s place', shows(kbd) && (!count || !shows(count)), count ? count.textContent : 'no count');
          check(name + ': they fit in the row, which keeps its height', caps.right <= box.right && caps.top >= box.top && caps.bottom <= box.bottom && row.getBoundingClientRect().height === height, JSON.stringify({ row: [box.left, box.right], caps: [caps.left, caps.right] }));
          check(name + ': its label isn’t cut short by them', label.scrollWidth <= label.clientWidth, label.scrollWidth + ' > ' + label.clientWidth);
          row.classList.remove('ca-test-hover');
        });
      }, Promise.resolve());
    })
    .then(function () {
      go(document.querySelector('.ca-board-list') || document.body, 'W');
      return on('Worktrees');
    })
    .then(function () {
      check('⇧⌘W goes to Worktrees', shown() === 'Worktrees', shown());
      var row = document.querySelector('.ca-board-list [id^="row-"]');
      if (!row) return note('no worktree to select, so ⌥⌘C went unchecked');
      row.dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
      return wait(300).then(function () {
        copied.length = 0;
        // ⌥ changes the character: ⌥C is ç on a US layout.
        key(document.body, { key: 'ç', code: 'KeyC', metaKey: true, altKey: true });
        return wait(200);
      }).then(function () {
        check('⌥⌘C copies the selected worktree’s path', copied.length === 1 && copied[0].charAt(0) === '/', JSON.stringify(copied));
        copied.length = 0;
        go(document.body, 'C');
        return wait(200);
      }).then(function () {
        check('⇧⌘C copies nothing and leaves the view', copied.length === 0 && shown() === 'Worktrees', JSON.stringify(copied) + ' ' + shown());
      });
    })
    .then(function () {
      go(document.body, 'P');
      return on('PRs');
    })
    .then(function () {
      check('⇧⌘P goes to PRs', shown() === 'PRs', shown());
      check('and doesn’t open the project switcher', !document.querySelector('.ca-projswitch-row'));
      go(document.body, 'T');
      return on('Todo');
    })
    .then(function () {
      check('⇧⌘T goes to Todo', shown() === 'Todo', shown());
      go(document.body, 'K');
      return on('Kanban');
    })
    .then(function () {
      check('⇧⌘K goes to Kanban', shown() === 'Kanban', shown());
      go(document.body, 'B');
      return on('Canvas');
    })
    .then(function () {
      check('⇧⌘B goes to Canvas', shown() === 'Canvas', shown());
      // The list's find (ListBar.tsx), as ⌘F opens it from the board.
      key(document.body, { key: 'f', code: 'KeyF', metaKey: true });
      return until(function () { return document.activeElement === document.querySelector('.ca-listfind input'); }, 2000, 'the list’s find');
    })
    .then(function () {
      go(document.activeElement, 'S');
      return on('Sessions');
    })
    .then(function () {
      check('⇧⌘S from the find field goes to Sessions', shown() === 'Sessions', shown());
      check('and the field keeps the keyboard', !!document.activeElement && document.activeElement === document.querySelector('.ca-listfind input'), document.activeElement && document.activeElement.className);
      key(document.body, { key: 'w', code: 'KeyW', metaKey: true });
      return wait(200);
    })
    .then(function () {
      check('⌘W without ⇧ doesn’t change the view', shown() === 'Sessions', shown());
      var row = document.querySelector('.ca-board-list [id^="row-"]');
      if (!row) return note('no session to open, so ⇧⌘W from its box went unchecked');
      row.dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
      // Once the session has opened, its box drawn again: the click that opens it shows Sessions
      // after a moment.
      return until(function () { return document.querySelector('.ca-sessinsp .ca-composer textarea'); }, 3000)
        .then(function () { return wait(500); })
        .then(function () {
          var box = document.querySelector('.ca-sessinsp .ca-composer textarea');
          box.focus();
          check('the box has the cursor', document.activeElement === box);
          go(box, 'W');
          return on('Worktrees');
        })
        .then(function () {
          check('⇧⌘W from a session’s box goes to Worktrees', shown() === 'Worktrees', shown());
          go(document.body, 'S');
          return on('Sessions');
        })
        .catch(function () { note('the session had no box, so ⇧⌘W from it went unchecked'); });
    })
    .then(function () {
      // ⌃⌘S folds the sidebar from the list, not from a text field.
      if (document.activeElement && document.activeElement.blur) document.activeElement.blur();
      key(document.body, { key: 's', code: 'KeyS', ctrlKey: true, metaKey: true });
      return until(function () { return all('.ca-rail .ca-rail-btn').length >= VIEWS.length; }, 3000, 'the rail');
    })
    .then(function () {
      var titles = all('.ca-rail .ca-rail-btn').slice(0, VIEWS.length).map(function (b) { return b.title; });
      check('the rail says each view’s keys', titles.every(function (t, i) { return t.indexOf(VIEWS[i] + ' (⇧⌘' + LETTERS[i] + ')') === 0; }), titles.join(' | '));
      key(document.body, { key: 's', code: 'KeyS', ctrlKey: true, metaKey: true });
      return until(function () { return navs().length === VIEWS.length; }, 3000, 'the sidebar again');
    })
    .then(done)
    .catch(stopped);
})();
