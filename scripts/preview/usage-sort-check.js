// Checks dragging a chip along the usage bar (useSort.ts): a press that barely moves
// is no drag; one that moves along the row lifts the chip and the chips it passes make way;
// Escape puts it back; letting go puts its account in the new place, writes the order
// (order_accounts) and opens nothing. A click still opens the Usage window on its account.
//   scripts/preview.sh worktrees '' scripts/preview/usage-sort-check.js
(function () {
  var row = function () { return q('.ca-usagebar-accts'); };
  var chips = function () { return all('.ca-usagebar-accts > .ca-uchip'); };
  var names = function () { return chips().map(function (c) { return c.querySelector('.ca-uchip-name').textContent; }); };
  var pointer = function (el, type, x, y) {
    el.dispatchEvent(new PointerEvent(type, { bubbles: true, cancelable: true, button: type === 'pointermove' ? -1 : 0, buttons: type === 'pointerup' ? 0 : 1, clientX: x, clientY: y, pointerType: 'mouse', isPrimary: true }));
  };
  var middle = function (el) {
    var r = el.getBoundingClientRect();
    return { x: (r.left + r.right) / 2, y: (r.top + r.bottom) / 2, right: r.right };
  };
  var shift = function (el) { return el.style.transform; };
  // A chip's click opens the Usage window, or signs in for one that asks to.
  var opens = function () { return calls('token_report').length + calls('sign_in').length; };
  var before;

  say('usage sort check: waiting for the chips');
  until(function () { return chips().length >= 2; }, 20000, 'two chips on the usage bar')
    .then(function () {
      before = names();
      var first = chips()[0];
      var at = middle(first);
      pointer(first, 'pointerdown', at.x, at.y);
      pointer(first, 'pointermove', at.x + 2, at.y + 1);
      return wait(50).then(function () {
        check('a press that barely moves is no drag', !row().classList.contains('is-sorting') && !shift(first), row().className);
        pointer(first, 'pointerup', at.x + 2, at.y + 1);
      });
    })
    .then(function () {
      var first = chips()[0];
      var second = chips()[1];
      var at = middle(first);
      var past = middle(second).x + 6;
      pointer(first, 'pointerdown', at.x, at.y);
      pointer(first, 'pointermove', at.x + 10, at.y);
      pointer(first, 'pointermove', past, at.y);
      return until(function () { return row().classList.contains('is-sorting') && /translateX\(-/.test(shift(second)); }, 5000, 'the second chip making way').then(function () {
        check('the dragged chip is lifted and follows the pointer', first.classList.contains('is-lifted') && /translateX\(\d/.test(shift(first)), first.className + ' ' + shift(first));
        check('the chip it passed moves aside', /translateX\(-\d/.test(shift(second)), shift(second));
        window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true }));
        return wait(50);
      }).then(function () {
        check('Escape puts it back', !row().classList.contains('is-sorting') && !shift(first) && !shift(second) && names().join() === before.join(), names().join());
        var opened = opens();
        pointer(first, 'pointerup', at.x, at.y);
        // A real release over the chip clicks it too.
        first.click();
        return wait(300).then(function () {
          check('and writes nothing', calls('order_accounts').length === 0);
          check('and letting go after opens nothing', opens() === opened && !q('.ca-usage'), opens() - opened);
        });
      });
    })
    .then(function () {
      var first = chips()[0];
      var second = chips()[1];
      var at = middle(first);
      var to = middle(second).x + 6;
      var opened = opens();
      pointer(first, 'pointerdown', at.x, at.y);
      pointer(first, 'pointermove', at.x + 10, at.y);
      pointer(first, 'pointermove', to, at.y);
      pointer(first, 'pointerup', to, at.y);
      first.click();
      var want = [before[1], before[0]].concat(before.slice(2));
      return until(function () { return names().join() === want.join(); }, 5000, 'the new order').then(function () {
        return wait(300);
      }).then(function () {
        var asked = last('order_accounts');
        check('letting go puts the account in its new place', names().join() === want.join(), names().join());
        check('and writes the order', !!asked && asked.accounts.join() === want.join(), JSON.stringify(asked));
        check('the chips stand still again', !row().classList.contains('is-sorting') && chips().every(function (c) { return !shift(c); }), chips().map(shift).join('|'));
        check('the click it ends in opens nothing', opens() === opened && !q('.ca-usage'), opens() - opened);
      });
    })
    .then(function () {
      var chip = chips().find(function (c) { return !c.classList.contains('is-attn'); });
      if (!chip) return note('every chip asks to sign in: no click to try');
      var name = chip.querySelector('.ca-uchip-name').textContent;
      var at = middle(chip);
      pointer(chip, 'pointerdown', at.x, at.y);
      pointer(chip, 'pointerup', at.x, at.y);
      chip.click();
      return until(function () { return q('.ca-usage') && last('token_report') && last('token_report').account === name; }, 5000, 'the Usage window').then(function () {
        check('a click still opens the Usage window on its account', true);
      });
    })
    .then(done)
    .catch(stopped);
})();
