// Checks the views' keys (mainKeys.ts `viewChord`): ⌃1 to ⌃5 go to Sessions, Worktrees, PRs,
// Todo and Canvas from the list, the search field and a session's box, ⌘ and a digit still
// don't; the sidebar shows each view's keys in its count's place while the pointer is on it, and
// the rail it folds to says them.
//   scripts/preview.sh sessions '' scripts/preview/view-keys-check.js
//   PREVIEW_SCHEME=dark scripts/preview.sh sessions '' scripts/preview/view-keys-check.js
(function () {
  localStorage.setItem('codealong.project', 'All projects');
  localStorage.setItem('codealong.view', 'sessions');
  localStorage.removeItem('codealong.sidebarCollapsed');

  var VIEWS = ['Sessions', 'Worktrees', 'PRs', 'Todo', 'Canvas'];
  var shown = function () { var m = document.querySelector('main.ca-board'); return m ? m.getAttribute('aria-label') : null; };
  var navs = function () { return all('.ca-side .ca-nav.has-kbd'); };
  var key = function (el, init) { (el || document.activeElement || document.body).dispatchEvent(new KeyboardEvent('keydown', Object.assign({ bubbles: true, cancelable: true }, init))); };
  var ctrl = function (el, n) { key(el, { key: String(n), code: 'Digit' + n, ctrlKey: true }); };
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
      check('each view has its keys, ⌃1 to ⌃5 in order', rows.every(function (r, i) {
        return r.querySelector('.ca-nav-label').textContent === VIEWS[i] && r.querySelector('.ca-kbds').textContent === '⌃' + (i + 1);
      }), rows.map(function (r) { return r.querySelector('.ca-nav-label').textContent + ' ' + r.querySelector('.ca-kbds').textContent; }).join(', '));
      check('the keys are hidden until pointed at', rows.every(function (r) { return !shows(r.querySelector('.ca-kbds')); }));
      check('the rows say ⌃Tab goes through them', rows.every(function (r) { return r.title === '⌃Tab goes to the next view'; }), rows[0].title);
      check('the hover rules are there', hoverAs('ca-test-hover') === 2);
      var prs = rows[2];
      var height = prs.getBoundingClientRect().height;
      prs.classList.add('ca-test-hover');
      var kbd = prs.querySelector('.ca-kbds');
      var count = prs.querySelector('.ca-nav-count');
      check('pointed at, the keys show in the count’s place', shows(kbd) && (!count || !shows(count)), count ? count.textContent : 'no count');
      var row = prs.getBoundingClientRect();
      var caps = kbd.getBoundingClientRect();
      check('they fit in the row, which keeps its height', caps.right <= row.right && caps.top >= row.top && caps.bottom <= row.bottom && prs.getBoundingClientRect().height === height, JSON.stringify({ row: [row.left, row.right, row.top, row.bottom], caps: [caps.left, caps.right, caps.top, caps.bottom] }));
      var label = prs.querySelector('.ca-nav-label');
      check('the label isn’t cut short by them', label.scrollWidth <= label.clientWidth, label.scrollWidth + ' > ' + label.clientWidth);
      // Left pointed at for the screenshot.
      ctrl(document.querySelector('.ca-board-list') || document.body, 2);
      return on('Worktrees');
    })
    .then(function () {
      check('⌃2 goes to Worktrees', shown() === 'Worktrees', shown());
      ctrl(document.body, 3);
      return on('PRs');
    })
    .then(function () {
      check('⌃3 goes to PRs', shown() === 'PRs', shown());
      ctrl(document.body, 4);
      return on('Todo');
    })
    .then(function () {
      check('⌃4 goes to Todo', shown() === 'Todo', shown());
      ctrl(document.body, 5);
      return on('Canvas');
    })
    .then(function () {
      check('⌃5 goes to Canvas', shown() === 'Canvas', shown());
      ctrl(document.body, 6);
      return wait(200);
    })
    .then(function () {
      check('⌃6 does nothing', shown() === 'Canvas', shown());
      var search = document.querySelector('.ca-search input');
      if (!search) throw new Error('no search field');
      search.focus();
      ctrl(search, 1);
      return on('Sessions');
    })
    .then(function () {
      check('⌃1 from the search field goes to Sessions', shown() === 'Sessions', shown());
      key(document.body, { key: '2', code: 'Digit2', metaKey: true });
      return wait(200);
    })
    .then(function () {
      check('⌘2 doesn’t change the view', shown() === 'Sessions', shown());
      var row = document.querySelector('.ca-board-list [id^="row-"]');
      if (!row) return note('no session to open, so ⌃2 from its box went unchecked');
      row.dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
      return until(function () { return document.querySelector('.ca-sessinsp .ca-composer textarea'); }, 3000)
        .then(function (box) {
          box.focus();
          ctrl(box, 2);
          return on('Worktrees');
        })
        .then(function () {
          check('⌃2 from a session’s box goes to Worktrees', shown() === 'Worktrees', shown());
          ctrl(document.body, 1);
          return on('Sessions');
        })
        .catch(function () { note('the session had no box, so ⌃2 from it went unchecked'); });
    })
    .then(function () {
      key(document.body, { key: 's', code: 'KeyS', ctrlKey: true, metaKey: true });
      return until(function () { return all('.ca-rail .ca-rail-btn').length >= VIEWS.length; }, 3000, 'the rail');
    })
    .then(function () {
      var titles = all('.ca-rail .ca-rail-btn').slice(0, VIEWS.length).map(function (b) { return b.title; });
      check('the rail says each view’s keys', titles.every(function (t, i) { return t.indexOf(VIEWS[i] + ' (⌃' + (i + 1) + ')') === 0; }), titles.join(' | '));
      key(document.body, { key: 's', code: 'KeyS', ctrlKey: true, metaKey: true });
      return until(function () { return navs().length === VIEWS.length; }, 3000, 'the sidebar again');
    })
    .then(function () {
      navs()[2].classList.add('ca-test-hover');
    })
    .then(done)
    .catch(stopped);
})();
