// A side conversation open above a Claude Code session's box in the bar's peek, with `/fo` typed
// in its box, for a screenshot of /fork listed there with what it does (side-fork-check.js checks
// what it does).
//   PREVIEW_INIT=scripts/preview/side-fork-init.js PREVIEW_NAME=side-fork scripts/preview.sh peek
(function () {
  var xhr = new XMLHttpRequest();
  xhr.open('GET', './board.json', false);
  xhr.send();
  var board = JSON.parse(xhr.responseText);
  var picked = null;
  board.lanes.forEach(function (l) { l.sessions.forEach(function (s) { if (!picked && !s.archived && !l.hidden && s.harness === 'claude') picked = s; }); });
  if (!picked) return;
  var key = picked.key;
  localStorage.setItem('codealong.preview.peek', key);
  localStorage.removeItem('codealong.draft.side:' + key);
  localStorage.removeItem('codealong.sideview.' + key);
  window.__sides = {};
  window.__sides[key] = { id: 'side-1', key: key, running: false, ended: false, turns: [{ question: 'What’s left before the release?', answer: 'Three things:\n\n1. Merge **#292**, then **#294**.\n2. Add the drift section to the checklist.\n3. Update the release notes.', done: true }] };
  var tries = 0;
  var go = function () {
    var box = document.querySelector('.ca-composer textarea');
    if (!box || !document.querySelector('.ca-sidestrip')) {
      if (++tries < 300) setTimeout(go, 100);
      return;
    }
    box.focus();
    type(box, '/fo');
  };
  go();
})();
