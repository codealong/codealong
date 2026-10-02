// Checks what clicks on the floating bar do (Bar.tsx): a click on a session's pill shows the
// session in Codealong's window once it's clear no second click follows, a double-click opens it
// where it lives without showing it in Codealong first, and a session picked from a folded pill's
// menu or the recent pill's shows in Codealong too. The same pills and menus serve the bar in the
// notch.
//   PREVIEW_WAIT=30000 scripts/preview.sh bar '' scripts/preview/bar-clicks-check.js
// Needs a folded pill and a recent one in the feed (a busy day has both).
(function () {
  var first = function (items) { return items.filter(function (i) { return i.text && i.handler; })[0]; };

  var tryMenu = function (name, selector) {
    var pill = document.querySelector(selector);
    if (!pill) {
      check(name + ': none in this feed', true);
      return Promise.resolve();
    }
    var opens = calls('open_from_bar').length;
    var shows = calls('show_main').length;
    pill.click();
    return wait(200).then(function () {
      var item = first(lastMenu());
      pick(item);
      return wait(100).then(function () {
        var shown = calls('show_main')[shows];
        check(name + ' shows the session in Codealong', !!shown && shown[1].reveal && shown[1].reveal.kind === 'session' && item.text.indexOf(' · ') > 0, JSON.stringify(shown && shown[1]));
        check(name + ' doesn’t open it where it lives', calls('open_from_bar').length === opens);
      });
    });
  };

  var clickPill = function () {
    var pill = document.querySelector('.ca-hud [data-key]');
    var key = pill.getAttribute('data-key');
    var opens = calls('open_from_bar').length;
    var shows = calls('show_main').length;
    pill.dispatchEvent(new MouseEvent('click', { bubbles: true, detail: 1 }));
    return wait(100)
      .then(function () {
        check('a click waits for a second', calls('show_main').length === shows);
        return wait(300);
      })
      .then(function () {
        var shown = calls('show_main')[shows];
        check('a click shows the session in Codealong', !!shown && shown[1].reveal && shown[1].reveal.key === key, JSON.stringify(shown && shown[1]));
        check('a click doesn’t open it where it lives', calls('open_from_bar').length === opens);
        shows = calls('show_main').length;
        pill.dispatchEvent(new MouseEvent('click', { bubbles: true, detail: 1 }));
        return wait(60);
      })
      .then(function () {
        pill.dispatchEvent(new MouseEvent('click', { bubbles: true, detail: 2 }));
        return wait(400);
      })
      .then(function () {
        check('a double-click opens it where it lives', calls('open_from_bar').length === opens + 1);
        check('a double-click doesn’t show it in Codealong', calls('show_main').length === shows);
      });
  };

  say('bar clicks check: waiting for pills');
  until(function () { return document.querySelector('.ca-hud [data-key]'); }, 20000)
    .then(function () { return wait(300); })
    .then(clickPill)
    .then(function () { return tryMenu('a folded pill’s menu', '.ca-hud .ca-hud-fold'); })
    .then(function () { return tryMenu('the recent pill’s menu', '.ca-hud .is-recent'); })
    .then(done)
    .catch(stopped);
})();
