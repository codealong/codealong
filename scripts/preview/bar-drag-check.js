// Checks dragging a session's pill off the floating bar onto the desktop (Bar.tsx): a press that
// moves a little is still a click, one that moves far enough hands the drag to the engine
// (drag_to_desktop, which moves the card with the pointer), the click it ends in does nothing, and
// the pill looks lifted until the engine says the drag is over. And the pill's menu pins it.
//   PREVIEW_WAIT=60000 scripts/preview.sh bar '' scripts/preview/bar-drag-check.js
(function () {
  var find = function (items, text) { return items.filter(function (i) { return i.text === text; })[0]; };
  var pointer = function (el, type, x, y, buttons) {
    el.dispatchEvent(new PointerEvent(type, { bubbles: true, cancelable: true, button: type === 'pointerdown' ? 0 : -1, buttons: buttons, screenX: x, screenY: y, clientX: x, clientY: y, pointerType: 'mouse' }));
  };
  var click = function (el) { el.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true, detail: 1 })); };
  var pill = function () { return document.querySelector('.ca-hud [data-key]'); };
  var shows = function () { return calls('show_main').length; };

  say('bar drag check: waiting for pills');
  var key = null;
  until(function () { return !!pill(); }, 20000)
    .then(function () {
      var el = pill();
      key = el.dataset.key;
      var before = shows();
      pointer(el, 'pointerdown', 100, 20, 1);
      pointer(el, 'pointermove', 103, 21, 1);
      click(el);
      check('a press that barely moves is no drag', calls('drag_to_desktop').length === 0);
      return wait(600).then(function () {
        check('and still a click', shows() === before + 1, String(shows() - before));
      });
    })
    .then(function () {
      var el = pill();
      var before = shows();
      pointer(el, 'pointerdown', 100, 20, 1);
      pointer(el, 'pointermove', 104, 22, 1);
      pointer(el, 'pointermove', 120, 60, 1);
      var d = last('drag_to_desktop');
      check('one that moves far enough drags its session off the bar', calls('drag_to_desktop').length === 1 && d && d.pin.ref === 'session' && d.pin.key === key, JSON.stringify(d));
      pointer(el, 'pointermove', 160, 120, 1);
      check('once', calls('drag_to_desktop').length === 1);
      check('and no peek opens as it goes', calls('peek_open').length === 0, String(calls('peek_open').length));
      return wait(50).then(function () {
        check('the pill looks lifted', pill().classList.contains('is-dragged'));
        click(pill());
        return wait(600).then(function () {
          check('the click it ends in does nothing', shows() === before && calls('open_from_bar').length === 0, String(shows() - before));
        });
      });
    })
    .then(function () {
      window.__emit('desktop-drag-end', { kept: true });
      return wait(100);
    })
    .then(function () {
      check('and settles when the engine says the drag is over', !pill().classList.contains('is-dragged'));
      // The menu asks for the session first, which takes a while on a busy Mac.
      var menus = calls('plugin:menu|new').length;
      rightClick(pill());
      return until(function () { return calls('plugin:menu|new').length > menus; }, 5000).then(function () { return wait(50); });
    })
    .then(function () {
      var item = find(lastMenu(), 'Pin to desktop');
      check("the pill's menu pins it to the desktop", !!item);
      if (item) pick(item);
      var p = last('pin_to_desktop');
      check('as a session', p && p.pin.ref === 'session' && p.pin.key === key, JSON.stringify(p));
      window.__emit('desktop-pins', [{ ref: 'session', key: key }]);
      return wait(100);
    })
    .then(function () {
      // The menu asks for the session first, which takes a while on a busy Mac.
      var menus = calls('plugin:menu|new').length;
      rightClick(pill());
      return until(function () { return calls('plugin:menu|new').length > menus; }, 5000).then(function () { return wait(50); });
    })
    .then(function () {
      var item = find(lastMenu(), 'Remove from desktop');
      check('and once pinned takes it off', !!item);
      if (item) pick(item);
      var p = last('unpin_from_desktop');
      check('the same session', p && p.pin.key === key, JSON.stringify(p));
      done();
    })
    .catch(stopped);
})();
