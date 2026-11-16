// Checks starting a general chat from the quick panel (Quick.tsx): the repo picker offers
// General chat ahead of the repos, picking it says so on the chip, leaves no worktree to pick and
// words the start row for a chat, and ⌘⏎ starts in Codealong's chats lane with no
// worktree. Opened as the chat shortcut opens it, the panel is on General chat, its project too,
// also when it opened before the first scan finished and the board came after, and its hint names
// that shortcut; an ordinary open after is on a worktree and the project picked before again. The
// shortcut pressed with the panel up on a project goes to General chat for that open alone, and
// pressed there puts the panel away (quick.rs `toggle_chat`). The project menu offers General chat
// too, after All projects, with that shortcut beside it: picked, it lists only the chats, starts in
// Codealong's with ⏎ alone, and a project picked after it starts in that project's repos again.
//   PREVIEW_WAIT=60000 scripts/preview.sh quick '' scripts/preview/quick-chat-check.js
// The board needs Codealong's chats lane, which an engine of this build always has.
(function () {
  localStorage.setItem('codealong.quick.project', 'All projects');
  var input = function () { return q('.ca-quick-input textarea'); };
  var chip = function () { return all('.ca-quick-chip').filter(function (b) { return /Repo \(⌘T\)/.test(b.title); })[0]; };
  var options = function () { return all('.ca-picker-opt'); };
  var generalChat = function () { return options().filter(function (o) { return o.textContent.indexOf('General chat') >= 0; })[0]; };
  var projectChip = function () { return q('.ca-quick-project'); };
  var board;

  say('quick chat check: waiting for the panel');
  until(function () { return !!input() && !!chip() && !chip().disabled; }, 20000, 'the repo chip')
    .then(function () {
      type(input(), 'What is the difference between a mutex and a semaphore?');
      chip().click();
      return until(function () { return options().length > 0; }, 5000, 'the repo picker');
    })
    .then(function () {
      var opts = options();
      var o = generalChat();
      check('the picker offers General chat', !!o, opts.map(function (x) { return x.textContent; }).join(' | '));
      check('ahead of the repos', !!o && opts[0] === o, opts.length + ' options');
      check('in a folder of its own', !!o && /In no repo: a folder of its own/.test(o.textContent), o && o.textContent);
      o.click();
      return wait(300);
    })
    .then(function () {
      check('the chip says General chat', /General chat/.test(chip().textContent), chip().textContent);
      var tree = all('.ca-quick-chip').filter(function (b) { return /Worktree \(⌘B\)/.test(b.title); })[0];
      check('a chat has no worktree to pick', !tree && !q('.ca-quick-newtree'));
      var title = q('.ca-quick-compose .ca-quick-row-title');
      var why = q('.ca-quick-compose .ca-quick-row-why');
      var where = q('.ca-quick-compose .ca-quick-row-where');
      check('the start row says it’s a chat', !!title && title.textContent === 'Start a general chat', title && title.textContent);
      check('in a folder of its own', !!why && /a folder of its own/.test(why.textContent), why && why.textContent);
      check('a general chat', !!where && where.textContent === 'General chat', where && where.textContent);
      key(input(), { key: 'Enter', metaKey: true });
      return until(function () { return sent('start_from_quick').length > 0; }, 5000, 'the start');
    })
    .then(function () {
      var spec = sent('start_from_quick').pop().spec;
      check('⌘⏎ starts in Codealong’s chats', /\/Codealong\/chats$/.test(spec.laneId), spec.laneId);
      check('with no worktree', spec.worktree === false, String(spec.worktree));
      check('on the question', /mutex/.test(spec.prompt), spec.prompt);
      return window.__TAURI_INTERNALS__.invoke('get_board');
    })
    .then(function (b) {
      board = b;
      window.__quickOpen({ chat: true }, Object.assign({}, board, { loading: true, lanes: [] }));
      check('opened before the first scan, no worktree yet', !/General chat/.test(chip().textContent), chip().textContent);
      // The panel listens for the board once it's up.
      return wait(300);
    })
    .then(function () {
      window.__emit('board', board);
      return until(function () { return /General chat/.test(chip().textContent); }, 5000, 'General chat once the board has the chats');
    })
    .then(function () {
      var hint = q('.ca-quick-hotkey');
      check('the hint names the chat shortcut', !!hint && hint.textContent === '⌥⌘\'', hint && hint.textContent);
      check('and the project chip says General chat', /General chat/.test(projectChip().textContent), projectChip().textContent);
      type(input(), 'How do I read a core dump?');
      var starts = sent('start_from_quick').length;
      key(input(), { key: 'Enter', metaKey: true });
      return until(function () { return sent('start_from_quick').length > starts; }, 5000, 'the start');
    })
    .then(function () {
      var spec = sent('start_from_quick').pop().spec;
      check('the chat shortcut starts in Codealong’s chats', /\/Codealong\/chats$/.test(spec.laneId), spec.laneId);
      window.__quickOpen({}, board);
      return wait(300);
    })
    .then(function () {
      check('an ordinary open after is on a worktree', !/General chat/.test(chip().textContent), chip().textContent);
      check('on the project picked before', /All projects/.test(projectChip().textContent), projectChip().textContent);
      // The chat shortcut, pressed with the panel up on a project.
      window.__emit('quick-chat', null);
      return until(function () { return /General chat/.test(projectChip().textContent); }, 5000, 'the chat shortcut going to General chat');
    })
    .then(function () {
      check('the chat shortcut in the panel goes to General chat', /General chat/.test(chip().textContent), chip().textContent);
      check('for this open alone', localStorage.getItem('codealong.quick.project') === 'All projects', localStorage.getItem('codealong.quick.project'));
      var hides = calls('hide_quick').length;
      window.__emit('quick-chat', null);
      return until(function () { return calls('hide_quick').length > hides; }, 5000, 'the chat shortcut on General chat putting the panel away');
    })
    .then(function () {
      window.__quickOpen({}, board);
      return wait(300);
    })
    .then(function () {
      check('and the next open is on the project picked before', /All projects/.test(projectChip().textContent), projectChip().textContent);
      projectChip().click();
      return until(function () { return options().length > 0; }, 5000, 'the project menu');
    })
    .then(function () {
      var opts = options();
      var o = generalChat();
      var everything = opts.filter(function (x) { return /All projects/.test(x.textContent); })[0];
      check('the project menu offers General chat', !!o, opts.map(function (x) { return x.textContent; }).join(' | '));
      check('after All projects', !!o && !!everything && opts.indexOf(o) === opts.indexOf(everything) + 1, opts.length + ' options');
      check('for a question in none of your repos', !!o && /A question for none of your repos/.test(o.textContent), o && o.textContent);
      var keyOf = o && q('.ca-projswitch-key', o);
      check('with the chat shortcut beside it', !!keyOf && keyOf.textContent === '⌥⌘\'', keyOf && keyOf.textContent);
      o.click();
      return wait(300);
    })
    .then(function () {
      check('the project chip says General chat', /General chat/.test(projectChip().textContent), projectChip().textContent);
      check('the repo chip does too', /General chat/.test(chip().textContent), chip().textContent);
      // Every session listed is in a folder chats are kept in.
      var chats = {};
      board.lanes.forEach(function (l) { if (l.chats) l.sessions.forEach(function (s) { chats[s.key] = true; }); });
      var listed = all('.ca-quick-list [data-row]').map(function (r) { return r.getAttribute('data-row'); }).filter(function (k) { return k !== 'compose'; });
      var strays = listed.filter(function (k) { return board.lanes.some(function (l) { return l.sessions.some(function (s) { return s.key === k; }); }) && !chats[k]; });
      check('it lists only the chats', strays.length === 0, strays.join(', ') || listed.length + ' rows');
      type(input(), 'Why does my shell take three seconds to start?');
      return wait(300);
    })
    .then(function () {
      check('no suggestions are asked for a chat', !q('.ca-quick-row.is-suggest'));
      var starts = sent('start_from_quick').length;
      key(input(), { key: 'Enter' });
      return until(function () { return sent('start_from_quick').length > starts; }, 5000, '⏎ to start');
    })
    .then(function () {
      var spec = sent('start_from_quick').pop().spec;
      check('⏎ starts a general chat', /\/Codealong\/chats$/.test(spec.laneId), spec.laneId);
      window.__quickOpen({}, board);
      return wait(300);
    })
    .then(function () {
      check('it opens on General chat again', /General chat/.test(projectChip().textContent), projectChip().textContent);
      projectChip().click();
      return until(function () { return options().length > 0; }, 5000, 'the project menu');
    })
    .then(function () {
      var o = options().filter(function (x) { return !/General chat|All projects/.test(x.textContent); })[0];
      if (!o) { check('a project to pick', false); return; }
      o.click();
      return wait(300).then(function () {
        check('a project picked after starts in its repos', !/General chat/.test(chip().textContent), chip().textContent);
      });
    })
    .then(function () {
      done();
    })
    .catch(stopped);
})();
