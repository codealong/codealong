// Checks how popovers go away (ui/useDismiss.ts), in the Todo view: a picker under its chip
// closes on a press anywhere else, a press on a chip that keeps its mousedown from its row
// included; a press inside it or on its own chip leaves it up; opening another picker closes
// the first; Escape closes the one that's up from wherever the keyboard is and is taken, so the
// window's own Escape (leaving several picked todos) doesn't run too; and the project menu
// closes the same two ways.
//   PREVIEW_TODOS=scripts/preview/todos-sample.json PREVIEW_WAIT=60000 scripts/preview.sh todo '' scripts/preview/dismiss-check.js
// Every step should say ok.
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
  var q = function (sel) { return document.querySelector(sel); };
  // A press as WebKit sends it: the pointer event, then the mouse event it stands for.
  var press = function (el) {
    el.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, cancelable: true, button: 0, buttons: 1 }));
    el.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, cancelable: true, button: 0 }));
  };
  var escape = function (el) {
    var e = new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true });
    el.dispatchEvent(e);
    return e;
  };
  var rows = function () { return Array.prototype.slice.call(document.querySelectorAll('.ca-todorow')); };
  var add = function (anchor) { return q('.ca-todoinsp ' + anchor + ' > .ca-labelchip--add'); };
  var LABEL = '.ca-labelpick-anchor:not(.ca-duepick-anchor):not(.ca-waitpick-anchor)';
  var labels = function () { return q('.ca-todoinsp ' + LABEL + ' .ca-labelpick'); };
  var openLabels = function () {
    if (!labels()) add(LABEL).click();
    return until(labels, 3000);
  };

  say('dismiss check: waiting for the todos');
  until(function () { return rows().length > 1; }, 20000)
    .then(function () {
      // A todo without a due date, so its Due line has the chip that adds one.
      var row = rows().filter(function (r) { return !r.querySelector('.ca-duechip'); })[0] || rows()[0];
      press(row);
      return until(function () { return add(LABEL); }, 5000);
    })
    .then(openLabels)
    .then(function () {
      check('the picker opens under its chip', !!labels());
      press(labels().querySelector('.ca-labelpick-list'));
      press(add(LABEL));
      return wait(100);
    })
    .then(function () {
      check('a press inside it or on its chip leaves it up', !!labels());
      press(q('.ca-todoinsp-title'));
      return wait(100);
    })
    .then(function () {
      check('a press anywhere else closes it', !labels());
      return openLabels();
    })
    .then(function () {
      // An agent's chip on a todo's row is a button that keeps its mousedown from the row.
      var chip = q('.ca-todorow .ca-chip--button');
      if (!chip) return out.push('a press on a chip that stops its mousedown: no row has one, skipped');
      press(chip);
      return wait(100).then(function () { check('a press on a chip that stops its mousedown closes it', !labels()); });
    })
    .then(openLabels)
    .then(function () {
      var due = add('.ca-duepick-anchor');
      if (!due) return out.push('another picker: the todo has a due date, skipped');
      press(due);
      due.click();
      return until(function () { return q('.ca-duepick'); }, 3000).then(function () {
        check('opening another picker closes the first', !labels() && !!q('.ca-duepick'));
        var e = escape(q('.ca-duepick input'));
        return wait(100).then(function () { check('Escape in its field closes it', !q('.ca-duepick') && e.defaultPrevented); });
      });
    })
    .then(openLabels)
    .then(function () {
      // Two todos picked: Escape on the window goes back to one. With a picker up it's the picker's.
      var others = rows().filter(function (r) { return r.getAttribute('aria-selected') !== 'true'; });
      document.activeElement && document.activeElement.blur();
      var e = escape(document.body);
      return wait(100).then(function () {
        check('Escape closes it wherever the keyboard is', !labels());
        check('and is taken', e.defaultPrevented);
        return others;
      });
    })
    .then(function () {
      var button = q('.ca-projswitch-btn');
      button.click();
      return until(function () { return q('.ca-projswitch .ca-menu'); }, 3000).then(function () {
        var e = escape(document.body);
        return wait(100).then(function () {
          check('the project menu closes on Escape, which it takes', !q('.ca-projswitch .ca-menu') && e.defaultPrevented);
          button.click();
          return until(function () { return q('.ca-projswitch .ca-menu'); }, 3000);
        });
      }).then(function () {
        press(q('.ca-todoinsp-title'));
        return wait(100).then(function () { check('and on a press elsewhere', !q('.ca-projswitch .ca-menu')); });
      });
    })
    .then(function () {
      var e = escape(document.body);
      check('with nothing up, Escape is left to the window', !e.defaultPrevented);
    })
    .catch(function (e) { check('ran', false, String(e)); })
    .then(function () { say('done: ' + out.join('; ')); });
})();
