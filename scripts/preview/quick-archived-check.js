// Checks that a search in the quick panel leaves archived sessions out until you ask for them
// (Quick.tsx): a row under the sessions it found says how many archived ones match, and picking
// it lists them under the others, each with an Archived chip, and keeps that for later searches;
// picking it again leaves them out again.
//   PREVIEW_WAIT=60000 scripts/preview.sh quick '' scripts/preview/quick-archived-check.js
(function () {
  // This runs before the app's script: two of the board's sessions get titles nothing else has,
  // one of them archived, so a search finds them alone by title.
  localStorage.setItem('codealong.quick.project', 'All projects');
  localStorage.removeItem('codealong.quick.archived');
  var WORD = 'pangolin';
  var LIVE = 'Feed the ' + WORD;
  var GONE = 'Groom the ' + WORD;
  var named = [];
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    var p = real(cmd, args, options);
    if (cmd !== 'get_board') return p;
    return p.then(function (board) {
      var n = 0;
      board.lanes.forEach(function (l) {
        if (l.hidden || l.device) return;
        l.sessions.forEach(function (s) {
          if (s.visitor || n >= 2) return;
          s.title = n === 0 ? LIVE : GONE;
          s.archived = n === 1;
          if (named.length < 2) named.push(s.key);
          n++;
        });
      });
      return board;
    });
  };

  var input = function () { return document.querySelector('.ca-quick-input textarea'); };
  var rows = function () { return Array.prototype.slice.call(document.querySelectorAll('.ca-quick-row')); };
  var row = function (text) { return rows().filter(function (r) { return r.textContent.indexOf(text) >= 0; })[0]; };
  var toggle = function () { return rows().filter(function (r) { return /archived session/i.test(r.textContent); })[0]; };
  var stored = function () { return localStorage.getItem('codealong.quick.archived'); };
  var pick = function (r) { r.dispatchEvent(new MouseEvent('click', { bubbles: true })); };

  say('quick archived check: waiting for the list');
  until(function () { return named.length === 2 && !!input() && rows().length > 0; }, 20000, 'the list')
    .then(function () { return wait(300); })
    .then(function () {
      type(input(), WORD);
      return until(function () { return !!row(LIVE) && !!toggle(); }, 5000, 'the session found and the archived row');
    })
    .then(function () {
      check('the archived session is left out', !row(GONE), row(GONE) ? row(GONE).textContent : '');
      check('a row says one archived session matches', /Show 1 archived session/.test(toggle().textContent), toggle().textContent);
      check('it comes after the sessions found', rows().indexOf(toggle()) > rows().indexOf(row(LIVE)));
      pick(toggle());
      return until(function () { return !!row(GONE); }, 3000, 'the archived session listed');
    })
    .then(function () {
      var gone = row(GONE);
      check('picked, it lists the archived session', !!gone);
      check('under the others', rows().indexOf(gone) > rows().indexOf(row(LIVE)) && rows().indexOf(gone) < rows().indexOf(toggle()));
      check('with an Archived chip', !!gone.querySelector('.ca-chip') && /Archived/.test(gone.querySelector('.ca-chip').textContent), gone.innerHTML);
      check('the live one has none', !row(LIVE).querySelector('.ca-chip'));
      check('the row now leaves them out', /Leave out archived sessions/.test(toggle().textContent), toggle().textContent);
      check('and it’s kept for later searches', stored() === '1', String(stored()));
      // A new search lists them still.
      type(input(), '');
      type(input(), 'groom ' + WORD);
      return until(function () { return !!row(GONE) && !row(LIVE); }, 3000, 'a new search');
    })
    .then(function () {
      check('a later search lists them too', !!row(GONE));
      pick(toggle());
      return until(function () { return !row(GONE); }, 3000, 'the archived session left out');
    })
    .then(function () {
      check('picked again, it leaves them out', !row(GONE));
      check('and says how many it leaves out', /Show 1 archived session/.test(toggle().textContent), toggle().textContent);
      check('and that’s kept too', stored() === '0', String(stored()));
    })
    .catch(function (e) { check('ran', false, String(e)); })
    .then(done);
})();
