// Opens Rewind to these questions' dialog on stand-in answers in the first Claude Code session in
// Sessions, for a screenshot of it (rewind-question-check.js checks what it does).
//   PREVIEW_INIT=scripts/preview/rewind-question-init.js PREVIEW_NAME=rewind-question scripts/preview.sh sessions
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
  var said = function (kind, text, cursor, more) { return Object.assign({ at: at - (500 - cursor) * 1000, kind: kind, text: text, failed: false, cursor: cursor, n: 0 }, more || {}); };
  var room = { question: 'Which room should I book for the kickoff?', header: 'Room', options: [{ label: 'Small room', description: 'Seats four, by the kitchen' }, { label: 'Large room', description: 'Seats twelve, with the projector' }] };
  var days = { question: 'Which days work for the team?', header: 'Days', multiSelect: true, options: [{ label: 'Monday' }, { label: 'Tuesday' }] };
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args) {
    if (cmd === 'session_page' && args && args.key === picked.key) {
      return Promise.resolve({ before: null, after: 500, items: [
        said('you', 'Book a room for the kickoff', 100),
        said('tool', 'AskUserQuestion', 200, { id: 't1', questions: [room, days] }),
        said('result', 'The user answered.', 300, { id: 't1', answers: { 'Which room should I book for the kickoff?': ['Large room'], 'Which days work for the team?': ['Monday', 'mornings only'] } }),
        said('agent', 'Booked the large room for Monday mornings.', 400),
      ] });
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
    var answers = Array.prototype.filter.call(document.querySelectorAll('.ca-thread-said.is-you'), function (s) { return /Room/.test(s.textContent) && /Days/.test(s.textContent); })[0];
    if (step === 1 && answers) {
      step = 2;
      answers.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true }));
      var item = window.__menu().filter(function (x) { return x[0] === 'Rewind to these questions…'; })[0];
      if (item) item[1]();
      clearInterval(t);
    }
  }, 100);
})();
