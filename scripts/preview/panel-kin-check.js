// Checks where a session came from and what came of it in its panel's header (SessionPanel.tsx):
// the one it came from is a line over the title, the ones that came of it are rows under it, each
// with its status; a right-click on one gives that session's menu, and a click shows its panel.
// A session that started many lists three and "and N more", which lists the rest.
//   PREVIEW_WAIT=90000 scripts/preview.sh sessions <project> scripts/preview/panel-kin-check.js
// The bar's peek has the same header (SessionHeader.tsx); there a click on one opens it in the peek:
//   PREVIEW_TRANSCRIPT=<key of a session with kin> PREVIEW_WAIT=60000 scripts/preview.sh peek '' scripts/preview/panel-kin-check.js
// Every step should say ok; "skipped" where the board has no such session.
(function () {
  var out = [];
  var say = function (t) { window.__t = t; };
  setInterval(function () { if (window.__t) document.title = window.__t; }, 100);
  var wait = function (ms) { return new Promise(function (r) { setTimeout(r, ms); }); };
  var check = function (name, ok, why) { out.push(name + (ok ? ' ok' : ' FAIL') + (why ? ' (' + why + ')' : '')); };
  var xhr = new XMLHttpRequest();
  xhr.open('GET', './board.json', false);
  xhr.send();
  var board = JSON.parse(xhr.responseText);
  var all = [];
  board.lanes.forEach(function (l) { if (!l.hidden) l.sessions.forEach(function (s) { if (!s.visitor && !s.archived) all.push(s); }); });
  var kids = function (s) { return all.filter(function (x) { return x.origin && x.origin.key === s.key; }); };
  var has = function (key) { return all.some(function (x) { return x.key === key; }); };
  var select = async function (title) {
    for (var i = 0; i < 80; i++) {
      var w = document.createTreeWalker(document.querySelector('.ca-win-body') || document.body, NodeFilter.SHOW_TEXT);
      for (var n = w.nextNode(); n; n = w.nextNode()) {
        if (n.nodeValue.trim() === title && !n.parentElement.closest('.ca-sessinsp')) {
          n.parentElement.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, button: 0 }));
          await wait(500);
          return true;
        }
      }
      await wait(100);
    }
    return false;
  };
  var head = function () { return (document.querySelector('.ca-sessinsp-title') || {}).textContent; };
  var peek = async function () {
    for (var i = 0; i < 100 && !document.querySelector('.ca-sessinsp-kin'); i++) await wait(100);
    var rows = document.querySelectorAll('.ca-reply .ca-sessinsp-kids .ca-sessinsp-kin');
    var parent = document.querySelector('.ca-reply .ca-sessinsp-kin.is-parent');
    check('the peek has the panel’s header', !!document.querySelector('.ca-reply .ca-sessinsp-head .ca-sessinsp-title') && !!document.querySelector('.ca-reply .ca-sessinsp-meta'));
    check('with the sessions around it', rows.length > 0 || !!parent, (parent ? 'from: ' + parent.textContent + ' ' : '') + rows.length + ' rows');
    var row = rows[0] || parent;
    if (!row) return say('done: ' + out.join('; '));
    row.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true, clientX: 30, clientY: 30 }));
    await wait(400);
    var items = (window.__menu() || []).map(function (x) { return x[0]; });
    check('a right-click gives its menu', items.indexOf('Pin to desktop') >= 0, items.length + ' items');
    var title = row.querySelector('.ca-sessinsp-kin-title').textContent;
    var want = all.find(function (x) { return x.title === title; });
    row.click();
    await wait(400);
    var opened = window.__calls.filter(function (c) { return c[0] === 'peek_open'; });
    check('a click opens it in the peek', opened.length === 1 && !!want && opened[0][1].key === want.key, JSON.stringify(opened.map(function (c) { return c[1].key; })));
    say('done: ' + out.join('; '));
  };
  var run = async function () {
    if (window.__CODEALONG_PEEK__) return peek();
    // One that came of another and had one come of it.
    var mid = all.find(function (s) { return s.origin && has(s.origin.key) && kids(s).length > 0; });
    if (!mid || !(await select(mid.title))) out.push('a session between two skipped');
    else {
      var parent = document.querySelector('.ca-sessinsp-kin.is-parent');
      var rows = document.querySelectorAll('.ca-sessinsp-kids .ca-sessinsp-kin');
      check('the one it came from is over the title', !!parent && parent.compareDocumentPosition(document.querySelector('.ca-sessinsp-top')) & Node.DOCUMENT_POSITION_FOLLOWING, parent && parent.textContent);
      check('the ones that came of it are under it, each with its status', rows.length === Math.min(3, kids(mid).length) && Array.prototype.every.call(rows, function (r) { return r.querySelector('.ca-glyph'); }), rows.length + ' rows');
      check('the chips are gone', !document.querySelector('.ca-sessinsp-links .ca-sessinsp-kin'));
      var row = rows[0];
      row.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true, clientX: 30, clientY: 30 }));
      await wait(400);
      var items = (window.__menu() || []).map(function (x) { return x[0]; });
      check('a right-click gives its menu', items.indexOf('Pin to desktop') >= 0 && items.some(function (x) { return /^Rename/.test(x); }), items.length + ' items');
      var title = row.querySelector('.ca-sessinsp-kin-title').textContent;
      row.click();
      await wait(600);
      check('a click shows it', head() === title, head());
      var back = document.querySelector('.ca-sessinsp-kin.is-parent');
      check('and it names the one it came of', !!back && back.textContent.indexOf(mid.title.slice(0, 20)) >= 0, back && back.textContent);
    }
    // One that started many.
    var many = all.find(function (s) { return kids(s).length > 3; });
    if (!many || !(await select(many.title))) out.push('a session with many skipped');
    else {
      var shown = document.querySelectorAll('.ca-sessinsp-kids .ca-sessinsp-kin').length;
      var more = document.querySelector('.ca-sessinsp-kids-more');
      check('many are cut to three and the rest counted', shown === 3 && !!more && more.textContent.trim() === 'and ' + (kids(many).length - 3) + ' more', shown + ' / ' + (more && more.textContent));
      if (more) more.click();
      await wait(300);
      check('which lists them all', document.querySelectorAll('.ca-sessinsp-kids .ca-sessinsp-kin').length === kids(many).length);
    }
    say('done: ' + out.join('; '));
  };
  setTimeout(run, 1500);
})();
