// Checks session emblems (primitives.tsx SessionEmblem, `[jev] emblems`): a session that has one
// shows it before its title on its row and in its header, its icon on a radial gradient of its
// two colors, the accent where its light falls and its main color over the rest; a session
// without one shows none. Sessions are given emblems in turn, every third without, going through
// emblems.rs's pairs and its eight lights.
//   PREVIEW_WAIT=60000 scripts/preview.sh sessions '' scripts/preview/emblems-check.js
(function () {
  var PAIRS = [['red', 'rose'], ['red', 'pink'], ['red', 'magenta'], ['red', 'violet'], ['rose', 'pink'], ['rose', 'magenta'], ['rose', 'violet'], ['rose', 'purple'], ['pink', 'magenta'], ['pink', 'violet'], ['pink', 'purple'], ['pink', 'indigo'], ['magenta', 'violet'], ['magenta', 'purple'], ['magenta', 'indigo'], ['magenta', 'blue'], ['violet', 'purple'], ['violet', 'indigo'], ['violet', 'blue'], ['purple', 'indigo'], ['purple', 'blue'], ['purple', 'sky'], ['indigo', 'blue'], ['indigo', 'sky'], ['indigo', 'teal'], ['blue', 'sky'], ['blue', 'teal'], ['blue', 'mint'], ['sky', 'teal'], ['sky', 'mint'], ['sky', 'green'], ['teal', 'mint'], ['teal', 'green'], ['teal', 'lime'], ['mint', 'green'], ['mint', 'lime'], ['green', 'lime']];
  var LIGHTS = ['28% 26%', '50% 16%', '72% 26%', '84% 50%', '72% 74%', '50% 84%', '28% 74%', '16% 50%'];
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
          if (!(s.key in byKey)) {
            var n = i++;
            var pair = PAIRS[n % PAIRS.length];
            byKey[s.key] = n % 3 === 2 ? null : { color: pair[n % 2], accent: pair[1 - (n % 2)], light: n % LIGHTS.length, icon: ICONS[n % ICONS.length] };
          }
          if (byKey[s.key]) s.emblem = byKey[s.key];
          else delete s.emblem;
        });
      });
      return b;
    });
  };

  var tones = function (el) { return { color: el.style.getPropertyValue('--emblem-color'), accent: el.style.getPropertyValue('--emblem-accent'), at: el.style.getPropertyValue('--emblem-at') }; };
  var rows = function () { return all('.ca-agentline-text'); };
  var run = async function () {
    await until(function () { return rows().length > 0; }, 20000, 'session rows');
    await wait(300);
    var shown = rows().map(function (r) { return { row: r, emblem: r.querySelector(':scope > .ca-emblem'), title: (r.querySelector('.ca-agentline-title') || {}).textContent }; });
    var withOne = shown.filter(function (x) { return x.emblem; });
    var looks = {};
    withOne.forEach(function (x) { looks[JSON.stringify(tones(x.emblem))] = true; });
    var label = /^var\(--emblem-(red|rose|pink|magenta|violet|purple|indigo|blue|sky|teal|mint|green|lime)\)$/;
    note(shown.length + ' rows, ' + withOne.length + ' with an emblem, ' + Object.keys(looks).length + ' looks among them');
    check('rows of sessions with an emblem show it, the others none', withOne.length > 0 && withOne.length < shown.length, withOne.length + ' of ' + shown.length);
    check('it comes before the title', withOne.every(function (x) { return x.emblem.nextElementSibling && x.emblem.nextElementSibling.classList.contains('ca-agentline-title'); }));
    check('it draws its icon', withOne.every(function (x) { var svg = x.emblem.querySelector('svg.ca-icon'); return svg && svg.querySelectorAll('path').length > 0; }));
    check('on a radial gradient', withOne.every(function (x) { return getComputedStyle(x.emblem).backgroundImage.indexOf('radial-gradient') === 0; }), withOne[0] && getComputedStyle(withOne[0].emblem).backgroundImage);
    check('of two emblem colors, always two', withOne.every(function (x) { var t = tones(x.emblem); return label.test(t.color) && label.test(t.accent) && t.color !== t.accent; }), withOne[0] && JSON.stringify(tones(withOne[0].emblem)));
    check('lit from where its light says', withOne.every(function (x) { return LIGHTS.indexOf(tones(x.emblem).at) >= 0; }));
    check('the gradient centred there', withOne.every(function (x) { var at = tones(x.emblem).at.split(' '); return getComputedStyle(x.emblem).backgroundImage.indexOf('at ' + at[0] + ' ' + at[1]) > 0; }), withOne[0] && getComputedStyle(withOne[0].emblem).backgroundImage.slice(0, 60));
    check('the rows look different from each other', Object.keys(looks).length >= Math.min(withOne.length, 20), Object.keys(looks).length + ' looks');
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
      check('in the same tones and light', !!big && JSON.stringify(tones(big)) === JSON.stringify(tones(row.emblem)));
    } else {
      check('the header of a session without an emblem shows none', !big, heading);
    }
    done();
  };
  setTimeout(function () { run().catch(stopped); }, 1500);
})();
