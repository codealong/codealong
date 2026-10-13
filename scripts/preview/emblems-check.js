// Checks session emblems (primitives.tsx SessionEmblem, `[jev] emblems`): a session that has one
// shows it before its title on its row and in its header, its icon on a radial gradient lit from
// its top left corner, its harness's color there (Claude Code's `/color`) where it has one and
// Codealong's over the rest, Codealong's alone without one; a session without one shows none. Sessions are
// given emblems in turn, every third without, every other one with a harness's color too.
//   PREVIEW_WAIT=60000 scripts/preview.sh sessions '' scripts/preview/emblems-check.js
(function () {
  var COLORS = ['blue', 'teal', 'green', 'purple', 'pink', 'red', 'indigo'];
  var ICONS = ['bug', 'palette', 'database', 'rocket', 'book', 'compass'];
  var byKey = {};
  var invoke = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    var p = invoke(cmd, args, options);
    if (cmd !== 'get_board') return p;
    return p.then(function (b) {
      var i = 0;
      b.lanes.forEach(function (l) {
        l.sessions.forEach(function (s) {
          if (!byKey[s.key]) {
            var n = i++;
            byKey[s.key] = n % 3 === 2 ? null : Object.assign({ color: COLORS[n % COLORS.length], icon: ICONS[n % ICONS.length] }, n % 2 === 0 ? { harnessColor: COLORS[(n + 3) % COLORS.length] } : {});
          }
          if (byKey[s.key]) s.emblem = byKey[s.key];
          else delete s.emblem;
        });
      });
      return b;
    });
  };

  var tones = function (el) { return { mid: el.style.getPropertyValue('--emblem-mid'), edge: el.style.getPropertyValue('--emblem-edge') }; };
  var rows = function () { return all('.ca-agentline-text'); };
  var run = async function () {
    await until(function () { return rows().length > 0; }, 20000, 'session rows');
    await wait(300);
    var shown = rows().map(function (r) { return { row: r, emblem: r.querySelector(':scope > .ca-emblem'), title: (r.querySelector('.ca-agentline-title') || {}).textContent }; });
    var withOne = shown.filter(function (x) { return x.emblem; });
    var twoTone = withOne.filter(function (x) { var t = tones(x.emblem); return t.mid !== t.edge; });
    note(shown.length + ' rows, ' + withOne.length + ' with an emblem, ' + twoTone.length + ' of them in two tones');
    check('rows of sessions with an emblem show it, the others none', withOne.length > 0 && withOne.length < shown.length, withOne.length + ' of ' + shown.length);
    check('it comes before the title', withOne.every(function (x) { return x.emblem.nextElementSibling && x.emblem.nextElementSibling.classList.contains('ca-agentline-title'); }));
    check('it draws its icon', withOne.every(function (x) { var svg = x.emblem.querySelector('svg.ca-icon'); return svg && svg.querySelectorAll('path').length > 0; }));
    check('on a radial gradient', withOne.every(function (x) { return getComputedStyle(x.emblem).backgroundImage.indexOf('radial-gradient') === 0; }), withOne[0] && getComputedStyle(withOne[0].emblem).backgroundImage);
    check('in label colors, Codealong’s at the edge', withOne.every(function (x) { return /^var\(--label-(blue|teal|green|purple|pink|red|indigo)\)$/.test(tones(x.emblem).edge); }), withOne[0] && JSON.stringify(tones(withOne[0].emblem)));
    check('some in two tones and some in one', twoTone.length > 0 && twoTone.length < withOne.length, twoTone.length + ' of ' + withOne.length);
    check('which it says nothing aloud about', withOne.every(function (x) { return x.emblem.getAttribute('aria-hidden') === 'true'; }));
    // The header of the session shown: its emblem beside its title, larger.
    await until(function () { return !!q('.ca-sessinsp-title'); }, 10000, 'a session header');
    var heading = q('.ca-sessinsp-title').textContent;
    var top = q('.ca-sessinsp-top');
    var big = top && top.querySelector(':scope > .ca-emblem');
    var row = shown.filter(function (x) { return x.title === heading; })[0];
    if (row && row.emblem) {
      check('the header shows the session’s emblem before its title', !!big && big.nextElementSibling === q('.ca-sessinsp-title'), heading);
      check('larger than the row’s', !!big && big.getBoundingClientRect().width > row.emblem.getBoundingClientRect().width, big && big.getBoundingClientRect().width + ' over ' + row.emblem.getBoundingClientRect().width);
      check('in the same tones', !!big && JSON.stringify(tones(big)) === JSON.stringify(tones(row.emblem)));
    } else {
      check('the header of a session without an emblem shows none', !big, heading);
    }
    done();
  };
  setTimeout(function () { run().catch(stopped); }, 1500);
})();
