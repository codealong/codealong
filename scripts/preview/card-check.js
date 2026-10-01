// Checks a session's card (SessionCard.tsx), the same in the bar's peek, a session kept on the
// desktop and the quick panel's reply view: its header leaves snoozing, archiving, pinning and
// showing it in Codealong to its menu; the ⋯ button and a right-click give that menu, with the
// Sessions list's shortcuts beside what they run; and the shortcuts run them (⌃⌘P pins, ⌃⌘R
// renames, ⌘E marks seen, ⌃⌘A archives).
//   PREVIEW_WAIT=60000 scripts/preview.sh peek '' scripts/preview/card-check.js
//   PREVIEW_WAIT=60000 scripts/preview.sh kept '' scripts/preview/card-check.js
//   PREVIEW_WAIT=60000 scripts/preview.sh quick '' scripts/preview/card-check.js
// Every step should say ok. Marking seen is checked where the session shown waits on you.
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
  var quick = window.__CODEALONG_QUICK__ === true;
  var calls = function (cmd) { return window.__calls.filter(function (c) { return c[0] === cmd; }); };
  var last = function (cmd) { var c = calls(cmd); return c.length ? c[c.length - 1][1] : null; };
  var head = function () { return document.querySelector('.ca-reply .ca-sessinsp-head'); };
  var button = function (label) { return document.querySelector('.ca-reply .ca-sessinsp-head button[aria-label="' + label + '"]'); };
  var key = function (init) { window.dispatchEvent(new KeyboardEvent('keydown', Object.assign({ bubbles: true, cancelable: true }, init))); };
  var chord = function (letter) { key({ key: letter, code: 'Key' + letter.toUpperCase(), metaKey: true, ctrlKey: true }); };
  var menu = function () { var m = calls('plugin:menu|new'); return m.length ? m[m.length - 1][1].options.items : []; };
  var said = function (items) { return items.filter(function (i) { return i.text; }).map(function (i) { return i.text + (i.accelerator ? '=' + i.accelerator : ''); }); };
  var has = function (list, text) { return list.some(function (t) { return t === text || t.indexOf(text + '=') === 0; }); };
  // Mark as seen is there for a session with a turn to see, which the header's check says.
  var whole = function (list, seenable) {
    return has(list, 'Show in Codealong') && has(list, 'Pin to desktop') && has(list, 'Snooze') && list.some(function (t) { return /^(Archive|Archive in Codex)=Ctrl\+Cmd\+A$/.test(t); }) && list.indexOf('Pin session=Ctrl+Cmd+P') >= 0 && (list.indexOf('Mark as seen=Cmd+E') >= 0) === seenable;
  };
  var seenable = false;

  say('card check: waiting for the session');
  (quick
    ? until(function () { return calls('quick_ready').length > 0 && document.querySelectorAll('.ca-quick-row').length > 0; }, 20000).then(function () {
        document.querySelector('.ca-quick-input textarea').dispatchEvent(new KeyboardEvent('keydown', { key: 'Tab', bubbles: true, cancelable: true }));
      })
    : Promise.resolve()
  )
    .then(function () { return until(function () { return !!head(); }, 20000); })
    .then(function () { return wait(400); })
    .then(function () {
      seenable = !!button('Mark as seen');
      check('the header keeps to opening it, the check and ⋯', !button('Snooze') && !button('Snoozed') && !button('Archive') && !button('Show in Codealong') && !!button('More'));
      button('More').click();
      return wait(200);
    })
    .then(function () {
      var first = said(menu());
      check('⋯ has the session’s menu, with its shortcuts', whole(first, seenable), first.join(', '));
      var n = calls('plugin:menu|new').length;
      document.querySelector('.ca-reply .ca-sessinsp-meta').dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true }));
      return wait(200).then(function () {
        var second = said(menu());
        check('a right-click has the same menu', calls('plugin:menu|new').length === n + 1 && second.join('|') === first.join('|'), second.join(', '));
      });
    })
    .then(function () {
      chord('p');
      return wait(100);
    })
    .then(function () {
      var p = last('set_pinned');
      check('⌃⌘P pins it in Sessions', !!p && p.keys.length > 0, JSON.stringify(p && p.keys.slice(0, 2)));
      key({ key: 'a', code: 'KeyA', metaKey: true });
      key({ key: 'a', code: 'KeyA' });
      check('⌘A and a plain letter archive nothing', calls('archive_session').length === 0);
      chord('r');
      return wait(200);
    })
    .then(function () {
      var name = document.querySelector('.ca-reply .ca-sessinsp-name');
      check('⌃⌘R shows its name ready to change', !!name);
      chord('a');
      check('and the shortcuts wait while it is typed', calls('archive_session').length === 0);
      if (name) name.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true }));
      return wait(300);
    })
    .then(function () {
      // Leaving the name puts the peek away and takes the quick panel back to its list; the
      // preview's peek stays, and a kept session never leaves.
      if (quick) return;
      key({ key: 'e', code: 'KeyE', metaKey: true });
      return wait(200).then(function () {
        check('⌘E marks it seen, when it has a turn to see', calls('mark_seen').length === (seenable ? 1 : 0), String(calls('mark_seen').length));
        chord('a');
        return wait(200);
      }).then(function () {
        check('⌃⌘A archives it', calls('archive_session').length === 1, String(calls('archive_session').length));
      });
    })
    .then(done)
    .catch(function (e) {
      out.push('FAIL ' + e);
      done();
    });
})();
