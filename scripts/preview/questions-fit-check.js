// Checks that questions a session asks fit a card too short for them (the peek, a short waiting
// window): the box's head, the box for your own answer, Decline and Answer, and the message box
// under them all stay inside the card, the question with its choices scrolls as one, the
// conversation above gave up its room before the question did, and the last choice can be
// scrolled to and picked. In a kept card made as small as it goes, too short even for that, the
// question keeps three lines, the box scrolls as a whole and keeps room to show Decline and Answer
// whole, and the card scrolls to the message box. At every size no part of the box lies over
// another or over the message box, and each control the check asks of shows whole, inside every
// scroller around it, and takes a click at its middle. The pick goes on to a second question
// without choices (Antigravity's to write an answer to), whose words keep all their room.
//   scripts/preview.sh peek '' scripts/preview/questions-fit-check.js
//   PREVIEW_SIZE=820,520 scripts/preview.sh waiting '' scripts/preview/questions-fit-check.js
//   PREVIEW_SIZE=520,340 scripts/preview.sh kept '' scripts/preview/questions-fit-check.js
//   PREVIEW_SIZE=400,280 scripts/preview.sh kept '' scripts/preview/questions-fit-check.js
(function () {
  var picked = null;
  var pending = {
    id: 'ask-fit',
    label: 'Asks you · Do you approve the revised customer email?',
    input: null,
    cannot: null,
    questions: [
      {
        question: 'Do you approve the revised customer email (full text: /private/tmp/claude-501/-Users-someone-projects-example-repo--claude-worktrees-swift-enchanting-wand/ca82d392-b40d-49d7-a308-618f9c5a29c3/scratchpad/customer-update-draft.md), and who signs it? You or your partner send it; I won’t. Two bracketed items are checked at send time: the upload date and whether the PR has merged.',
        header: 'Email',
        options: [
          { label: 'Approve, I sign', description: 'Signed by you. You send it before Thursday’s check-in; I record the approval and send time once you tell me it’s sent.' },
          { label: 'Approve, partner signs', description: 'Signed by your partner. They send it; I record the approval and send time once told.' },
          { label: 'Changes needed', description: 'Tell me what to change, and I’ll revise and show it again.' },
          { label: 'Hold it', description: 'Don’t send anything this week; I note why and ask again after the check-in.' },
        ],
      },
      { question: 'Anything to add before it goes? Write it as it should read in the email, and say which paragraph it goes after; leave it empty for none.', options: [] },
    ],
    unwritten: true,
  };
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    if (cmd === 'session_pending' && picked && (args || {}).key === picked) {
      window.__calls.push([cmd, args, options]);
      return Promise.resolve(pending);
    }
    var p = real(cmd, args, options);
    if (cmd !== 'get_board') return p;
    return p.then(function (board) {
      board.lanes.forEach(function (l) {
        l.sessions.forEach(function (s) {
          if (s.status === 'needs' || s.status === 'error') s.status = 'idle';
          s.snoozedUntil = undefined;
          s.raised = undefined;
          if ((picked && s.key !== picked) || l.hidden || s.visitor || s.archived || s.harness !== 'claude') return;
          s.status = 'needs';
          s.statusSince = Date.now() - 60e3;
          s.asks = false;
          s.hosted = false;
          s.excerpt = pending.label;
          picked = s.key;
        });
      });
      return board;
    });
  };

  var box = function () { return q('.ca-composer.is-questions'); };
  var rect = function (el) { return el.getBoundingClientRect(); };
  var inside = function (el, card) { return !!el && rect(el).top >= rect(card).top - 1 && rect(el).bottom <= rect(card).bottom + 1; };
  var scrolls = function (el) { return el.scrollHeight > el.clientHeight + 1; };
  // Whole inside every box around it that clips (or with `middle`, its middle: a choice can be
  // taller than three lines), and what a click at its middle lands on: a control cut to a sliver
  // by a scroller is no more use than one off the card.
  var shows = function (el, middle) {
    var r = rect(el);
    var y = r.top + r.height / 2;
    for (var p = el.parentElement; p; p = p.parentElement) {
      if (getComputedStyle(p).overflowY === 'visible') continue;
      if (middle ? y < rect(p).top || y > rect(p).bottom : !inside(el, p)) return false;
    }
    var at = document.elementFromPoint(r.left + r.width / 2, y);
    return !!at && el.contains(at);
  };
  // A kept card made as small as it goes has no room for the box's parts and three lines.
  var tiny = window.innerHeight < 400;

  say('questions fit check: waiting for the questions');
  until(function () { return box() && q('.ca-question-choice', box()); }, 20000)
    .then(function () { return wait(300); })
    .then(function () {
      var card = box().closest('.ca-reply');
      var body = q('.ca-question-body', box());
      var thread = q('.ca-thread', card);
      var message = all('.ca-composer', card).filter(function (c) { return !c.classList.contains('is-questions'); })[0];
      var cs = getComputedStyle(thread);
      var room = thread.clientHeight - parseFloat(cs.paddingTop) - parseFloat(cs.paddingBottom);
      var foot = q('.ca-composer-foot', box());
      var buttons = all('button', foot);
      var decline = buttons[0];
      var next = buttons[buttons.length - 1];
      note('card ' + card.clientHeight + ' of ' + card.scrollHeight + ', box ' + box().clientHeight + ' of ' + box().scrollHeight + ', question ' + body.clientHeight + ' of ' + body.scrollHeight + ', conversation ' + Math.round(room));
      check('the card is too short for all of it', scrolls(body), body.clientHeight + ' of ' + body.scrollHeight);
      var parts = ['.ca-approval-head', '.ca-approval-unwritten', '.ca-question-steps', '.ca-question-body', '.ca-question-own', '.ca-composer-foot'].map(function (sel) { return q(sel, box()); });
      var over = parts.filter(function (el, i) { return i > 0 && rect(el).top < rect(parts[i - 1]).bottom - 1; });
      check('no part of the box lies over another', parts.every(Boolean) && over.length === 0, over.map(function (el) { return el.className; }).join(', '));
      check('the box ends above the message box', rect(box()).bottom <= rect(message).top + 1, Math.round(rect(box()).bottom) + ' > ' + Math.round(rect(message).top));
      check('the conversation gave up its room first', room <= 1, Math.round(room) + 'px left to it');
      check('the question shows three lines at least', body.clientHeight >= 54, body.clientHeight + 'px');
      if (tiny) {
        check('the box keeps room to show its buttons whole', box().offsetHeight >= 112, box().offsetHeight + 'px');
        check('the box and the card scroll', scrolls(box()) && scrolls(card), box().clientHeight + ' of ' + box().scrollHeight + ', ' + card.clientHeight + ' of ' + card.scrollHeight);
        foot.scrollIntoView({ block: 'nearest' });
        check('scrolled to, Decline shows whole', shows(decline));
        check('scrolled to, Next shows whole', shows(next), next.textContent);
        message.scrollIntoView({ block: 'nearest' });
        check('scrolled to, the message box shows whole', inside(message, card), Math.round(rect(message).bottom) + ' > ' + Math.round(rect(card).bottom));
      } else {
        check('the box and the card hold still', !scrolls(box()) && !scrolls(card), box().clientHeight + ' of ' + box().scrollHeight + ', ' + card.clientHeight + ' of ' + card.scrollHeight);
        check('the head shows', shows(parts[0].querySelector('span')));
        check('your own answer shows', shows(parts[4]));
        check('Decline shows', shows(decline));
        check('Next shows', shows(next), next.textContent);
        check('the message box is in the card', inside(message, card));
      }
      var choices = all('.ca-question-choice', box());
      var last = choices[choices.length - 1];
      last.scrollIntoView({ block: tiny ? 'center' : 'nearest' });
      check(tiny ? 'the last choice scrolls into view' : 'the last choice scrolls into view whole', shows(last, tiny));
      var r = rect(last);
      document.elementFromPoint(r.left + r.width / 2, r.top + r.height / 2).click();
      // A pick on a question that takes one goes on to the next, which has no choices.
      return until(function () { return box() && !q('.ca-question-choice', box()) && q('.ca-question-body', box()); }, 2000, 'the question without choices');
    })
    .then(function (body) {
      var label = q('.ca-approval-label', body);
      note('without choices: box ' + box().clientHeight + ' of ' + box().scrollHeight + ', question ' + body.clientHeight + ' of ' + body.scrollHeight);
      check('a question without choices keeps all its words', body.clientHeight > 0 && !scrolls(body), body.clientHeight + ' of ' + body.scrollHeight);
      label.scrollIntoView({ block: 'nearest' });
      check('its words show', shows(label, tiny));
      check('it asks for your answer', q('.ca-question-own', box()).placeholder === 'Type your answer');
      all('.ca-question-steps button', box())[0].click();
      return until(function () { var c = all('.ca-question-choice', box()); return c.length && c.pop().getAttribute('aria-checked') === 'true'; }, 2000, 'the first question again, its last choice picked');
    })
    .then(function () { check('a click on the last choice picked it', true); })
    .then(done)
    .catch(stopped);
})();
