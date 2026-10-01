// Checks dragging results out of the quick panel (Quick.tsx, useRowDrag.tsx): a session's or a
// todo's row dragged tells the engine what its card would be (drag_to_desktop, which pins it
// once the pointer leaves the panel), the click the drag ends in picks nothing, a plain click
// still picks, and the row that starts a session can't be dragged.
//   PREVIEW_WAIT=60000 scripts/preview.sh quick '' scripts/preview/quick-drag-check.js
// Every step should say ok. The board needs a session the panel lists.
(function () {
  var out = [];
  var say = function (t) { window.__t = t; };
  setInterval(function () { if (window.__t) document.title = window.__t; }, 100);
  var wait = function (ms) { return new Promise(function (r) { setTimeout(r, ms); }); };
  var until = function (f, ms) {
    var s = Date.now();
    return new Promise(function (res, rej) {
      (function p() { if (f()) return res(); if (Date.now() - s > ms) return rej(new Error('timeout')); setTimeout(p, 10); })();
    });
  };
  var check = function (name, ok, why) { out.push(name + (ok ? ' ok' : ' FAIL') + (why ? ' (' + why + ')' : '')); };
  var done = function () { say('done: ' + out.join(' · ')); };
  localStorage.setItem('codealong.quick.project', 'All projects');
  var calls = function (cmd) { return window.__calls.filter(function (c) { return c[0] === cmd; }); };
  var last = function (cmd) { var c = calls(cmd); return c.length ? c[c.length - 1][1] : null; };
  var pointer = function (target, type, x, y) {
    target.dispatchEvent(new PointerEvent(type, { bubbles: true, cancelable: true, button: 0, buttons: type === 'pointerup' ? 0 : 1, clientX: x, clientY: y, pointerId: 1, isPrimary: true, pointerType: 'mouse' }));
  };
  var click = function (el) { el.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true, detail: 1 })); };
  var rows = function () { return Array.prototype.slice.call(document.querySelectorAll('.ca-quick-group .ca-quick-row:not(.is-more)')); };
  var at = function (el) { var r = el.getBoundingClientRect(); return { x: r.left + r.width / 2, y: r.top + r.height / 2 }; };
  var ghost = function () { return document.querySelector('.ca-dragghost'); };

  say('quick drag check: waiting for the panel');
  until(function () { return calls('quick_ready').length > 0 && rows().length > 0; }, 20000)
    .then(function () {
      var row = rows()[0];
      var p = at(row);
      var shown = calls('show_main').length;
      pointer(row, 'pointerdown', p.x, p.y);
      pointer(window, 'pointermove', p.x + 30, p.y + 4);
      return wait(50).then(function () {
        var d = last('drag_to_desktop');
        check('a result dragged tells the engine what its card would be', calls('drag_to_desktop').length === 1 && d && (d.pin.ref === 'session' || d.pin.ref === 'todo') && !!d.pin.key, JSON.stringify(d));
        check('the pointer holds its title', !!ghost() && ghost().textContent.length > 0 && !/^Pin/.test(ghost().textContent), ghost() && ghost().textContent);
        check('the row dims', rows()[0].classList.contains('is-drag-source'));
        pointer(window, 'pointermove', p.x, -30);
        return wait(50);
      }).then(function () {
        check('outside the panel the pointer holds nothing here', getComputedStyle(ghost()).display === 'none');
        // Back over the row, and let go on it: the click that follows picks nothing.
        pointer(window, 'pointermove', p.x + 6, p.y);
        pointer(window, 'pointerup', p.x + 6, p.y);
        click(rows()[0]);
        return wait(600);
      }).then(function () {
        check('the click a drag ends in picks nothing', calls('show_main').length === shown && calls('hide_quick').length === 0, String(calls('show_main').length - shown));
        check('and the drag is over', !ghost() && !document.querySelector('.is-drag-source'));
        check('once', calls('drag_to_desktop').length === 1);
      });
    })
    .then(function () {
      var compose = document.querySelector('.ca-quick-compose');
      var p = at(compose);
      pointer(compose, 'pointerdown', p.x, p.y);
      pointer(window, 'pointermove', p.x + 40, p.y - 40);
      pointer(window, 'pointerup', p.x + 40, p.y - 40);
      check('the row that starts a session is not dragged', calls('drag_to_desktop').length === 1 && !ghost());
      var row = rows()[0];
      var q = at(row);
      pointer(row, 'pointerdown', q.x, q.y);
      pointer(window, 'pointerup', q.x, q.y);
      click(row);
      return wait(600).then(function () {
        check('a plain click still picks', calls('show_main').length + calls('hide_quick').length > 0);
        done();
      });
    })
    .catch(function (e) {
      out.push('FAIL ' + e);
      done();
    });
})();
