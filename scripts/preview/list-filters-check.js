// Checks the filters on as chips over the list (ListBar.tsx `FilterChips`) on a narrow window, the
// sidebar folded: a chip for the age and for each status and agent picked, each with its × and,
// with several, Clear, all of them in sight on the strip however many there are (a name gives way,
// then they wrap); a × takes its own filter off, and Clear takes them all off.
//   PREVIEW_SIZE=860,640 scripts/preview.sh sessions '' scripts/preview/list-filters-check.js
(function () {
  localStorage.setItem('codealong.age', '7d');
  var chips = function () { return all('.ca-listfilters .ca-listfilter'); };
  var label = function (words) { return all('.ca-side-label').filter(function (l) { return l.textContent.trim().indexOf(words) === 0; })[0]; };
  var picks = function (words, n) {
    var l = label(words);
    var row = l && l.nextElementSibling;
    return row ? all('.ca-side-chip', row).slice(0, n) : [];
  };
  // Whole on the screen, inside the board and the strip.
  var inSight = function (el) {
    var r = el.getBoundingClientRect();
    var board = q('.ca-board').getBoundingClientRect();
    var strip = q('.ca-listbar').getBoundingClientRect();
    return r.width > 0 && r.left >= board.left - 0.5 && r.right <= board.right + 0.5 && r.top >= strip.top - 0.5 && r.bottom <= strip.bottom + 0.5;
  };

  say('list filters check: waiting for the sidebar');
  until(function () { return label('Status') && q('.ca-listbar'); }, 20000, 'the sidebar and the strip')
    .then(function () {
      picks('Status', 2).forEach(function (c) { c.click(); });
      picks('Agents', 3).forEach(function (c) { c.click(); });
      return wait(200);
    })
    .then(function () {
      var toggle = q('.ca-sidetoggle.is-on');
      if (toggle) toggle.click();
      return until(function () { return !q('.ca-sidetoggle.is-on') && chips().length > 2; }, 3000, 'the chips, the sidebar folded');
    })
    .then(function () {
      var n = chips().length;
      check('a chip for the age and each pick', n >= 3 && /^Last 7 days/.test(chips()[0].textContent), n + ': ' + chips().map(function (c) { return c.textContent; }).join(' | '));
      var xs = chips().map(function (c) { return q('button', c); });
      check('each chip’s × is in sight', xs.every(function (x) { return x && inSight(x); }), xs.filter(function (x) { return !x || !inSight(x); }).length + ' out of sight');
      var clear = q('.ca-listfilters-clear');
      check('Clear is in sight', !!clear && inSight(clear));
      check('each × says what it takes off', xs.every(function (x) { return /^Stop filtering to ./.test(x.getAttribute('aria-label') || ''); }), xs.map(function (x) { return x.getAttribute('aria-label'); }).join(' | '));
      var last = chips()[n - 1].textContent;
      q('button', chips()[n - 1]).click();
      return until(function () { return chips().length === n - 1; }, 3000, 'one chip fewer').then(function () {
        check('a × takes its own filter off', chips().every(function (c) { return c.textContent !== last; }), last);
      });
    })
    .then(function () {
      q('.ca-listfilters-clear').click();
      return until(function () { return chips().length === 0; }, 3000, 'no chips');
    })
    .then(function () {
      check('Clear takes them all off, the age too', localStorage.getItem('codealong.age') === 'all' && !q('.ca-rail-btn[aria-label^="Filtered"]'), localStorage.getItem('codealong.age'));
    })
    .then(done)
    .catch(stopped);
})();
