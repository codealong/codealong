// Opens Edit from here's dialog on a stand-in prompt of the first Claude Code session in Sessions,
// for a screenshot of it (edit-from-check.js checks what it does).
//   PREVIEW_INIT=scripts/preview/edit-from-init.js PREVIEW_NAME=edit-from scripts/preview.sh sessions
(function () {
  localStorage.setItem('codealong.project', 'All projects');
  var xhr = new XMLHttpRequest();
  xhr.open('GET', './board.json', false);
  xhr.send();
  var board = JSON.parse(xhr.responseText);
  var picked = null;
  board.lanes.forEach(function (l) { l.sessions.forEach(function (s) { if (!picked && !s.archived && !l.hidden && s.harness === 'claude') picked = s; }); });
  if (!picked) return;
  var at = picked.lastActivity || Date.now();
  var said = function (kind, text, cursor) { return { at: at - (500 - cursor) * 1000, kind: kind, text: text, failed: false, cursor: cursor, n: 0 }; };
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args) {
    if (cmd === 'session_page' && args && args.key === picked.key) {
      return Promise.resolve({ before: null, after: 500, items: [said('you', 'Fix the parser', 100), said('agent', 'Fixed: it reads nested lists now.', 200), said('you', 'Now test it, with the old fixtures too.\n\nAnd say which ones you changed.', 300), said('agent', 'All 40 pass.', 400)] });
    }
    return real.apply(this, arguments);
  };
  var step = 0;
  var t = setInterval(function () {
    var row = document.querySelector('[id$="::' + picked.key + '"]');
    if (step === 0 && row) {
      step = 1;
      row.dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
      return;
    }
    var prompts = document.querySelectorAll('.ca-thread-said.is-you');
    if (step === 1 && prompts.length === 2) {
      step = 2;
      prompts[1].dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true }));
      var item = window.__menu().filter(function (x) { return x[0] === 'Edit from here…'; })[0];
      if (item) item[1]();
      clearInterval(t);
    }
  }, 100);
})();
