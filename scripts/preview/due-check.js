// Checks todos' due dates in the Todo view (due.ts, TodoView.tsx, TodoPanel.tsx): the rows' chips,
// the panel's date and its picker, D, and the Due date submenu for one todo and for several.
//   PREVIEW_WAIT=60000 PREVIEW_TODOS=scripts/preview/todos-sample.json scripts/preview.sh todo '' scripts/preview/due-check.js
// Every step should say ok.
(function () {
  var out = [];
  var say = function (t) { window.__t = t; };
  setInterval(function () { if (window.__t) document.title = window.__t; }, 100);
  var wait = function (ms) { return new Promise(function (r) { setTimeout(r, ms); }); };
  var until = function (f, ms) {
    var s = Date.now();
    return new Promise(function (res, rej) {
      (function p() { var v = f(); if (v) return res(v); if (Date.now() - s > ms) return rej(new Error('timeout')); setTimeout(p, 20); })();
    });
  };
  var check = function (name, ok, why) { out.push(name + (ok ? ' ok' : ' FAIL') + (why ? ' (' + why + ')' : '')); };
  var day = function (n) {
    var d = new Date();
    d = new Date(d.getFullYear(), d.getMonth(), d.getDate() + n);
    return d.getFullYear() + '-' + String(d.getMonth() + 1).padStart(2, '0') + '-' + String(d.getDate()).padStart(2, '0');
  };
  var row = function (title) {
    return Array.prototype.find.call(document.querySelectorAll('.ca-todorow'), function (r) { return r.querySelector('.ca-todorow-title').textContent.indexOf(title) === 0; });
  };
  var chip = function (title) { var r = row(title); var c = r && r.querySelector('.ca-duechip'); return c ? c.textContent + ' ' + c.className.replace(/.*is-/, '') : 'none'; };
  var ops = function () { return window.__calls.filter(function (c) { return c[0] === 'todo_op'; }).map(function (c) { return c[1].op; }); };
  var lastOp = function () { var o = ops(); return JSON.stringify(o[o.length - 1]); };
  var select = function (title, meta) { row(title).dispatchEvent(new MouseEvent('mousedown', { bubbles: true, button: 0, metaKey: !!meta })); };
  var right = function (el) { el.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true })); };
  var item = function (label) { return (window.__menu().find(function (x) { return x[0] === label; }) || [])[1]; };
  var labels = function (prefix) { return window.__menu().map(function (x) { return x[0]; }).filter(function (l) { return l.indexOf(prefix) === 0; }).map(function (l) { return l.slice(prefix.length); }); };
  var panelDue = function () { var n = document.querySelector('.ca-todoinsp .ca-duenote'); return n ? n.textContent : (document.querySelector('.ca-todoinsp .ca-duepick-anchor') || {}).textContent; };
  var setDate = function (input, value) {
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set.call(input, value);
    input.dispatchEvent(new Event('input', { bubbles: true }));
  };

  say('due check: waiting for todos');
  until(function () { return row('Write up the config.toml'); }, 20000)
    .then(function () {
      check('overdue in red', chip('Keep the sidebar') === '2 days overdue overdue', chip('Keep the sidebar'));
      check('due today in amber', chip('Show how long each check') === 'Due today today', chip('Show how long each check'));
      check('tomorrow', chip('Write up the config.toml') === 'Due tomorrow soon', chip('Write up the config.toml'));
      check('within the week by its day', /^Due (Mon|Tue|Wed|Thu|Fri|Sat|Sun) soon$/.test(chip('Publish the release notes')), chip('Publish the release notes'));
      check('later by its date', /^Due [A-Z][a-z]{2} \d+ later$/.test(chip('Cut the board push')), chip('Cut the board push'));
      check('none without one', chip('Try the floating bar') === 'none', chip('Try the floating bar'));
      select('Write up the config.toml');
      return until(function () { return document.querySelector('.ca-todoinsp .ca-duenote'); }, 6000);
    })
    .then(function () {
      check('the panel says the day', /, tomorrow$/.test(panelDue()), panelDue());
      document.querySelector('.ca-duenote-show').click();
      return until(function () { return document.querySelector('.ca-duepick input[type="date"]'); }, 6000);
    })
    .then(function (input) {
      check('the picker opens on its date', input.value === day(1), input.value);
      check('and offers days', Array.prototype.map.call(document.querySelectorAll('.ca-duepick .ca-menu-item .ca-menu-label'), function (l) { return l.textContent; }).join('|').indexOf('Today|Tomorrow|') === 0);
      setDate(input, day(10));
      return wait(50).then(function () {
        input.form.requestSubmit();
        return wait(200);
      });
    })
    .then(function () {
      check('a typed day is set', lastOp() === JSON.stringify({ op: 'update', id: 't-sample4', patch: { due: day(10) } }), lastOp());
      check('and the picker closes', !document.querySelector('.ca-duepick'));
      check('the panel shows it', /in 10 days$/.test(panelDue()), panelDue());
      document.querySelector('.ca-duenote [aria-label="Remove the due date"]').click();
      return wait(200);
    })
    .then(function () {
      check('x takes it off', lastOp() === JSON.stringify({ op: 'update', id: 't-sample4', patch: { due: null } }), lastOp());
      check('then it offers one', /Add due date/.test(panelDue()), panelDue());
      document.activeElement && document.activeElement.blur && document.activeElement.blur();
      window.dispatchEvent(new KeyboardEvent('keydown', { key: 'd', code: 'KeyD', bubbles: true }));
      return until(function () { return document.querySelector('.ca-duepick input[type="date"]'); }, 6000).catch(function () { return null; });
    })
    .then(function (input) {
      check('D opens the picker', !!input);
      if (input) {
        check('on today when it has no date', input.value === day(0), input.value);
        input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
      }
      return wait(200);
    })
    .then(function () {
      check('Escape closes it', !document.querySelector('.ca-duepick'));
      right(row('Keep the sidebar'));
      return wait(300);
    })
    .then(function () {
      var days = labels('Due date › ');
      check('the menu has a Due date submenu', days[0] === 'Today' && days[1] === 'Tomorrow' && days.indexOf('Pick a date…') > 0 && days[days.length - 1] === 'Remove due date', days.join('|'));
      item('Due date › Today')();
      return wait(200);
    })
    .then(function () {
      check('Today sets today', lastOp() === JSON.stringify({ op: 'update', id: 't-sample1', patch: { due: day(0) } }), lastOp());
      right(row('Try the floating bar'));
      return wait(300);
    })
    .then(function () {
      check('no Remove without a date', labels('Due date › ').indexOf('Remove due date') < 0, labels('Due date › ').join('|'));
      item('Due date › Pick a date…')();
      return until(function () { return document.querySelector('.ca-duepick input[type="date"]'); }, 6000).catch(function () { return null; });
    })
    .then(function (input) {
      var title = document.querySelector('.ca-todoinsp-title');
      check('Pick a date… shows that todo with its picker open', !!input && title && title.value.indexOf('Try the floating bar') === 0, title && title.value);
      if (input) input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
      select('Show how long each check');
      return wait(100).then(function () {
        select('Publish the release notes', true);
        return wait(200);
      });
    })
    .then(function () {
      right(row('Publish the release notes'));
      return wait(300);
    })
    .then(function () {
      var days = labels('Due date › ');
      check('several picked get one submenu, without Pick a date…', days.indexOf('Pick a date…') < 0 && days[days.length - 1] === 'Remove due dates', days.join('|'));
      var before = ops().length;
      item('Due date › Tomorrow')();
      return wait(200).then(function () {
        var made = ops().slice(before).map(function (o) { return o.id + '=' + o.patch.due; }).sort().join(' ');
        check('and set each', made === 't-sample2=' + day(1) + ' t-sample7=' + day(1), made);
      });
    })
    .then(function () { say('done: ' + out.join('; ')); })
    .catch(function (e) { say('done: ' + out.join('; ') + '; FAILED ' + e.message); });
})();
