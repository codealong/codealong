// Checks the keys of the strip over a PR's panel in the main window (PrStrip.tsx), where the panel
// shows a session's PR in its place in Sessions, the session still selected in the list: ⌃⌘1
// snoozes the PR's review and not the session, ⌃⌘G opens the PR on GitHub, and ⌘⏎ outside the box
// approves nothing (it's the list's there, and the box's in it). The main window's keys listen
// from before the PR showed, so the strip's must go first. Every open PR is made one that waits
// on your review, someone else's, so whichever a session shows can be snoozed.
//   PREVIEW_WAIT=60000 scripts/preview.sh sessions '' scripts/preview/pr-panel-keys-check.js
(function () {
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    var p = real(cmd, args, options);
    if (cmd !== 'get_board') return p;
    return p.then(function (board) {
      board.prs.forEach(function (pr) {
        if (pr.state === 'merged' || pr.state === 'closed') return;
        Object.assign(pr, { state: 'open', mine: false, author: 'ana', reviewRequested: true, snoozedUntil: null });
      });
      return board;
    });
  };
  var press = function (el) {
    el.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, cancelable: true, button: 0, buttons: 1 }));
    el.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, cancelable: true, button: 0 }));
    el.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true, detail: 1 }));
  };
  // A key pressed with the keyboard on the window, not in a field: where the list's keys act.
  var key = function (init) {
    if (document.activeElement && document.activeElement !== document.body) document.activeElement.blur();
    document.body.dispatchEvent(new KeyboardEvent('keydown', Object.assign({ bubbles: true, cancelable: true }, init)));
  };
  // Sessions' rows, by their ids (`row-<worktree>::<session>`).
  var rows = function () { return all('[role="option"][id^="row-"]').filter(function (r) { return r.id.indexOf('::') > 0; }); };
  var chip = function () { return q('.ca-sessinsp-head .ca-pr--open'); };
  var strip = function () { return q('.ca-prpanel > .ca-reply-queue'); };
  var shown = function () { var c = q('.ca-prpanel .ca-insp-call'); return c ? c.textContent : null; };
  var number = null;

  say('pr panel keys check: looking for a session with an open PR');
  until(function () { return q('.ca-sessinsp'); }, 20000, 'no session’s panel')
    .then(function () {
      // The first session listed whose header has an open PR's chip, selected.
      var tried = 0;
      var next = function () {
        if (chip()) return Promise.resolve(true);
        var list = rows();
        if (tried >= Math.min(list.length, 40)) return Promise.resolve(false);
        press(list[tried++]);
        return wait(150).then(next);
      };
      return next();
    })
    .then(function (found) {
      if (!found) throw new Error('no session listed has an open PR');
      number = (chip().textContent.match(/#\d+/) || [])[0];
      press(chip());
      // A click waits for a second before it shows the PR, as a double-click opens it on GitHub.
      return until(function () { return !!strip() && shown() === number; }, 3000, 'the PR’s panel in the session’s place');
    })
    .then(function () {
      check('the session is still selected in the list', rows().some(function (r) { return r.getAttribute('aria-selected') === 'true'; }), '');
      var reviews = calls('snooze_reviews').length;
      var sessions = calls('snooze_sessions').length;
      key({ key: '1', code: 'Digit1', metaKey: true, ctrlKey: true });
      return wait(200).then(function () {
        var s = calls('snooze_reviews')[reviews];
        check('⌃⌘1 snoozes the PR’s review', !!s && s[1].keys.length === 1 && s[1].keys[0].slice(-number.length) === number && s[1].until > Date.now(), JSON.stringify(s && s[1]));
        check('and not the session behind it', calls('snooze_sessions').length === sessions, JSON.stringify(last('snooze_sessions')));
      });
    })
    .then(function () {
      var opens = calls('open_url').length;
      key({ key: 'g', code: 'KeyG', metaKey: true, ctrlKey: true });
      return wait(200).then(function () {
        var o = calls('open_url')[opens];
        check('⌃⌘G opens it on GitHub', !!o && /\/pull\/\d+$/.test(o[1].url) && o[1].url.slice(-number.length + 1) === number.slice(1), JSON.stringify(o && o[1]));
      });
    })
    .then(function () {
      var reviews = calls('review_pr').length;
      key({ key: 'Enter', code: 'Enter', metaKey: true });
      return wait(200).then(function () {
        check('⌘⏎ outside the box approves nothing', calls('review_pr').length === reviews, JSON.stringify(last('review_pr')));
      });
    })
    .then(done, stopped);
})();
