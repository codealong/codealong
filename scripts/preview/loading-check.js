// Checks what the windows show at launch, before the engine's first scan has finished: the main
// window says it's reading rather than listing nothing, or only other devices' worktrees, selects
// none of those, keeps its project and gives no counts, until the board comes; the floating bar
// says it's reading until its feed comes.
//   PREVIEW_LOADING=1 PREVIEW_DEVICES=scripts/preview/devices-sample.json PREVIEW_WAIT=60000 scripts/preview.sh sessions '' scripts/preview/loading-check.js
//   PREVIEW_LOADING=1 PREVIEW_WAIT=60000 scripts/preview.sh bar '' scripts/preview/loading-check.js
(function () {
  var text = function (sel) { var el = document.querySelector(sel); return el ? el.textContent : ''; };
  var json = function (file) { return fetch(file).then(function (r) { return r.json(); }); };
  // The main window should keep the project it had before launch: a run that names none gets
  // the board's first, put there before the app starts.
  if (!window.__CODEALONG_BAR__ && !localStorage.getItem('codealong.project')) {
    var xhr = new XMLHttpRequest();
    xhr.open('GET', './board.json', false);
    xhr.send();
    var first = JSON.parse(xhr.responseText).projects[0];
    if (first) localStorage.setItem('codealong.project', first.name);
  }

  if (window.__CODEALONG_BAR__) {
    say('loading check: waiting for the bar');
    until(function () { return !!document.querySelector('.ca-hud-note'); }, 20000)
      .then(function () {
        check('the bar says it is reading', text('.ca-hud-note') === 'Reading sessions…', text('.ca-hud-note'));
        return json('./bar-feed.json').then(function (feed) {
          window.__emit('bar-feed', feed);
          return until(function () { return all('.ca-hud [data-key]').length > 0; }, 10000).then(function () {
            check('and shows its pills once the feed comes', !document.querySelector('.ca-hud-note'), String(all('.ca-hud [data-key]').length));
          });
        });
      })
      .then(done, stopped);
    return;
  }

  say('loading check: waiting for the main window');
  var mac = function () { return all('.ca-side-devrow').map(function (r) { return r.textContent; }).filter(function (t) { return t.indexOf('This Mac') >= 0; })[0] || ''; };
  until(function () { return !!document.querySelector('.ca-board-loading'); }, 20000)
    .then(function () {
      // Other devices' worktrees come first; give them a moment to show if they were going to.
      return until(function () { return mac().length > 0; }, 5000).catch(function () {});
    })
    .then(function () {
      check('the list says it is reading', text('.ca-board-loading') === 'Reading sessions…', text('.ca-board-loading'));
      check('with the working glyph', !!document.querySelector('.ca-board-loading .ca-g-arc'));
      check('and lists nothing yet, other devices included', !document.querySelector('.ca-board-list') && all('[role="option"]').length === 0, String(all('[role="option"]').length));
      check('nor selects any of theirs', !!document.querySelector('.ca-insp--empty'));
      var stored = localStorage.getItem('codealong.project');
      check('the header keeps the project', !stored || text('.ca-projswitch-name') === stored, text('.ca-projswitch-name'));
      check('the sidebar gives no counts', all('.ca-side-chip-count').length === 0 && all('.ca-side .ca-nav-count').length === 0, all('.ca-side-chip-count').length + '/' + all('.ca-side .ca-nav-count').length);
      check('this Mac reads as reading', mac() === '' || mac().indexOf('reading sessions') >= 0, mac());
      check('no "No accounts yet."', document.body.textContent.indexOf('No accounts yet.') < 0);
      check('no empty-list message', !document.querySelector('.ca-board-empty:not(.ca-board-loading)'));
      return json('./board.json').then(function (board) {
        window.__emit('board', board);
        return until(function () { return !document.querySelector('.ca-board-loading') && all('[role="option"]').length > 0; }, 10000).then(function () {
          check('the board comes in once read', true, all('[role="option"]').length + ' rows');
          check('with its counts', all('.ca-side-chip-count').length > 0, String(all('.ca-side-chip-count').length));
          check('and this Mac working', mac() === '' || / working/.test(mac()), mac());
        });
      });
    })
    .then(done, stopped);
})();
