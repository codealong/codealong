// Checks session emblems (primitives.tsx SessionEmblem, `[emblems]`): a session that has one
// shows it before its title on its row and in its header, larger there: its emoji as text, or the
// picture drawn for it, loaded as a kept image, with what it is as its tooltip; a session without
// one shows none, and a picture that doesn't load gives way to its emoji. Sessions are given
// emblems in turn: an emoji, a picture (scripts/preview/emblems/), none.
//   PREVIEW_EMBLEMS=drawn scripts/preview.sh sessions '' scripts/preview/emblems-check.js
(function () {
  var GIVEN = [{ emoji: '🦖', name: 't rex' }, { picture: 'spool.png', name: 'spool' }, null, { picture: 'not-drawn.png', emoji: '🧵', name: 'spool' }];
  var byKey = {};
  var invoke = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    var p = invoke(cmd, args, options);
    if (cmd !== 'get_board') return p;
    return p.then(function (b) {
      var i = 0;
      b.lanes.forEach(function (l) {
        l.sessions.forEach(function (s) {
          if (!(s.key in byKey)) byKey[s.key] = GIVEN[i++ % GIVEN.length];
          if (byKey[s.key]) s.emblem = byKey[s.key];
          else delete s.emblem;
        });
      });
      return b;
    });
  };

  var rows = function () { return all('.ca-agentline-text'); };
  var shownIn = function (r) {
    var title = r.querySelector('.ca-agentline-title');
    return { row: r, emblem: r.querySelector(':scope > .ca-emblem'), title: title ? title.textContent : '' };
  };
  var run = async function () {
    await until(function () { return rows().length > 3; }, 20000, 'session rows');
    await wait(500);
    var shown = rows().map(shownIn);
    var emoji = shown.filter(function (x) { return x.emblem && x.emblem.tagName === 'SPAN'; });
    var pictures = shown.filter(function (x) { return x.emblem && x.emblem.tagName === 'IMG'; });
    var none = shown.filter(function (x) { return !x.emblem; });
    note(shown.length + ' rows: ' + emoji.length + ' with an emoji, ' + pictures.length + ' with a picture, ' + none.length + ' with none');
    check('rows show emoji, pictures, and nothing for a session without one', emoji.length > 0 && pictures.length > 0 && none.length > 0);
    check('it comes before the title', shown.filter(function (x) { return x.emblem; }).every(function (x) { return x.emblem.nextElementSibling && x.emblem.nextElementSibling.classList.contains('ca-agentline-title'); }));
    check('an emoji is drawn as itself, named in its tooltip', emoji.every(function (x) { return /🦖|🧵/.test(x.emblem.textContent) && /^(t rex|spool)$/.test(x.emblem.title); }), emoji[0] && emoji[0].emblem.outerHTML);
    check('a picture is the kept image, named in its tooltip', pictures.every(function (x) { return /\/images\/spool\.png$/.test(x.emblem.getAttribute('src')) && x.emblem.title === 'spool'; }), pictures[0] && pictures[0].emblem.outerHTML);
    check('a picture fills the emblem’s square', pictures.every(function (x) { var r = x.emblem.getBoundingClientRect(); return r.width === 16 && r.height === 16 && x.emblem.complete && x.emblem.naturalWidth === 64; }), pictures[0] && JSON.stringify(pictures[0].emblem.getBoundingClientRect()));
    check('an emoji fills it too', emoji.every(function (x) { var r = x.emblem.getBoundingClientRect(); return r.width === 16 && r.height === 16; }));
    check('a picture that doesn’t load shows its emoji', !all('.ca-agentline-text > img.ca-emblem').some(function (i) { return /not-drawn/.test(i.getAttribute('src')); }) && emoji.some(function (x) { return x.emblem.textContent === '🧵'; }));
    check('which it says nothing aloud about', emoji.every(function (x) { return x.emblem.getAttribute('aria-hidden') === 'true'; }) && pictures.every(function (x) { return x.emblem.getAttribute('alt') === ''; }));
    // The header of the session shown: its emblem beside its title, larger.
    await until(function () { return !!q('.ca-sessinsp-title'); }, 10000, 'a session header');
    var heading = q('.ca-sessinsp-title').textContent;
    var top = q('.ca-sessinsp-top');
    var big = top && top.querySelector(':scope > .ca-emblem');
    var row = shown.filter(function (x) { return x.title === heading; })[0];
    if (row && row.emblem) {
      check('the header shows the session’s emblem before its title', !!big && big.nextElementSibling === q('.ca-sessinsp-title'), heading);
      check('larger than the row’s', !!big && big.getBoundingClientRect().width === 20 && row.emblem.getBoundingClientRect().width === 16, big && big.getBoundingClientRect().width + ' over ' + row.emblem.getBoundingClientRect().width);
      check('the same emblem', !!big && big.tagName === row.emblem.tagName && (big.getAttribute('src') || big.textContent) === (row.emblem.getAttribute('src') || row.emblem.textContent));
    } else {
      check('the header of a session without an emblem shows none', !big, heading);
    }
    done();
  };
  setTimeout(function () { run().catch(stopped); }, 1500);
})();
