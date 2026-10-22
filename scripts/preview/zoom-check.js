// Checks zooming the board's windows (zoom.ts, mainKeys.ts, App.tsx `zoom`): ⌘= and ⌘+ ask the
// engine to zoom in, ⌘- out and ⌘0 back to actual size, from the list and from a session's box,
// and say where it got to; View's menu asks the same; ⌃- stays history's; a menu dropped from a
// button lands under it on a zoomed page (api.ts `popupMenu`); and the canvas keeps the keys for
// its own zoom. The board here says 125%; the stub's answers to a step are fake-tauri.js's.
//   PREVIEW_WAIT=60000 scripts/preview.sh sessions '' scripts/preview/zoom-check.js
//   PREVIEW_WAIT=60000 PREVIEW_BOARDS=scripts/preview/boards-everything.json scripts/preview.sh canvas '' scripts/preview/zoom-check.js
(function () {
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    var p = real(cmd, args, options);
    if (cmd !== 'get_board') return p;
    return p.then(function (board) {
      board.zoom = 125;
      return board;
    });
  };

  var key = function (init, el) { (el || document.activeElement || document.body).dispatchEvent(new KeyboardEvent('keydown', Object.assign({ bubbles: true, cancelable: true, metaKey: true }, init))); };
  var steps = function () { return sent('zoom').map(function (a) { return a.step; }).join(' '); };
  var toast = function () { return Array.prototype.map.call(document.querySelectorAll('.ca-toast'), function (t) { return t.textContent; }).join(' | '); };
  var rows = function () { return document.querySelectorAll('.ca-lane.is-agent'); };
  var canvas = function () { return document.querySelector('.ca-cv-zoom'); };

  if (localStorage.getItem('codealong.view') === 'canvas') {
    say('zoom check: waiting for the canvas');
    until(function () { return !!canvas(); }, 20000)
      .then(function () { return wait(300); })
      .then(function () {
        var before = canvas().textContent;
        document.body.focus();
        key({ key: '=', code: 'Equal' }, document.body);
        return wait(300).then(function () {
          check('on the canvas ⌘= zooms the canvas, not the window', steps() === '' && canvas().textContent !== before, before + ' → ' + canvas().textContent + '; zoom asked: ' + steps());
          key({ key: '0', code: 'Digit0' }, document.body);
          return wait(300);
        });
      })
      .then(function () {
        check('and ⌘0 puts the canvas back', steps() === '' && canvas().textContent === '100%', canvas().textContent);
        done();
      })
      .catch(stopped);
    return;
  }

  say('zoom check: waiting for the list');
  until(function () { return rows().length > 0; }, 20000)
    .then(function () { return wait(300); })
    .then(function () {
      document.body.focus();
      key({ key: '=', code: 'Equal' });
      return wait(200);
    })
    .then(function () {
      check('⌘= asks the engine to zoom in', steps() === 'in', steps());
      check('and says where it got to', /Zoomed to 110%\./.test(toast()), toast());
      key({ key: '+', code: 'Equal', shiftKey: true });
      key({ key: '-', code: 'Minus' });
      key({ key: '0', code: 'Digit0' });
      return wait(200);
    })
    .then(function () {
      check('⌘+ zooms in, ⌘- out and ⌘0 back to actual size', steps() === 'in in out actual', steps());
      key({ key: '-', code: 'Minus', metaKey: false, ctrlKey: true });
      key({ key: '0', code: 'Digit0', altKey: true });
      return wait(200);
    })
    .then(function () {
      check('⌃- and ⌥⌘0 keep their own keys', steps() === 'in in out actual', steps());
      window.__emit('zoom', 'out');
      return wait(200);
    })
    .then(function () {
      check('View’s Zoom out asks the same', / out$/.test(steps()), steps());
      rows()[0].dispatchEvent(new MouseEvent('mousedown', { bubbles: true, cancelable: true }));
      return until(function () { return !!document.querySelector('.ca-sessinsp .ca-composer textarea'); }, 5000);
    })
    .then(function () {
      var box = document.querySelector('.ca-sessinsp .ca-composer textarea');
      box.focus();
      var n = sent('zoom').length;
      key({ key: '=', code: 'Equal' }, box);
      return wait(200).then(function () {
        check('from a session’s box too', sent('zoom').length === n + 1 && / in$/.test(steps()), steps());
      });
    })
    .then(function () {
      var more = document.querySelector('.ca-sessinsp button[aria-label="More"]');
      if (!more) {
        note('skipped: the session’s header has no More button');
        return;
      }
      var r = more.getBoundingClientRect();
      more.click();
      return wait(300).then(function () {
        var at = (calls('popup_menu').pop() || [])[1];
        var p = at && at.at && at.at.Logical;
        var want = { x: r.left * 1.25, y: (r.bottom + 4) * 1.25 };
        check('a menu dropped from a button lands under it at 125%', !!p && Math.abs(p.x - want.x) < 0.5 && Math.abs(p.y - want.y) < 0.5, JSON.stringify(p) + ' wanted ' + JSON.stringify(want));
      });
    })
    .then(done)
    .catch(stopped);
})();
