// Checks the quick panel's skills (Quick.tsx, skills.ts): a slash and the start of a name list
// the repos' skills, ⏎ finishes the name, the session starts in that skill's repo with the name
// written as the account's agent takes it, and only accounts whose agent reads the skill are
// offered. What starts with a slash but names no skill is words like any other.
//   PREVIEW_WAIT=60000 PREVIEW_SKILLS=scripts/preview/skills-sample.json scripts/preview.sh quick '' scripts/preview/skills-quick-check.js
// The sample puts `ship` in every repo and `shape-up` (Claude Code's only) and `probe` (Codex's
// only) in the first. The board needs a repo on this Mac in a project.
(function () {
  localStorage.setItem('codealong.quick.project', 'All projects');
  var board = null;
  var options = null;
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, o) {
    var p = real(cmd, args, o);
    if (cmd === 'get_board') p.then(function (b) { board = b; });
    if (cmd === 'start_options') p.then(function (x) { options = x; });
    return p;
  };
  var input = function () { return q('.ca-quick-input textarea'); };
  var rows = function () { return all('.ca-quick-row.is-skill'); };
  var names = function () { return rows().map(function (r) { return q('.ca-quick-row-title', r).textContent.trim().split(' ')[0]; }); };
  var laneAt = function (path) { return board.lanes.filter(function (l) { return l.path === path && !l.device; })[0]; };
  var laneOf = function (id) { return board.lanes.filter(function (l) { return l.id === id; })[0]; };
  var harnessOf = function (account) { return (options.accounts.filter(function (a) { return a.name === account; })[0] || {}).harness; };
  var startable = function (harness) { return options.accounts.some(function (a) { return a.harness === harness && a.places.length > 0; }); };
  var first = null;
  var startsBefore = 0;

  say('skills quick check: waiting for the panel');
  until(function () { return !!board && !!options && !!input() && sent('quick_ready').length > 0; }, 20000)
    .then(function () { return wait(300); })
    .then(function () {
      type(input(), '/sh');
      return until(function () { return rows().length > 0; }, 5000, 'no skill rows for /sh');
    })
    .then(function () {
      var asked = sent('skills')[0];
      first = laneAt(asked.folders[0]);
      check('the skills of the project’s repos are asked for, not accounts’', asked.folders.length > 0 && asked.accounts.length === 0, JSON.stringify(asked).slice(0, 200));
      var got = names();
      check('a skill every repo has leads the list, and leaves the others room', got[0] === '/ship' && got[1] !== '/ship', got.join(' '));
      if (asked.folders.length > 1) check('and is a row for each repo after them', got.filter(function (n) { return n === '/ship'; }).length > 1, got.join(' '));
      check('a skill in one repo is one row', got.filter(function (n) { return n === '/shape-up'; }).length === 1, got.join(' '));
      check('only skills are listed', all('.ca-quick-row').length === rows().length + 1, String(all('.ca-quick-row').length));
      check('the first row is the one ⏎ does', rows()[0].classList.contains('is-active'));
      type(input(), '/shape');
      return until(function () { return names().length === 1 && names()[0] === '/shape-up'; }, 3000, 'no single /shape-up row');
    })
    .then(function () {
      check('its row names its repo', q('.ca-quick-row-where', rows()[0]).textContent.trim() === first.repo, q('.ca-quick-row-where', rows()[0]).textContent);
      startsBefore = sent('start_from_quick').length;
      key(input(), { key: 'Enter' });
      return until(function () { return input().value === '/shape-up '; }, 3000, 'the name wasn’t finished: ' + input().value);
    })
    .then(function () { return wait(200); })
    .then(function () {
      check('finishing a name starts nothing', sent('start_from_quick').length === startsBefore);
      check('with the name whole there’s nothing left to list', rows().length === 0, names().join(' '));
      check('the start row says the skill and its repo', q('.ca-quick-compose').textContent.indexOf('Runs /shape-up from ' + first.repo) >= 0, q('.ca-quick-compose').textContent);
      if (options.accounts.some(function (a) { return a.harness !== 'claude'; })) check('and which accounts it leaves out', /isn’t where .* reads? skills/.test(q('.ca-quick-compose').textContent), q('.ca-quick-compose').textContent);
      type(input(), '/shape-up the next cycle');
      return wait(200);
    })
    .then(function () {
      key(input(), { key: 'Enter', metaKey: true });
      return until(function () { return sent('start_from_quick').length > startsBefore; }, 3000, 'no start');
    })
    .then(function () {
      var spec = last('start_from_quick').spec;
      var lane = laneOf(spec.laneId);
      check('the session starts in the skill’s repo', !!lane && lane.repo === first.repo && lane.project === first.project, JSON.stringify(spec).slice(0, 200));
      check('on an account whose agent reads it', harnessOf(spec.account) === 'claude', spec.account);
      check('with the skill’s name as that agent takes it', spec.prompt === '/shape-up the next cycle', spec.prompt);
      return wait(400);
    })
    .then(function () {
      if (!startable('codex')) {
        note('no Codex account that can start: a skill only Codex reads wasn’t started');
        return null;
      }
      startsBefore = sent('start_from_quick').length;
      type(input(), '/probe the login page');
      return wait(400).then(function () {
        key(input(), { key: 'Enter', metaKey: true });
        return until(function () { return sent('start_from_quick').length > startsBefore; }, 3000, 'no start on /probe');
      }).then(function () {
        var spec = last('start_from_quick').spec;
        check('a skill only Codex reads starts on a Codex account', harnessOf(spec.account) === 'codex', spec.account);
        check('written as Codex takes it', spec.prompt === '$probe the login page', spec.prompt);
        return wait(400);
      });
    })
    .then(function () {
      type(input(), '/tmp is full again');
      return wait(500);
    })
    .then(function () {
      startsBefore = sent('start_from_quick').length;
      check('a slash that names no skill lists no skills', rows().length === 0);
      check('and names none on the start row', q('.ca-quick-compose').textContent.indexOf('Runs /') < 0, q('.ca-quick-compose').textContent);
      key(input(), { key: 'Enter', metaKey: true });
      return until(function () { return sent('start_from_quick').length > startsBefore; }, 3000, 'no start on plain words');
    })
    .then(function () {
      check('and goes as typed', last('start_from_quick').spec.prompt === '/tmp is full again', last('start_from_quick').spec.prompt);
      done();
    })
    .catch(stopped);
})();
