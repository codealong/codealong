// Checks that a dialog keeps the keyboard (primitives.tsx `Dialog`, focusTrap.ts), on Settings
// and on Rename session: it has the keyboard as it opens (the field that asks for it, else the
// dialog itself), Tab and ⇧Tab go round its controls and never out to the window behind, a Tab
// from outside comes in, Escape closes it, and what had the keyboard before has it again.
//   PREVIEW_WAIT=60000 scripts/preview.sh sessions '' scripts/preview/dialog-focus-check.js
// A scripted Tab moves nothing by itself, so where the page leaves the key alone the check moves
// the focus to the next control in the page, as the browser would.
(function () {
  var dialog = function () { return q('.ca-dialog'); };
  var active = function () { return document.activeElement; };
  var named = function (el) { return !el ? 'nothing' : el === document.body ? 'the page' : (el.getAttribute('aria-label') || el.textContent || el.tagName).trim().slice(0, 30) || el.tagName; };
  var stops = function (root) {
    return Array.prototype.filter.call(root.querySelectorAll('a[href], button, input, select, textarea, [tabindex]'), function (el) {
      return el.tabIndex >= 0 && !el.matches(':disabled') && el.getClientRects().length > 0;
    });
  };
  var tab = function (back) {
    var el = active() || document.body;
    var e = new KeyboardEvent('keydown', { key: 'Tab', shiftKey: !!back, bubbles: true, cancelable: true });
    el.dispatchEvent(e);
    if (e.defaultPrevented) return;
    var every = stops(document);
    var i = every.indexOf(el);
    var next = back ? every[(i <= 0 ? every.length : i) - 1] : every[(i + 1) % every.length];
    if (next) next.focus();
  };
  var escape = function () {
    var e = new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true });
    (active() || document.body).dispatchEvent(e);
    return e;
  };
  // Tabs all the way round and once more, each way: every stop has to be in the dialog.
  var goesRound = function (name) {
    var inside = stops(dialog());
    var left = [];
    var seen = [];
    for (var i = 0; i < inside.length + 2; i++) {
      tab(false);
      if (!dialog().contains(active())) left.push(named(active()));
      seen.push(active());
    }
    check(name + ': Tab stays in it', left.length === 0, left.join(', '));
    check(name + ': and reaches every control', inside.every(function (el) { return seen.indexOf(el) >= 0; }), inside.length + ' controls');
    inside[0].focus();
    tab(true);
    check(name + ': ⇧Tab from the first goes to the last', active() === inside[inside.length - 1], named(active()));
    tab(false);
    check(name + ': Tab from the last goes to the first', active() === inside[0], named(active()));
    active().blur();
    tab(false);
    check(name + ': a Tab from outside comes in', dialog().contains(active()), named(active()));
  };

  var opener = null;
  say('dialog focus check: waiting for the list');
  until(function () { return q('[role="listbox"] [role="option"]') && q('.ca-side-config'); }, 20000, 'the list and Settings')
    .then(function () { return wait(300); })
    .then(function () {
      opener = q('.ca-side-config');
      opener.focus();
      opener.click();
      return until(dialog, 5000);
    })
    .then(function () { return wait(300); })
    .then(function () {
      check('Settings: has the keyboard as it opens', dialog().contains(active()), named(active()));
      check('Settings: is named by its title', (document.getElementById(dialog().getAttribute('aria-labelledby')) || {}).textContent === 'Settings');
      goesRound('Settings');
      var e = escape();
      return wait(200).then(function () {
        check('Settings: Escape closes it', !dialog() && e.defaultPrevented);
        check('Settings: and the button that opened it has the keyboard again', active() === opener, named(active()));
      });
    })
    .then(function () {
      var row = q('[role="listbox"] [role="option"]');
      q('[role="listbox"]').focus();
      opener = active();
      row.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true }));
      return wait(300).then(function () {
        var item = window.__menu().filter(function (m) { return /^Rename…/.test(m[0]); })[0];
        if (!item) return note('Rename: the first session takes no name here, skipped');
        item[1]();
        return until(dialog, 5000).then(function () { return wait(300); }).then(function () {
          var field = q('.ca-renamedlg input');
          check('Rename: its field has the keyboard as it opens', active() === field, named(active()));
          check('Rename: with the name selected', field.selectionStart === 0 && field.selectionEnd === field.value.length && field.value.length > 0);
          goesRound('Rename');
          var e = escape();
          return wait(200).then(function () {
            check('Rename: Escape closes it', !dialog() && e.defaultPrevented);
            check('Rename: and the list has the keyboard again', active() === opener, named(active()));
          });
        });
      });
    })
    .then(done)
    .catch(stopped);
})();
