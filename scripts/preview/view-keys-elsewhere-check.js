// Checks the views' keys in the windows that float (mainKeys.ts `useViewKeys`): ⇧⌘P, ⇧⌘T and
// ⇧⌘S there bring Codealong's window forward on PRs, Todo and Sessions, putting away what goes for
// it as Show in Codealong does (the quick panel, the peek, the notch's session; the waiting window
// stays), from a field too, where the quick panel's ⇧⌘P and ⇧⌘T used to open its menus; ⌘P still
// does. In the board's window, such an ask goes to the view, and one for a view it doesn't have
// leaves it where it is.
//   scripts/preview.sh quick '' scripts/preview/view-keys-elsewhere-check.js
//   scripts/preview.sh waiting '' scripts/preview/view-keys-elsewhere-check.js
//   scripts/preview.sh peek '' scripts/preview/view-keys-elsewhere-check.js
//   PREVIEW_SIZE=1512,980 PREVIEW_WAIT=60000 scripts/preview.sh notch '' scripts/preview/view-keys-elsewhere-check.js
//   scripts/preview.sh sessions '' scripts/preview/view-keys-elsewhere-check.js
(function () {
  // macOS reports a letter with ⇧ held as the capital.
  var press = function (el, letter, shift) {
    key(el || document.body, { key: shift === false ? letter.toLowerCase() : letter, code: 'Key' + letter, metaKey: true, shiftKey: shift !== false });
  };
  var asked = function () { var s = sent('show_main'); return s.map(function (a) { return a.reveal && a.reveal.kind === 'view' ? a.reveal.view : a.reveal && a.reveal.kind; }); };
  // Each press asks for its view once, and puts away what goes (`hide`, a command's name) or
  // nothing.
  var goes = function (where, el, letter, view, hide) {
    var before = asked().length;
    var hid = hide ? sent(hide).length : 0;
    press(el, letter);
    return wait(200).then(function () {
      var now = asked().slice(before);
      check(where + ': ⇧⌘' + letter + ' asks for ' + view + ' in Codealong’s window', now.length === 1 && now[0] === view, JSON.stringify(now));
      if (hide) check(where + ': and puts itself away', sent(hide).length === hid + 1, hide + ' ×' + (sent(hide).length - hid));
    });
  };

  if (window.__CODEALONG_QUICK__) {
    var input = function () { return q('.ca-quick-input textarea'); };
    var picker = function () { return !!q('[data-picker-trigger][aria-expanded="true"]'); };
    say('view keys elsewhere: waiting for the quick panel');
    until(function () { return calls('quick_ready').length > 0 && !!input(); }, 30000, 'the quick panel')
      .then(function () {
        input().focus();
        return goes('the quick panel', input(), 'P', 'prs', 'hide_quick');
      })
      .then(function () {
        check('the quick panel: no menu opened for it', !picker());
        return goes('the quick panel', input(), 'T', 'todo', 'hide_quick');
      })
      .then(function () {
        check('the quick panel: ⇧⌘T left the box’s text alone', input().value === '', input().value);
        var before = asked().length;
        press(input(), 'P', false);
        return wait(300).then(function () {
          check('the quick panel: ⌘P without ⇧ still opens its menu, and asks nothing', asked().length === before && picker(), String(picker()));
        });
      })
      .then(done)
      .catch(stopped);
    return;
  }

  if (window.__CODEALONG_WAITING__) {
    say('view keys elsewhere: waiting for the waiting window');
    until(function () { return !!q('.ca-waiting'); }, 30000, 'the waiting window')
      .then(function () { return wait(300); })
      .then(function () { return goes('the waiting window', q('.ca-waiting .ca-composer textarea') || document.body, 'T', 'todo', null); })
      .then(function () { return goes('the waiting window', document.body, 'S', 'sessions', null); })
      .then(done)
      .catch(stopped);
    return;
  }

  if (window.__CODEALONG_PEEK__) {
    say('view keys elsewhere: waiting for the peek');
    until(function () { return !!q('.ca-peek .ca-composer textarea'); }, 30000, 'the peek’s box')
      .then(function () { return goes('the peek', q('.ca-peek .ca-composer textarea'), 'P', 'prs', 'peek_hide'); })
      .then(done)
      .catch(stopped);
    return;
  }

  if (window.__CODEALONG_BAR__) {
    var notch = function () { return q('.ca-notch'); };
    var is = function (state) { return !!notch() && notch().classList.contains('is-' + state); };
    say('view keys elsewhere: waiting for the notch');
    until(function () { return !!q('.ca-notch-shape') && !!q('.ca-notch-count'); }, 30000, 'the notch with something waiting')
      .then(function () {
        q('.ca-notch-shape').click();
        return until(function () { return is('session') && !!q('.ca-notch-session .ca-composer textarea'); }, 10000, 'the notch’s session');
      })
      .then(function () { return wait(300); })
      .then(function () {
        var box = q('.ca-notch-session .ca-composer textarea');
        box.focus();
        return goes('the notch', box, 'P', 'prs', null);
      })
      .then(function () { return until(function () { return !is('session'); }, 3000).catch(function () {}); })
      .then(function () {
        check('the notch: its session is put away', !is('session'), notch() && notch().className);
        var keys = sent('notch_keys');
        check('the notch: and it gives the keyboard back', keys.length > 0 && keys[keys.length - 1].on === false, JSON.stringify(keys));
      })
      .then(done)
      .catch(stopped);
    return;
  }

  // The board's window.
  localStorage.setItem('codealong.view', 'sessions');
  var shown = function () { var m = q('main.ca-board'); return m ? m.getAttribute('aria-label') : null; };
  say('view keys elsewhere: waiting for the board');
  until(function () { return shown() === 'Sessions' && calls('plugin:event|listen').some(function (c) { return c[1].event === 'reveal'; }); }, 20000, 'the board, listening for reveals')
    .then(function () {
      window.__emit('reveal', { kind: 'view', view: 'prs' });
      return until(function () { return shown() === 'PRs'; }, 3000).catch(function () {});
    })
    .then(function () {
      check('the board: an ask for PRs from another window shows PRs', shown() === 'PRs', shown());
      window.__emit('reveal', { kind: 'view', view: 'nowhere' });
      return wait(300);
    })
    .then(function () {
      check('the board: one for a view it doesn’t have leaves it on PRs', shown() === 'PRs', shown());
      window.__emit('reveal', { kind: 'view', view: 'todo' });
      return until(function () { return shown() === 'Todo'; }, 3000).catch(function () {});
    })
    .then(function () {
      check('the board: and one for Todo shows Todo', shown() === 'Todo', shown());
      window.__emit('reveal', { kind: 'view', view: 'sessions' });
      return until(function () { return shown() === 'Sessions'; }, 3000).catch(function () {});
    })
    .then(done)
    .catch(stopped);
})();
