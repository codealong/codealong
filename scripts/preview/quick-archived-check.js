// Checks that a search in the quick panel leaves archived sessions out until you ask for them
// (Quick.tsx): a box on the Sessions heading, there whenever there's a search, says how many
// archived sessions match; checked, by a click or ⇧⌘A, it lists them after the others, each with
// an Archived chip, for later searches too, and the keyboard stays in the box; unchecked, it
// leaves them out again. With only archived sessions matching, the heading is there all the same,
// and the note says ⇧⌘A lists them.
//   PREVIEW_WAIT=60000 scripts/preview.sh quick '' scripts/preview/quick-archived-check.js
(function () {
  // This runs before the app's script: two of the board's sessions get titles nothing else has,
  // one of them archived, so a search finds them alone by title. The board has no todos or PRs
  // and the text search finds nothing, so what a search lists is those two.
  localStorage.setItem('codealong.quick.project', 'All projects');
  localStorage.removeItem('codealong.quick.archived');
  var WORD = 'pangolin';
  var LIVE = 'Feed the ' + WORD;
  // A word only the archived one has.
  var SHORT = 'okapi';
  var GONE = 'Groom the ' + WORD + ' ' + SHORT;
  var named = [];
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    if (cmd === 'search_text') return Promise.resolve({ hits: [], building: false, off: false });
    var p = real(cmd, args, options);
    if (cmd !== 'get_board') return p;
    return p.then(function (board) {
      board.todos = [];
      board.prs = [];
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
  var box = function () { return document.querySelector('.ca-quick-glabel .ca-quick-archived'); };
  var checked = function () { return !!box() && box().querySelector('input').checked; };
  var stored = function () { return localStorage.getItem('codealong.quick.archived'); };
  var note = function () { var n = document.querySelector('.ca-quick-note'); return n ? n.textContent : ''; };
  var search = function (words) {
    type(input(), '');
    type(input(), words);
  };
  var chord = function () { key(input(), { key: 'A', code: 'KeyA', metaKey: true, shiftKey: true }); };

  say('quick archived check: waiting for the list');
  until(function () { return named.length === 2 && !!input() && rows().length > 0; }, 20000, 'the list')
    .then(function () { return wait(300); })
    .then(function () {
      check('with nothing typed there’s no box', !box());
      search(WORD);
      return until(function () { return !!row(LIVE) && !!box(); }, 5000, 'the session found and the box');
    })
    .then(function () {
      check('the archived session is left out', !row(GONE), row(GONE) ? row(GONE).textContent : '');
      check('the box is unchecked and says one archived session matches', !checked() && /Archived\s*1/.test(box().textContent), box().textContent);
      check('it’s on the Sessions heading', /^Sessions/i.test(box().parentElement.textContent));
      box().dispatchEvent(new MouseEvent('mousedown', { bubbles: true, cancelable: true }));
      box().click();
      return until(function () { return !!row(GONE); }, 3000, 'the archived session listed');
    })
    .then(function () {
      var gone = row(GONE);
      check('checked, it lists the archived session', !!gone && checked());
      check('under the others', rows().indexOf(gone) > rows().indexOf(row(LIVE)));
      check('with an Archived chip', !!gone.querySelector('.ca-chip') && /Archived/.test(gone.querySelector('.ca-chip').textContent), gone.innerHTML);
      check('the live one has none', !row(LIVE).querySelector('.ca-chip'));
      check('it’s kept for later searches', stored() === '1', String(stored()));
      return wait(100);
    })
    .then(function () {
      check('the keyboard stays in the search', document.activeElement === input(), document.activeElement && document.activeElement.className);
      search('groom ' + WORD);
      return until(function () { return !!row(GONE) && !row(LIVE); }, 3000, 'a new search');
    })
    .then(function () {
      check('a later search lists them too', !!row(GONE) && checked());
      chord();
      return until(function () { return !row(GONE); }, 3000, 'the archived session left out');
    })
    .then(function () {
      check('⇧⌘A unchecks it and leaves them out', !row(GONE) && !checked());
      check('and that’s kept too', stored() === '0', String(stored()));
      // Only the archived session matches: the heading stays, with its box, and the note says so.
      search(SHORT);
      return until(function () { return /archived session does/.test(note()); }, 3000, 'only the archived one matching');
    })
    .then(function () {
      check('with only archived sessions matching, the heading and its box are there', !!box() && /Archived\s*1/.test(box().textContent));
      check('and the note says ⇧⌘A lists it', /1 archived session does: ⇧⌘A lists it/.test(note()), note());
      chord();
      return until(function () { return !!row(GONE); }, 3000, '⇧⌘A again');
    })
    .then(function () {
      check('⇧⌘A checks it again', checked() && !!row(GONE));
    })
    .catch(function (e) { check('ran', false, String(e) + ' rows: ' + rows().map(function (r) { return r.textContent.slice(0, 60); }).join(' / ') + ' note: ' + note()); })
    .then(done);
})();
