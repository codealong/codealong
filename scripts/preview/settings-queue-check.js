// Checks Settings › Queue (`[queue]`, config.rs `QueueCfg`): what waits on you, in the default
// order to begin with, the sessions' two always in it (their boxes on and fixed) and the todos
// ready to start after the list. A box taken off saves the order without that kind and lists it
// under Not in the queue; put back, it goes where the default has it. ⌃⌘↓ on a grip moves its
// kind a place, the keyboard going with it, and one dragged by its row goes where it's let go.
// Todos come due saves `due_within`. config.toml reads back as it was saved.
//   scripts/preview.sh sessions '' scripts/preview/settings-queue-check.js
(function () {
  // config.toml reads back as Settings saved it, as the engine would.
  var latest = null;
  var invoke = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    if (cmd === 'save_config') latest = args.config;
    var p = invoke(cmd, args, options);
    if (cmd !== 'get_config') return p;
    return p.then(function (view) {
      if (!latest) {
        var config = Object.assign({}, view.config);
        delete config.queue;
        latest = config;
      }
      return Object.assign({}, view, { config: latest });
    });
  };
  var nav = function (label) { return all('.ca-settings-nav button').filter(function (b) { return b.textContent.trim() === label; })[0]; };
  var field = function (words) { return all('.ca-settings-pane .ca-field').filter(function (f) { var s = f.querySelector(':scope > span:not(.ca-checkbox)'); return s && words.test(s.textContent); })[0]; };
  var kinds = function (within) { return within ? all('.ca-queue-kind', within).map(function (r) { return r.dataset.kind; }) : []; };
  var listed = function () { return kinds(q('.ca-queue-kinds')); };
  var row = function (kind) { return q('.ca-queue-kind[data-kind="' + kind + '"]'); };
  var box = function (kind) { return q('input[type="checkbox"]', row(kind)); };
  var lastSave = function () { var c = calls('save_config'); return c.length ? c[c.length - 1][1].config : null; };
  var saved = function () { var c = lastSave(); return c && c.queue ? c.queue.order.join() : ''; };
  var pointer = function (el, type, x, y) {
    el.dispatchEvent(new PointerEvent(type, { bubbles: true, cancelable: true, button: type === 'pointermove' ? -1 : 0, buttons: type === 'pointerup' ? 0 : 1, clientX: x, clientY: y, pointerType: 'mouse', isPrimary: true }));
  };
  var middle = function (el) {
    var r = el.getBoundingClientRect();
    return { x: (r.left + r.right) / 2, y: (r.top + r.bottom) / 2 };
  };

  (async function () {
    try {
      (await until(function () { return q('.ca-side-config'); }, 20000, 'the settings button')).click();
      (await until(function () { return nav('Queue'); }, 20000, 'Queue in Settings')).click();
      await until(function () { return q('.ca-queue-kinds'); }, 20000, 'the queue’s list');
      var inQueue = field(/^In the queue$/);
      check('the default order to begin with, the todos ready to start after the list', listed().join() === 'needs,reviews,due,unread' && kinds(inQueue).join() === 'needs,reviews,due,unread,ready', kinds(inQueue).join());
      check('nothing is out of it', !field(/^Not in the queue$/), String(!!field(/^Not in the queue$/)));
      check('the sessions’ two are always in it: their boxes on and fixed', ['needs', 'unread'].every(function (k) { return box(k).checked && box(k).disabled; }) && ['reviews', 'due', 'ready'].every(function (k) { return box(k).checked && !box(k).disabled; }), ['needs', 'reviews', 'due', 'unread', 'ready'].map(function (k) { return k + ':' + box(k).checked + '/' + box(k).disabled; }).join(' '));
      check('the ones in the list have grips, the ready ones none', all('.ca-queue-kinds .ca-queue-grip').length === 4 && !q('.ca-queue-grip', row('ready')), String(all('.ca-queue-grip').length));

      box('reviews').click();
      await until(function () { return saved() === 'needs,due,unread,ready'; }, 5000, 'reviews taken out');
      await until(function () { return field(/^Not in the queue$/); }, 5000, 'Not in the queue');
      check('a box taken off saves the order without it and lists it under Not in the queue, dimmed', kinds(field(/^Not in the queue$/)).join() === 'reviews' && row('reviews').classList.contains('is-off') && listed().join() === 'needs,due,unread', listed().join() + ' / ' + kinds(field(/^Not in the queue$/)).join());

      box('reviews').click();
      await until(function () { return saved() === 'needs,reviews,due,unread,ready'; }, 5000, 'reviews put back');
      check('put back, it goes where the default has it', listed().join() === 'needs,reviews,due,unread', listed().join());

      var grip = q('.ca-queue-grip', row('needs'));
      grip.focus();
      key(grip, { key: 'ArrowDown', ctrlKey: true, metaKey: true });
      await until(function () { return saved() === 'reviews,needs,due,unread,ready'; }, 5000, 'needs moved down');
      await until(function () { return listed().join() === 'reviews,needs,due,unread'; }, 5000, 'the list in its new order');
      await wait(50);
      check('⌃⌘↓ on a grip moves it a place, the keyboard going with it', document.activeElement === q('.ca-queue-grip', row('needs')), document.activeElement && document.activeElement.outerHTML.slice(0, 80));

      // Due dragged by its row from third to first.
      var from = middle(row('due'));
      var to = middle(row('reviews'));
      pointer(row('due'), 'pointerdown', from.x, from.y);
      pointer(window, 'pointermove', from.x, from.y - 6);
      pointer(window, 'pointermove', from.x, to.y - 12);
      pointer(window, 'pointerup', from.x, to.y - 12);
      await until(function () { return saved() === 'due,reviews,needs,unread,ready'; }, 5000, 'due dragged to the top');
      check('one dragged by its row goes where it’s let go', listed().join() === 'due,reviews,needs,unread', listed().join());
      // The click the drag's release makes is swallowed until the next turn (useSort.ts).
      await wait(50);

      box('ready').click();
      await until(function () { return saved() === 'due,reviews,needs,unread'; }, 5000, 'the ready ones taken out');
      check('the todos ready to start taken out are listed under Not in the queue', kinds(field(/^Not in the queue$/)).join() === 'ready' && !row('ready').closest('.ca-queue-kinds'), kinds(field(/^Not in the queue$/)).join());

      var select = q('select', field(/^Todos come due$/));
      check('todos come due on their date to begin with', select.value === '0', select.value);
      Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype, 'value').set.call(select, '1');
      select.dispatchEvent(new Event('change', { bubbles: true }));
      await until(function () { return lastSave().queue.due_within === 1; }, 5000, 'a day before saved');
      check('a day before saves [queue] due_within, the order as it was', saved() === 'due,reviews,needs,unread' && lastSave().queue.due_within === 1, JSON.stringify(lastSave().queue));
      done();
    } catch (e) {
      stopped(e);
    }
  })();
})();
