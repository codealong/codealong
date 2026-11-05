// Checks that an image pasted with a side conversation's question shows as the image, as one in a
// prompt does in the thread (SideStrip.tsx `Turns`): the box writes it into the question as
// `![image](images/…)`, which showed as those words. The question's words keep their lines, the
// image loads, and a click on it opens it. In the bar's peek, above the box, and in a window of
// its own. The image is one the board names (a todo's, say), which the preview's images have,
// else a made-up one, which they don't unless PREVIEW_IMAGES names a folder that does: the
// question then shows that it isn't on this Mac, which leaves its line below the strip's end, as
// the conversation does with one (SessionThread.tsx `onImage` hears only a load).
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
  var named = xhr.responseText.match(/images\/([0-9a-f]{16}\.(?:png|jpg|gif|webp))/);
  var name = named ? named[1] : '0123456789abcdef.png';
  var now = Date.now();
  window.__sides = {};
  window.__sides[key] = {
    id: 'side-1',
    key: key,
    // Questions before it fill the strip, so the image's room once it loads would leave the
    // answer under the strip's end.
    turns: [1, 2, 3, 4].map(function (n) {
      return { question: 'question ' + n, answer: 'An answer that takes a few lines, so the strip has to scroll to show its end. '.repeat(4), done: true, at: now - (10 - n) * 60 * 1000 };
    }).concat([{ question: 'this one?\n\n![image](images/' + name + ')', answer: 'Yes, that one.', done: true, at: now - 60 * 1000 }]),
    running: false,
    ended: false,
    account: picked.account,
    startedAt: now - 2 * 60 * 1000,
  };

  (async function () {
    try {
      await until(function () { return all('.ca-sidestrip-q').length === 5; }, 8000, 'the side conversation’s questions');
      var question = all('.ca-sidestrip-q')[4];
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
        check('the image is no taller than one in a prompt', img.offsetHeight <= 200, String(img.offsetHeight));
        check('the image sits under the words', img.getBoundingClientRect().top > q('p', question).getBoundingClientRect().bottom - 1, '');
        img.closest('button').click();
        await wait(50);
        check('a click on the image opens it', sent('open_attachment').some(function (a) { return a.path === 'images/' + name; }), JSON.stringify(sent('open_attachment')));
      }
      var answers = all('.ca-sidestrip-a');
      check('the answer shows', answers.length === 5 && answers[4].textContent === 'Yes, that one.', answers.length ? answers[answers.length - 1].textContent : 'none');
      var turns = q('.ca-sidestrip-turns');
      check('the strip scrolls', turns.scrollHeight > turns.clientHeight + 50, turns.scrollHeight + ' in ' + turns.clientHeight);
      if (img) check('the end is still in view once the image has its room', turns.scrollHeight - turns.scrollTop - turns.clientHeight < 2, (turns.scrollHeight - turns.scrollTop - turns.clientHeight) + ' px below');
    } catch (e) {
      stopped(e);
      return;
    }
    done();
  })();
})();
