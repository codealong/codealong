// Checks the skills in a session's reply box (SessionReply.tsx, SlashList.tsx, skills.ts), in the
// bar's peek: a slash and the start of a name that start the text list the session's skills (its worktree's and its
// account's, only the ones its agent reads) under Codealong's own commands; ↑ ↓ choose, ⇥
// finishes the name, Escape puts the list away without stopping anything; and a skill is sent
// as the session's agent takes it (`$name` to Codex, `/name` to Claude Code).
//   PREVIEW_WAIT=60000 PREVIEW_SKILLS=scripts/preview/skills-sample.json scripts/preview.sh peek '' scripts/preview/skills-reply-check.js
// The sample has `ship` for both agents, `shape-up` for Claude Code only, `probe` for Codex
// only and `notes:tidy` as the account's own. A Codex session is used when the board has one.
(function () {
  var xhr = new XMLHttpRequest();
  xhr.open('GET', './board.json', false);
  xhr.send();
  var board = JSON.parse(xhr.responseText);
  var picked = null;
  var pick = function (harness) {
    board.lanes.forEach(function (l) { l.sessions.forEach(function (s) { if (!picked && !s.archived && !s.visitor && !l.hidden && !l.device && s.harness === harness && s.cwd) picked = s; }); });
  };
  pick('codex');
  pick('claude');
  if (!picked) { stopped('no Codex or Claude Code session to peek at'); return; }
  var codex = picked.harness === 'codex';
  var sessionKey = picked.key;
  localStorage.setItem('codealong.preview.peek', sessionKey);
  localStorage.removeItem('codealong.draft.' + sessionKey);
  // Working, so Escape would stop its turn if the list didn't take it.
  var busy = { status: 'working', live: true, stoppable: true, stalledSince: null, asks: false };
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd) {
    var got = real.apply(this, arguments);
    if (cmd !== 'get_board') return got;
    return got.then(function (b) {
      b.lanes.forEach(function (l) { l.sessions.forEach(function (s) { if (s.key === sessionKey) Object.assign(s, busy); }); });
      return b;
    });
  };

  var box = function () { return q('.ca-composer textarea'); };
  var rows = function () { return all('.ca-slash-row'); };
  var names = function () { return rows().map(function (r) { return q('.ca-slash-name', r).textContent.trim().split(' ')[0]; }); };
  var active = function () { var r = q('.ca-slash-row.is-active'); return r ? q('.ca-slash-name', r).textContent.trim().split(' ')[0] : null; };
  var noteText = function () { var n = q('.ca-composer-note'); return n ? n.textContent : ''; };

  (async function () {
    try {
      await until(box, 20000);
      if (!codex) note('no Codex session on the board: checked on a Claude Code one, where a skill goes as typed');
      type(box(), '/');
      await until(function () { return rows().length > 0 && names().indexOf('/ship') >= 0; }, 5000, 'no list for /');
      var asked = sent('skills')[0];
      check('the session’s folder and account are asked about', asked.folders.length === 1 && asked.folders[0] === picked.cwd && asked.accounts.length === 1 && asked.accounts[0] === picked.account, JSON.stringify(asked));
      var got = names();
      check('Codealong’s own commands lead the list', got[0] === '/fork', got.join(' '));
      check('then the worktree’s skills, then the account’s', got.indexOf('/ship') > 0 && got.indexOf('/notes:tidy') > got.indexOf('/ship'), got.join(' '));
      check('only the ones its agent reads', codex ? got.indexOf('/probe') >= 0 && got.indexOf('/shape-up') < 0 : got.indexOf('/shape-up') >= 0 && got.indexOf('/probe') < 0, got.join(' '));

      type(box(), '/sh');
      await until(function () { return names()[0] === '/ship'; }, 3000, 'no /ship first for /sh');
      check('the first is the one chosen', active() === '/ship', String(active()));
      if (rows().length > 1) {
        key(box(), { key: 'ArrowDown' });
        await wait(50);
        check('↓ chooses the next', active() === names()[1], String(active()));
        key(box(), { key: 'ArrowUp' });
        await wait(50);
      }
      var stops = sent('session_cancel').length;
      key(box(), { key: 'Escape' });
      await wait(100);
      check('Escape puts the list away', rows().length === 0);
      check('and stops nothing', sent('session_cancel').length === stops);
      type(box(), '/shi');
      await until(function () { return rows().length > 0; }, 3000, 'the list didn’t come back as the name changed');
      key(box(), { key: 'Tab' });
      await until(function () { return box().value === '/ship '; }, 3000, 'the name wasn’t finished: ' + box().value);
      check('⇥ finishes the name', true);
      check('and the list is done', rows().length === 0);
      check('the line under the box says what the skill takes and does', noteText().indexOf('/ship [branch]: Commit') === 0, noteText());

      var sends = sent('send_to_session').length;
      type(box(), '/ship the fix');
      await wait(50);
      key(box(), { key: 'Enter' });
      await until(function () { return sent('send_to_session').length > sends; }, 5000, 'nothing sent');
      var text = last('send_to_session').text;
      check('a skill goes as the session’s agent takes it', text === (codex ? '$ship the fix' : '/ship the fix'), text);

      sends = sent('send_to_session').length;
      type(box(), '/shi');
      await until(function () { return rows().length > 0; }, 3000, 'no list for /shi');
      key(box(), { key: 'Enter' });
      await until(function () { return box().value === '/ship '; }, 3000, '⏎ didn’t finish the name: ' + box().value);
      check('⏎ on part of a name finishes it and sends nothing', sent('send_to_session').length === sends);
      type(box(), '/ship');
      await wait(50);
      key(box(), { key: 'Enter' });
      await until(function () { return sent('send_to_session').length > sends; }, 5000, 'a whole name wasn’t sent');
      check('⏎ on a whole name sends it', last('send_to_session').text === (codex ? '$ship' : '/ship'), last('send_to_session').text);

      // Only a name that starts the text is a skill: one after other words lists nothing, and
      // goes as typed.
      sends = sent('send_to_session').length;
      type(box(), 'look at it, then /shi');
      await wait(300);
      check('a name after other words lists nothing', rows().length === 0, names().join(' '));
      type(box(), 'look at it, then /ship');
      await wait(50);
      key(box(), { key: 'Enter' });
      await until(function () { return sent('send_to_session').length > sends; }, 5000, 'nothing sent with a name after words');
      check('and goes as typed', last('send_to_session').text === 'look at it, then /ship', last('send_to_session').text);

      sends = sent('send_to_session').length;
      type(box(), '/compact');
      await wait(50);
      key(box(), { key: 'Enter' });
      await until(function () { return sent('send_to_session').length > sends; }, 5000, '/compact wasn’t sent');
      check('what isn’t a skill goes as typed', last('send_to_session').text === '/compact', last('send_to_session').text);
    } catch (e) {
      return stopped(e);
    }
    done();
  })();
})();
