// Checks how a reply shows the directives Codex's app draws (Markdown.tsx, directives.ts): a
// writing block is a card named for its variant, with Copy, which copies the writing without its
// fences; an email's recipient and subject show over it, and its versions are picked between; a
// code comment is a card with where it is; a cited file shows its name and folder; and a
// follow-up puts its prompt in the session's box. The conversation is a stand-in, on the first
// session:
//   scripts/preview.sh sessions '' scripts/preview/codex-directives-check.js
// As PREVIEW_INIT (with PREVIEW_NAME=codex-directives) it leaves the panel open on it, for a
// screenshot.
(function () {
  localStorage.setItem('codealong.project', 'All projects');
  var STATUS = 'W&M 3 evaluation preparation is complete. Please read this status before continuing.\n\n- `pilot/pilot-checks.json`: the frozen 31-check selection.\n- **Do not open the held-out cases.**';
  var FIRST = 'Here’s a prompt you can paste into the other chat:\n\n:::writing{variant="standard" id="83147"}\n' + STATUS + '\n:::\n\nAnd the email:\n\n:::writing{variant="email" id="20411" subject="Pilot ready" recipient="greg@example.com"}\n---tone Friendly\nHi Greg, the pilot is ready whenever you are.\n\n---tone Direct\nGreg: the pilot is ready. Start Monday.\n:::';
  var PROMPT = 'Fix the retry authorization check and add a test. Do not push.';
  var SECOND = 'One blocker remains.\n\n::code-comment{title="[P1] Retry skips the team check" body="An ordinary manager can retry a failed transfer." file="/Users/me/projects/app/src/transfer.ts" start=42 end=44 priority=1}\n\nThe updated PDF:\n\n:codex-file-citation{path="/Users/me/projects/app/.local/review-v3.pdf" purpose="output"}\n\n- :codex-followup[Fix the retry check]{prompt="' + PROMPT + '"}\n- :codex-followup[Plan the regression]{prompt="Plan a regression for transfers."}';
  var copied = [];
  try {
    Object.defineProperty(navigator, 'clipboard', { configurable: true, value: { writeText: function (t) { copied.push(t); return Promise.resolve(); } } });
  } catch (e) {
    // A check that can't see what's copied says so below.
  }
  var picked = null;
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    if (cmd === 'session_page' && picked && (args || {}).key === picked.key) {
      var at = Date.now() - 9 * 60e3;
      var item = function (kind, text, n) { return { at: at + n * 1000, kind: kind, text: text, failed: false, cursor: n, n: 0 }; };
      return Promise.resolve({ before: null, after: 3, items: [item('you', 'Write the status for the other chat, and an email to Greg', 0), item('agent', FIRST, 1), item('agent', SECOND, 2)] });
    }
    var p = real(cmd, args, options);
    if (cmd !== 'get_board') return p;
    return p.then(function (board) {
      board.lanes.forEach(function (l) {
        l.sessions.forEach(function (s) {
          if (!picked && !l.hidden && !s.visitor && !s.archived) picked = { key: s.key, lane: l.id };
        });
      });
      return board;
    });
  };

  var row = function () { return picked && document.getElementById('row-' + picked.lane + '::' + picked.key); };
  var panel = function (sel) { return Array.prototype.slice.call(document.querySelectorAll('.ca-sessinsp ' + sel)); };
  var text = function (el) { return el ? el.textContent.replace(/\s+/g, ' ').trim() : ''; };

  say('codex directives check: waiting for the list');
  until(function () { return row(); }, 20000)
    .then(function () {
      row().dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
      row().click();
      return until(function () { return panel('.ca-md-card').length === 3 && panel('.ca-md-followup').length === 2; }, 10000);
    })
    .then(function () {
      var cards = panel('.ca-md-card');
      var titles = cards.map(function (c) { return text(c.querySelector('.ca-md-card-title')); });
      check('each writing block is a card named for its variant, and the comment one too', titles.join('|') === 'Writing|Email|[P1] Retry skips the team check', titles.join('|'));
      var replies = panel('.ca-thread-agent');
      check('no fence or directive shows as text', replies.every(function (r) { return !/:::|::code-comment|:codex-/.test(r.textContent); }), replies.map(text).join(' / '));
      var status = cards[0].querySelector('.ca-md-card-body');
      check('a card’s writing is Markdown', status.querySelectorAll('li').length === 2 && !!status.querySelector('strong') && !!status.querySelector('code'), status.innerHTML.slice(0, 200));
      var fields = Array.prototype.slice.call(cards[1].querySelectorAll('.ca-md-card-fields > *')).map(text).join(' ');
      check('an email shows its recipient and subject', fields === 'To greg@example.com Subject Pilot ready', fields);
      var tones = Array.prototype.slice.call(cards[1].querySelectorAll('.ca-md-card-tones button'));
      check('and its versions to pick between, the first shown', tones.map(text).join('|') === 'Friendly|Direct' && /whenever you are/.test(text(cards[1].querySelector('.ca-md-card-body'))), tones.map(text).join('|'));
      tones[1].click();
      var where = text(cards[2].querySelector('.ca-md-card-where'));
      check('a code comment says where it is', where === '~/projects/app/src/transfer.ts:42–44', where);
      var file = panel('.ca-md-file')[0];
      check('a cited file shows its name and folder', text(file.querySelector('.ca-md-file-name')) === 'review-v3.pdf' && text(file.querySelector('.ca-md-file-dir')) === '~/projects/app/.local' && file.title === '/Users/me/projects/app/.local/review-v3.pdf', text(file));
      return until(function () { return /Start Monday/.test(text(panel('.ca-md-card')[1].querySelector('.ca-md-card-body'))); }, 3000)
        .then(function () {
          check('picking a version shows it', true);
          panel('.ca-md-card')[0].querySelector('.ca-copybtn').click();
          panel('.ca-md-card')[1].querySelector('.ca-copybtn').click();
          return until(function () { return copied.length === 2; }, 3000);
        })
        .then(function () {
          check('Copy copies the writing without its fences', copied[0] === STATUS, JSON.stringify(copied[0]));
          check('and only the version shown', copied[1] === 'Greg: the pilot is ready. Start Monday.', JSON.stringify(copied[1]));
          var follow = panel('.ca-md-followup');
          check('a follow-up is a button with its words', follow.every(function (b) { return b.tagName === 'BUTTON'; }) && text(follow[0]) === 'Fix the retry check', follow.map(text).join('|'));
          follow[0].click();
          return until(function () { var box = document.querySelector('.ca-sessinsp .ca-composer textarea'); return box && box.value.indexOf(PROMPT) >= 0; }, 3000);
        })
        .then(function () {
          check('a click puts its prompt in the session’s box', true);
          // Back on the first version, for a screenshot.
          panel('.ca-md-card')[1].querySelectorAll('.ca-md-card-tones button')[0].click();
        });
    })
    .then(done)
    .catch(stopped);
})();
