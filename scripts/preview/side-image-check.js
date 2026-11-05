// Checks that an image pasted with a side conversation's question shows as the image, as one in a
// prompt does in the thread (SideStrip.tsx `Turns`): the box writes it into the question as
// `![image](images/…)`, which showed as those words. The question's words keep their lines, the
// image loads, and a click on it opens it. In the bar's peek, above the box, and in a window of
// its own. The image is a made-up one, which Codealong's images don't have unless PREVIEW_IMAGES
// names a folder that does: without it the question shows that it isn't on this Mac.
//   scripts/preview.sh peek '' scripts/preview/side-image-check.js
//   scripts/preview.sh side '' scripts/preview/side-image-check.js
(function () {
  var xhr = new XMLHttpRequest();
  xhr.open('GET', './board.json', false);
  xhr.send();
  var board = JSON.parse(xhr.responseText);
  var picked = null;
  board.lanes.forEach(function (l) { l.sessions.forEach(function (s) { if (!picked && !s.archived && !l.hidden && s.harness === 'claude') picked = s; }); });
  if (!picked) { stopped('no Claude Code session to show'); return; }
  var key = picked.key;
  localStorage.setItem('codealong.preview.peek', key);
  localStorage.removeItem('codealong.sideview.' + key);
  // An open side conversation, answered, whose question came with a picture.
  var name = '0123456789abcdef.png';
  var now = Date.now();
  window.__sides = {};
  window.__sides[key] = {
    id: 'side-1',
    key: key,
    turns: [{ question: 'this one?\n\n![image](images/' + name + ')', answer: 'Yes, that one.', done: true, at: now - 60 * 1000 }],
    running: false,
    ended: false,
    account: picked.account,
    startedAt: now - 2 * 60 * 1000,
  };

  (async function () {
    try {
      await until(function () { return q('.ca-sidestrip-q'); }, 8000, 'the side conversation’s question');
      var question = q('.ca-sidestrip-q');
      var drawn = function () { return q('.ca-desc-img img, .ca-desc-noimg', question); };
      check('the question shows its image', !!drawn(), question.textContent);
      check('the question’s words show', question.textContent.indexOf('this one?') >= 0, question.textContent);
      // Loaded, or once it fails, the note that it isn't on this Mac.
      await until(function () { var i = q('img', question); return q('.ca-desc-noimg', question) || (i && i.complete && i.naturalWidth > 0); }, 4000, 'the image to load or fail');
      check('the question doesn’t show the image as words', question.textContent.indexOf('![image]') < 0 && question.textContent.indexOf('images/') < 0, question.textContent);
      var img = q('img', question);
      if (!img) {
        note('the image isn’t in the preview’s images, so it says so: ' + (drawn() || {}).textContent);
      } else {
        check('the image is no taller than one in a prompt', img.getBoundingClientRect().height <= 200, String(img.getBoundingClientRect().height));
        check('the image sits under the words', img.getBoundingClientRect().top > q('p', question).getBoundingClientRect().bottom - 1, '');
        img.closest('button').click();
        await wait(50);
        check('a click on the image opens it', sent('open_attachment').some(function (a) { return a.path === 'images/' + name; }), JSON.stringify(sent('open_attachment')));
      }
      check('the answer shows', (q('.ca-sidestrip-a') || {}).textContent === 'Yes, that one.', (q('.ca-sidestrip-a') || {}).textContent);
    } catch (e) {
      stopped(e);
      return;
    }
    done();
  })();
})();
