// Checks starting a chat in no project from the quick panel (Quick.tsx): the worktree picker
// offers No project ahead of the worktrees, picking it says so on the chip, turns New worktree
// off and words the start row for a chat, and ⌘⏎ starts in Codealong's chats lane with no
// worktree.
//   PREVIEW_WAIT=60000 scripts/preview.sh quick '' scripts/preview/quick-chat-check.js
// The board needs Codealong's chats lane, which an engine of this build always has.
(function () {
  localStorage.setItem('codealong.quick.project', 'All projects');
  var input = function () { return q('.ca-quick-input textarea'); };
  var chip = function () { return all('.ca-quick-chip').filter(function (b) { return /Worktree \(⌘T\)/.test(b.title); })[0]; };
  var options = function () { return all('.ca-picker-opt'); };
  var noProject = function () { return options().filter(function (o) { return o.textContent.indexOf('No project') >= 0; })[0]; };

  say('quick chat check: waiting for the panel');
  until(function () { return !!input() && !!chip() && !chip().disabled; }, 20000, 'the worktree chip')
    .then(function () {
      type(input(), 'What is the difference between a mutex and a semaphore?');
      chip().click();
      return until(function () { return options().length > 0; }, 5000, 'the worktree picker');
    })
    .then(function () {
      var opts = options();
      var o = noProject();
      check('the picker offers No project', !!o, opts.map(function (x) { return x.textContent; }).join(' | '));
      check('ahead of the worktrees', !!o && opts[0] === o, opts.length + ' options');
      check('as a chat of its own', !!o && /A chat of its own/.test(o.textContent), o && o.textContent);
      o.click();
      return wait(300);
    })
    .then(function () {
      check('the chip says No project', /No project/.test(chip().textContent), chip().textContent);
      var tree = q('.ca-quick-newtree input');
      check('a chat has no new worktree', !!tree && tree.disabled && !tree.checked);
      var why = q('.ca-quick-compose .ca-quick-row-why');
      check('the start row says it’s a chat', !!why && /a chat of its own/.test(why.textContent), why && why.textContent);
      key(input(), { key: 'Enter', metaKey: true });
      return until(function () { return sent('start_from_quick').length > 0; }, 5000, 'the start');
    })
    .then(function () {
      var spec = sent('start_from_quick').pop().spec;
      check('⌘⏎ starts in Codealong’s chats', /\/Codealong\/chats$/.test(spec.laneId), spec.laneId);
      check('with no worktree', spec.worktree === false, String(spec.worktree));
      check('on the question', /mutex/.test(spec.prompt), spec.prompt);
      done();
    })
    .catch(stopped);
})();
