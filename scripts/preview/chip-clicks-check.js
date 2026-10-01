// Checks the chips on the lists' rows (LaneRow.tsx): a click on one selects its row, as a click
// anywhere on the row does, and opens nothing; a double-click does the chip's own thing (the PR
// on GitHub, the session where it runs, the todo in Todo, the app in Chrome) in place of the
// row's. Sessions has PR, agent and todo chips, Worktrees the app's.
//   PREVIEW_TODOS=scripts/preview/todos-sample.json PREVIEW_WAIT=60000 scripts/preview.sh sessions '' scripts/preview/chip-clicks-check.js
//   PREVIEW_WAIT=60000 scripts/preview.sh worktrees '' scripts/preview/chip-clicks-check.js
// Every step should say ok; a chip no row has says so and is skipped.
(function () {
  var out = [];
  var say = function (t) { window.__t = t; };
  setInterval(function () { if (window.__t) document.title = window.__t; }, 100);
  var wait = function (ms) { return new Promise(function (r) { setTimeout(r, ms); }); };
  var until = function (f, ms) {
    var s = Date.now();
    return new Promise(function (res, rej) {
      (function p() { if (f()) return res(); if (Date.now() - s > ms) return rej(new Error('timeout')); setTimeout(p, 20); })();
    });
  };
  var check = function (name, ok, why) { out.push(name + (ok ? ' ok' : ' FAIL') + (why ? ' (' + why + ')' : '')); };
  var list = function () { return document.querySelector('[role="listbox"]'); };
  var fire = function (el, type, detail) { el.dispatchEvent(new MouseEvent(type, { bubbles: true, cancelable: true, button: 0, detail: detail })); };
  var click = function (el) { fire(el, 'mousedown', 1); fire(el, 'mouseup', 1); fire(el, 'click', 1); };
  var dbl = function (el) { click(el); fire(el, 'mousedown', 2); fire(el, 'mouseup', 2); fire(el, 'click', 2); fire(el, 'dblclick', 2); };
  var opens = function (from) { return window.__calls.slice(from).filter(function (c) { return c[0] === 'open_target' || c[0] === 'open_url'; }); };
  var said = function (cs) { return cs.map(function (c) { return c[0] + ' ' + (c[1].url || c[1].targetId); }).join(', ') || 'none'; };
  // A chip on a row that isn't selected, so the click has something to do.
  var chipOn = function (sel) {
    return Array.prototype.filter.call(document.querySelectorAll('[role="option"] ' + sel), function (c) {
      var r = c.closest('[role="option"]');
      return r && !r.classList.contains('is-selected') && r.getBoundingClientRect().height > 0;
    })[0];
  };

  var step = function (name, sel, after) {
    return function () {
      var chip = chipOn(sel);
      if (!chip) {
        out.push(name + ': no row has one, skipped');
        return;
      }
      var row = chip.closest('[role="option"]');
      var n = window.__calls.length;
      click(chip);
      return wait(300).then(function () {
        check(name + ': a click selects its row', row.classList.contains('is-selected') || row.getAttribute('aria-selected') === 'true');
        check(name + ': and opens nothing', opens(n).length === 0, said(opens(n)));
        check(name + ': its tooltip says double-click', /Double-click to/.test(chip.title), chip.title);
        n = window.__calls.length;
        dbl(chip);
        return wait(400);
      }).then(function () { return after(opens(n), row); });
    };
  };

  say('chip clicks check: waiting for the list');
  until(function () { return document.querySelector('[role="option"]'); }, 20000)
    .then(function () { return wait(500); })
    .then(function () {
      var label = list().getAttribute('aria-label');
      if (label === 'Sessions') {
        return Promise.resolve()
          .then(step('PR chip', '.ca-pr', function (o) {
            check('PR chip: a double-click opens the PR, not the session', o.length === 1 && o[0][0] === 'open_url', said(o));
          }))
          .then(step('agent chip', '.ca-agentrow-line1 .ca-chip', function (o) {
            check('agent chip: a double-click opens the session once', o.length === 1 && o[0][0] === 'open_target', said(o));
          }))
          .then(step('todo chip', '.ca-todochip', function (o) {
            return wait(200).then(function () {
              check('todo chip: a double-click shows it in Todo', list().getAttribute('aria-label') === 'Todos' && o.length === 0, list().getAttribute('aria-label') + ', ' + said(o));
            });
          }));
      }
      return Promise.resolve()
        .then(step('app chip', '.ca-previewchip', function (o) {
          check('app chip: a double-click opens the app, not the session', o.length === 1 && o[0][1].targetId === 'l|preview', said(o));
        }))
        .then(step('agent chip', '.ca-agentline-main .ca-chip', function (o) {
          check('agent chip: a double-click opens the session once', o.length === 1 && o[0][0] === 'open_target', said(o));
        }));
    })
    .catch(function (e) { check('ran', false, String(e)); })
    .then(function () { say('done: ' + out.join('; ')); });
})();
