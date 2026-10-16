// Checks the main window's title bar and the list's find (App.tsx, ui/ListBar.tsx). The box in the
// middle of the title bar says what the quick panel's box says, with the panel's shortcut, sits in
// the middle of the window, and a click opens the quick panel; New session and Improve Codealong
// are icons, each named and with a tooltip. ⌘F from the list opens the find in the strip over it,
// saying the view it finds in, with the keyboard in it; words narrow the list, and words nothing
// has say so. Left with words, it stays and the list stays narrowed, and ⌘F gives it the keyboard
// back with the words selected; a view's keys take it along to that view, which it says. Escape
// clears it and puts it away, the list whole again; left with nothing typed it goes away by
// itself; its × clears it and puts it away.
//   scripts/preview.sh sessions '' scripts/preview/list-find-check.js
//   PREVIEW_SCHEME=dark scripts/preview.sh sessions '' scripts/preview/list-find-check.js
(function () {
  localStorage.setItem('codealong.project', 'All projects');
  localStorage.setItem('codealong.view', 'sessions');
  // What was said in sessions finds nothing: the stub's text search has the latest two say any
  // words, which would be listed for the words nothing has.
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    if (cmd === 'search_text') return Promise.resolve({ hits: [], building: false, off: false });
    return real(cmd, args, options);
  };
  var rows = function () { return all('.ca-board-list [id^="row-"]'); };
  var list = function () { return q('.ca-board-list'); };
  var field = function () { return q('.ca-listfind input'); };
  var press = function (el, init) {
    var e = new KeyboardEvent('keydown', Object.assign({ bubbles: true, cancelable: true }, init));
    el.dispatchEvent(e);
    return wait(150).then(function () { return e; });
  };
  var findKey = function (el) { return press(el, { key: 'f', code: 'KeyF', metaKey: true }); };
  var view = function (letter) { return press(document.activeElement || document.body, { key: letter, code: 'Key' + letter, metaKey: true, shiftKey: true }); };
  var typeIn = function (text) { type(field(), text); return wait(300); };
  var whole = 0;
  var word = null;

  say('list find check: waiting for the board');
  until(function () { return rows().length > 1 && !!q('.ca-win-bar .ca-search'); }, 20000, 'the list')
    .then(function () {
      var box = q('.ca-win-bar .ca-search');
      var r = box.getBoundingClientRect();
      check('the title bar’s box says what the quick panel’s does', q('span', box).textContent === 'Find a session, or describe new work…', q('span', box).textContent);
      var keys = all('.ca-kbd', box).map(function (k) { return k.textContent; }).join('');
      check('with the panel’s shortcut', keys.length > 1 && box.title.indexOf(keys === '⌘K' ? '⌘K' : keys) >= 0, keys + ' / ' + box.title);
      check('in the middle of the window', Math.abs(r.left + r.width / 2 - window.innerWidth / 2) <= 1, Math.round(r.left) + '–' + Math.round(r.right) + ' of ' + window.innerWidth);
      check('and no field to type in', !q('.ca-win-bar input'));
      var icons = ['New session', 'Improve Codealong'].map(function (name) {
        var b = q('.ca-win-bar button[aria-label="' + name + '"]');
        return !!b && !!q('svg', b) && !b.textContent.trim() && !!b.title;
      });
      check('New session and Improve Codealong are icons, named and with tooltips', icons[0] && icons[1], icons.join(','));
      var before = calls('show_quick').length;
      box.click();
      return wait(100).then(function () {
        var asked = last('show_quick');
        check('a click on the box opens the quick panel', calls('show_quick').length === before + 1 && !!asked && !asked.context.improve, JSON.stringify(asked));
      });
    })
    .then(function () {
      whole = rows().length;
      check('no find over the list until it’s asked for', !field());
      list().focus();
      return findKey(list());
    })
    .then(function (e) {
      check('⌘F from the list opens the find over it, with the keyboard', e.defaultPrevented && !!field() && document.activeElement === field(), document.activeElement && document.activeElement.className);
      check('saying what it finds in', field().placeholder === 'Find in sessions', field().placeholder);
      // Made up as it runs: written here, it would be said in the conversation of a session
      // working on this file, and found there.
      return typeIn('q' + Math.random().toString(36).slice(2, 10) + 'xz');
    })
    .then(function () {
      var empty = q('.ca-board-empty');
      check('words nothing has say so', rows().length === 0 && !!empty && /match/.test(empty.textContent), rows().length + ' rows (' + rows().slice(0, 2).map(function (r) { return r.id + ': ' + r.textContent.slice(0, 60); }).join(' | ') + '); ' + (empty ? empty.textContent : 'no note'));
      return typeIn('');
    })
    .then(function () {
      // A word of the first session's title, which some rows have and, mostly, not all.
      var title = q('.ca-board-list [id^="row-"] .ca-agentline-title');
      var text = title ? title.textContent : rows()[0].textContent;
      word = (text.match(/[A-Za-z]{4,}/) || [null])[0];
      if (!word) throw new Error('no word to find in ' + JSON.stringify(text));
      return typeIn(word);
    })
    .then(function () {
      var n = rows().length;
      check('words narrow the list', n > 0 && n <= whole, n + ' of ' + whole + ' for ' + word);
      if (n === whole) note('every row had “' + word + '”, so the narrowing shows only that nothing went');
      list().focus();
      return wait(150);
    })
    .then(function () {
      check('left with words, it stays, and so does what it found', !!field() && field().value === word && rows().length <= whole);
      return findKey(list());
    })
    .then(function (e) {
      var f = field();
      check('⌘F gives it the keyboard back, the words selected', e.defaultPrevented && document.activeElement === f && f.selectionStart === 0 && f.selectionEnd === word.length, f && f.selectionStart + '–' + f.selectionEnd);
      return view('W');
    })
    .then(function () {
      check('a view’s keys take it along', !!q('.ca-board[aria-label="Worktrees"]') && !!field() && field().value === word && document.activeElement === field(), document.activeElement && document.activeElement.className);
      check('and it says that view', field().placeholder === 'Find in worktrees', field().placeholder);
      return press(field(), { key: 'Escape' });
    })
    .then(function () {
      check('Escape clears it and puts it away', !field() && !q('.ca-listbar'));
      return view('S');
    })
    .then(function () {
      check('the list is whole again', rows().length === whole, rows().length + ' of ' + whole);
      return findKey(document.body);
    })
    .then(function () {
      check('⌘F opens it empty', !!field() && field().value === '');
      list().focus();
      return wait(150);
    })
    .then(function () {
      check('left with nothing typed, it goes away by itself', !field());
      return findKey(list());
    })
    .then(function () { return typeIn(word); })
    .then(function () {
      q('.ca-listfind button[aria-label="Close find"]').click();
      return wait(150);
    })
    .then(function () {
      check('its × clears it and puts it away', !field() && rows().length === whole, rows().length + ' of ' + whole);
    })
    .then(done)
    .catch(stopped);
})();
