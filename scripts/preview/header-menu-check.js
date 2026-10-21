// Checks a right-click on a session's header in the main window's panel (SessionHeader.tsx): it
// gives the session's menu at the pointer, the same one ⋯ drops from its button; a PR and a
// session the header names keep their own menus, one menu a click; and the conversation under
// the header gives none.
//   PREVIEW_WAIT=60000 scripts/preview.sh sessions '' scripts/preview/header-menu-check.js
// A step is noted as skipped where the session shown names no PR or session. The cards
// (the peek, a kept session, the quick panel) have the same header: card-check.js.
(function () {
  var labels = function () { return window.__menu().map(function (m) { return m[0]; }).join('|'); };
  // The menu is long (every todo is in it), so a step says how many items, not which.
  var count = function () { return window.__menu().length + ' items'; };
  var right = function (el) { rightClick(el, { clientX: 40, clientY: 40 }); };
  var head = function () { return document.querySelector('.ca-sessinsp .ca-sessinsp-head'); };
  var rows = function () { return document.querySelectorAll('.ca-lane.is-agent'); };
  var session = '';

  say('header menu check: waiting for a session');
  until(function () { return rows().length > 0; }, 20000)
    .then(function () {
      rows()[0].dispatchEvent(new MouseEvent('mousedown', { bubbles: true, cancelable: true }));
      return until(function () { return !!head(); }, 5000);
    })
    .then(function () { return wait(400); })
    .then(function () {
      var n = menus();
      right(head().querySelector('.ca-sessinsp-title'));
      return wait(300).then(function () {
        session = labels();
        check('a right-click on the title gives the session’s menu', menus() === n + 1 && /Rename…/.test(session) && /Pin to desktop/.test(session), count());
        var at = calls('popup_menu').pop();
        check('at the pointer', !!at && !at[1].at, JSON.stringify(at && at[1]));
      });
    })
    .then(function () {
      var n = menus();
      right(head().querySelector('.ca-sessinsp-meta'));
      return wait(300).then(function () {
        check('and so does one on the line under it', menus() === n + 1 && labels() === session, count());
      });
    })
    .then(function () {
      var n = menus();
      head().querySelector('button[aria-label="More"]').click();
      return wait(300).then(function () {
        var at = calls('popup_menu').pop();
        check('⋯ drops the same menu from its button', menus() === n + 1 && labels() === session && !!at && !!at[1].at, count());
      });
    })
    .then(function () {
      var pr = head().querySelector('.ca-sessinsp-links .ca-pr');
      if (!pr) return void note('a PR’s own menu skipped');
      var n = menus();
      right(pr);
      return wait(300).then(function () {
        check('a PR keeps its own menu, and only that', menus() === n + 1 && labels() !== session, count());
      });
    })
    .then(function () {
      var kin = head().querySelector('.ca-sessinsp-kin');
      if (!kin) return void note('a named session’s own menu skipped');
      var n = menus();
      right(kin);
      return wait(300).then(function () {
        check('a session it names gives one menu, its own', menus() === n + 1, String(menus() - n));
      });
    })
    .then(function () {
      var thread = document.querySelector('.ca-sessinsp .ca-thread') || document.querySelector('.ca-sessinsp .ca-composer');
      if (!thread) return void note('the conversation skipped');
      var n = menus();
      right(thread);
      return wait(300).then(function () {
        check('the conversation under it gives none', menus() === n, String(menus() - n));
      });
    })
    .then(done)
    .catch(stopped);
})();
