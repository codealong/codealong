// An archived session on a PR (board.rs `pr_item`, boardLists.ts's PR rows read from every
// session, archived ones too): the PR's panel still lists it, its chip drawn with an archive box
// for its status glyph and said to be archived, and it isn't tagged primary. The check archives
// one of a listed PR's sessions in every board the page gets, as the engine lists it once it's
// archived (no longer the PR's primary).
//   scripts/preview.sh prs '' scripts/preview/pr-archived-check.js
(function () {
  localStorage.removeItem('codealong.prDetailsOpen');
  localStorage.setItem('codealong.prRows', 'two');
  var press = function (el) {
    el.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, cancelable: true, button: 0, buttons: 1 }));
    el.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, cancelable: true, button: 0 }));
    el.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true, detail: 1 }));
  };
  var rowOf = function (pr) {
    return all('.ca-prrow').filter(function (r) { var n = q('.ca-prrow-num', r); return n && n.textContent === '#' + pr.number && q('.ca-lane-headline', r).textContent === pr.title; })[0];
  };
  var sessionOf = function (board, x) {
    var lane = (board.lanes || []).filter(function (l) { return l.id === x.lane; })[0];
    return lane && lane.sessions.filter(function (s) { return s.key === x.key; })[0];
  };

  say('Archived session on a PR check: waiting for the list');
  until(function () { return all('.ca-prrow').length > 0; }, 20000, 'a PR row')
    .then(function () { return window.__TAURI_INTERNALS__.invoke('get_board'); })
    .then(function (board) {
      var listed = (board.prs || []).filter(function (p) { return rowOf(p) && (p.sessions || []).some(function (x) { return sessionOf(board, x) && !sessionOf(board, x).archived; }); });
      var pr = listed[0];
      if (!pr) return stopped('no PR listed here has a session on this Mac');
      var x = pr.sessions.filter(function (y) { return sessionOf(board, y) && !sessionOf(board, y).archived; })[0];
      var session = sessionOf(board, x);
      // The board as the engine has it with that session archived and finished with.
      var archived = function (b) {
        var copy = JSON.parse(JSON.stringify(b));
        copy.lanes.forEach(function (l) {
          l.sessions.forEach(function (s) {
            if (s.key !== session.key) return;
            s.archived = true;
            s.status = 'stopped';
          });
        });
        copy.prs.forEach(function (p) {
          if (p.key !== pr.key || p.primary !== session.key) return;
          delete p.primary;
          delete p.primaryBy;
        });
        return copy;
      };
      var invoke = window.__TAURI_INTERNALS__.invoke;
      window.__TAURI_INTERNALS__.invoke = function (cmd) {
        var got = invoke.apply(this, arguments);
        return cmd === 'get_board' ? got.then(archived) : got;
      };
      window.__emit('board', archived(board));
      var chip = function () {
        var head = q('.ca-prinsp-head');
        if (!head || head.textContent.indexOf(pr.title) < 0) return null;
        return all('.ca-places .ca-chip', head).filter(function (c) { return (c.getAttribute('title') || '').indexOf(session.title) === 0; })[0];
      };
      return until(function () { return rowOf(pr); }, 20000, 'the row of #' + pr.number)
        .then(function (row) {
          press(row);
          return until(chip, 5000, 'the chip of “' + session.title + '” in the panel of #' + pr.number);
        })
        .then(function (c) {
          var glyph = q('.ca-glyph', c);
          check('its panel lists the archived session', !!c, session.title);
          check('with an archive box for its status glyph', !!glyph && glyph.classList.contains('ca-glyph--archived'), glyph ? glyph.getAttribute('class') : 'no glyph');
          check('its chip says it’s archived', (c.getAttribute('title') || '').indexOf(' · Archived') > 0, (c.getAttribute('title') || '').replace(/\n/g, ' | '));
          check('its chip is dimmed as archived', c.classList.contains('is-archived'), c.getAttribute('class'));
          check('it isn’t tagged primary', !q('.ca-places-primary', c), c.textContent);
        });
    })
    .then(done)
    .catch(stopped);
})();
