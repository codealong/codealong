// Checks session emblems in the quick panel (primitives.tsx SessionEmblem, `[emblems]`): a session
// row shows its emblem first on its title's line, inside the title so the title still shortens
// and the emblem isn't cut; a session without one shows none; and Jev's suggestion to send to a
// session shows that session's emblem before what it says. Sessions are given emblems in turn, as
// emblems-check.js gives them: an emoji, a picture (scripts/preview/emblems/), none.
//   PREVIEW_EMBLEMS=drawn PREVIEW_ROUTE=scripts/preview/route-sample.json PREVIEW_WAIT=60000 scripts/preview.sh quick '' scripts/preview/quick-emblems-check.js
(function () {
  var GIVEN = [{ emoji: '🦖', name: 't rex' }, { picture: 'spool.png', name: 'spool' }, null];
  var byKey = {};
  var byTitle = {};
  // How many sessions have each title: a row is matched to its session by a title only one has.
  var titled = {};
  // The sessions with an emblem that Jev may send to (fake-tauri.js `route_quick`).
  var sendable = [];
  var LONGER = ' and a great many more words, so that it runs past the end of the row';
  var invoke = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    var p = invoke(cmd, args, options);
    // Jev sends to a session that has an emblem and a title of its own, so its suggestion shows it.
    if (cmd === 'route_quick') {
      return p.then(function (r) {
        var to = sendable.filter(function (x) { return titled[x.title] === 1; })[0];
        if (to && r.sessions && r.sessions[0]) r.sessions[0].id = to.key;
        return r;
      });
    }
    if (cmd !== 'get_board') return p;
    return p.then(function (b) {
      var i = 0;
      sendable = [];
      b.lanes.forEach(function (l) {
        l.sessions.forEach(function (s) {
          if (!(s.key in byKey)) byKey[s.key] = GIVEN[i++ % GIVEN.length];
          if (byKey[s.key]) s.emblem = byKey[s.key];
          else delete s.emblem;
          // Those with an emoji get a title too long for their rows.
          if (byKey[s.key] && byKey[s.key].emoji && s.title.indexOf(LONGER) < 0) s.title += LONGER;
          byTitle[s.title] = byKey[s.key];
          titled[s.title] = (titled[s.title] || 0) + 1;
          if (byKey[s.key] && s.live && !s.visitor && s.status !== 'needs') sendable.push({ key: s.key, title: s.title });
        });
      });
      return b;
    });
  };

  var NOT_SESSIONS = '.is-suggest, .is-skill, .is-todo, .is-pr, .is-more, .ca-quick-compose';
  var sessionRows = function () { return all('.ca-quick-row').filter(function (r) { return !r.matches(NOT_SESSIONS); }); };
  var suggestions = function () { return all('.ca-quick-row.is-suggest:not(.is-note):not(.is-stale)'); };
  var shownIn = function (r) {
    var title = q('.ca-quick-row-title', r);
    return { row: r, title: title, emblem: title && title.querySelector(':scope > .ca-emblem'), text: title ? title.textContent : '' };
  };
  var same = function (el, given) {
    if (!given) return !el;
    if (!el) return false;
    return given.picture ? el.tagName === 'IMG' && /\/images\/spool\.png$/.test(el.getAttribute('src')) : el.tagName === 'SPAN' && el.textContent === given.emoji;
  };

  localStorage.setItem('codealong.quick.project', 'All projects');
  var run = async function () {
    await until(function () { return calls('quick_ready').length > 0 && sessionRows().length > 2; }, 30000, 'session rows');
    await wait(500);
    var shown = sessionRows().map(shownIn);
    var emoji = shown.filter(function (x) { return x.emblem && x.emblem.tagName === 'SPAN'; });
    var pictures = shown.filter(function (x) { return x.emblem && x.emblem.tagName === 'IMG'; });
    var none = shown.filter(function (x) { return !x.emblem; });
    note(shown.length + ' session rows: ' + emoji.length + ' with an emoji, ' + pictures.length + ' with a picture, ' + none.length + ' with none');
    check('rows show emoji, pictures, and nothing for a session without one', emoji.length > 0 && pictures.length > 0 && none.length > 0);
    var titleOf = function (x) { return x.text.replace(/^🦖/, ''); };
    var told = shown.filter(function (x) { return titled[titleOf(x)] === 1; });
    note(told.length + ' rows with a title only one session has');
    check('each row shows its own session’s emblem', told.length > 2 && told.every(function (x) { return same(x.emblem, byTitle[titleOf(x)]); }), told.filter(function (x) { return !same(x.emblem, byTitle[titleOf(x)]); }).map(function (x) { return x.text + ' (' + (x.emblem ? x.emblem.outerHTML : 'none') + ')'; }).join(' | '));
    check('it leads the title', shown.filter(function (x) { return x.emblem; }).every(function (x) { return x.title.firstChild === x.emblem; }));
    check('named in its tooltip, saying nothing aloud', emoji.every(function (x) { return x.emblem.title === 't rex' && x.emblem.getAttribute('aria-hidden') === 'true'; }) && pictures.every(function (x) { return x.emblem.title === 'spool' && x.emblem.getAttribute('alt') === ''; }));
    check('16 points square, as Sessions’ rows have it', shown.filter(function (x) { return x.emblem; }).every(function (x) { var r = x.emblem.getBoundingClientRect(); return r.width === 16 && r.height === 16; }));
    check('a picture loads', pictures.every(function (x) { return x.emblem.complete && x.emblem.naturalWidth > 0; }));
    var gap = function (x) { var e = x.emblem.getBoundingClientRect(); var t = document.createRange(); t.selectNodeContents(x.title); var rects = Array.prototype.slice.call(t.getClientRects()).filter(function (r) { return r.left >= e.right; }); return rects.length ? Math.round(rects[0].left - e.right) : null; };
    check('with a little room before the title’s words', shown.filter(function (x) { return x.emblem; }).every(function (x) { var g = gap(x); return g != null && g >= 4 && g <= 8; }), shown.filter(function (x) { return x.emblem; }).map(gap).join(','));
    check('the row keeps its height', shown.every(function (x) { return Math.round(x.row.getBoundingClientRect().height) === Math.round(none[0].row.getBoundingClientRect().height); }), shown.map(function (x) { return Math.round(x.row.getBoundingClientRect().height); }).join(','));
    // A long title shortens behind the emblem, which stays whole at the line's start.
    var longest = shown.filter(function (x) { return x.emblem && x.title.scrollWidth > x.title.clientWidth; })[0];
    if (longest) {
      var t = longest.title.getBoundingClientRect();
      var e = longest.emblem.getBoundingClientRect();
      check('a title cut short keeps its emblem whole', e.left >= t.left && e.right <= t.right, longest.text);
    } else note('no title long enough to be cut short');

    // Jev's suggestion to send to a session names it, with its emblem.
    var box = q('.ca-quick-input textarea');
    Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value').set.call(box, 'Fix the login page so it remembers the account');
    box.dispatchEvent(new Event('input', { bubbles: true }));
    await until(function () { return suggestions().some(function (r) { return /^Send to “/.test(shownIn(r).text.replace(/^🦖/, '')); }); }, 10000, 'a send suggestion');
    var send = suggestions().map(shownIn).filter(function (x) { return /^Send to “/.test(x.text.replace(/^🦖/, '')); })[0];
    var to = send.text.replace(/^🦖/, '').replace(/^Send to “/, '').replace(/”$/, '');
    note('suggestion: ' + send.text + ' (' + (byTitle[to] ? byTitle[to].name : 'none') + ')');
    check('a suggestion to send to a session shows its emblem, as its row does', titled[to] === 1 && !!send.emblem && same(send.emblem, byTitle[to]) && send.title.firstChild === send.emblem, send.title.innerHTML);
    var start = suggestions().map(shownIn).filter(function (x) { return !/^(Send to|Open|Answer) “/.test(x.text.replace(/^🦖/, '')); });
    check('a suggestion to start shows none', start.every(function (x) { return !x.emblem; }), start.map(function (x) { return x.text; }).join(' | '));
    done();
  };
  setTimeout(function () { run().catch(stopped); }, 500);
})();
