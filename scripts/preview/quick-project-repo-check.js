// Checks that picking another project in the quick panel (⌘P) moves where it starts into that
// project (quick.ts `repoForProject`, Quick.tsx `pickProject`): a repo picked under All projects
// gives way to the new project's first repo, ⌘T then lists that project's repos alone, and going
// back to All projects, which has every repo, leaves the repo picked where it is. And a project
// picked while a skill has moved the start to its repo counts as a pick since: what the skill
// replaced, a repo of another project, doesn't come back once the skill's gone from the input.
//   PREVIEW_WAIT=60000 PREVIEW_SKILLS=scripts/preview/skills-sample.json scripts/preview.sh quick '' scripts/preview/quick-project-repo-check.js
// The board needs two projects with worktrees on this Mac, one with a repo the other hasn't.
(function () {
  localStorage.setItem('codealong.quick.project', 'All projects');
  var input = function () { return q('.ca-quick-input textarea'); };
  var repoChip = function () { return all('.ca-quick-chip').filter(function (b) { return b.title.indexOf('Repo (⌘T)') >= 0; })[0]; };
  var projectChip = function () { return q('.ca-quick-project'); };
  var said = function (el) { return el ? el.textContent.trim() : null; };
  var repoSaid = function () { var c = repoChip(); return c ? said(c.querySelector('span')) : null; };
  var filter = function () { return q('.ca-picker-filter'); };
  var options = function () { return all('.ca-picker-opt'); };
  var titles = function () { return options().map(function (o) { return said(o.querySelector('.ca-picker-title')); }); };
  var named = function (title) { return options().filter(function (o) { return said(o.querySelector('.ca-picker-title')) === title; })[0]; };
  var press = function (letter) { key(input(), { key: letter, code: 'Key' + letter.toUpperCase(), metaKey: true }); };
  var closed = function () { return until(function () { return !filter(); }, 3000, 'the menu closed'); };
  var opened = function () { return until(function () { return options().length > 0; }, 3000, 'a menu'); };
  var from = null;
  var to = null;
  var reposOf = {};
  var lanes = [];
  var skill = null;
  var left = null;

  var pickProject = function (name) {
    press('p');
    return opened().then(function () {
      var row = named(name);
      if (!row) throw new Error('no ' + name + ' in the project menu: ' + titles().join(' | '));
      row.click();
      return closed();
    }).then(function () { return wait(100); });
  };

  say('quick project repo check: waiting for the panel');
  until(function () { return !!input() && !!repoChip() && !repoChip().disabled && calls('quick_ready').length > 0; }, 20000, 'the repo chip')
    .then(function () { return window.__TAURI_INTERNALS__.invoke('get_board'); })
    .then(function (board) {
      lanes = board.lanes.filter(function (l) { return !l.startsChats && !l.device; });
      lanes.forEach(function (l) {
        reposOf[l.project] = reposOf[l.project] || [];
        if (reposOf[l.project].indexOf(l.repo) < 0) reposOf[l.project].push(l.repo);
      });
      var names = (board.projects || []).map(function (p) { return p.name; }).filter(function (n) { return reposOf[n]; });
      // A repo of one project whose name no repo of another has, so the chip can only say it
      // while the start stays in the project left.
      names.some(function (a) {
        return names.some(function (b) {
          if (a === b) return false;
          var only = reposOf[a].filter(function (r) { return reposOf[b].indexOf(r) < 0; })[0];
          if (only) { from = { project: a, repo: only }; to = b; }
          return !!only;
        });
      });
      if (!from) throw new Error('no two projects with worktrees here, one with a repo the other hasn’t');
      press('t');
      return opened();
    })
    .then(function () {
      type(filter(), from.repo);
      return wait(100);
    })
    .then(function () {
      var row = named(from.repo) || named(from.project + ' · ' + from.repo);
      if (!row) throw new Error('no row for ' + from.repo + ': ' + titles().join(' | '));
      row.click();
      return closed();
    })
    .then(function () {
      check('a repo of ' + from.project + ' picked under All projects', repoSaid() === from.repo, repoSaid());
      return pickProject(to);
    })
    .then(function () {
      check('the project chip says ' + to, said(projectChip()).indexOf(to) >= 0, said(projectChip()));
      check('picking ' + to + ' moves the start to a repo of it', reposOf[to].indexOf(repoSaid()) >= 0, repoSaid() + ' (' + to + ': ' + reposOf[to].join(', ') + ')');
      press('t');
      return opened();
    })
    .then(function () {
      var repos = titles().filter(function (t) { return t !== 'General chat' && t !== 'Another folder…'; });
      check('⌘T lists ' + to + '’s repos alone', repos.length > 0 && repos.every(function (t) { return reposOf[to].indexOf(t) >= 0; }), repos.join(' | '));
      press('t');
      return closed();
    })
    .then(function () {
      var before = repoSaid();
      return pickProject('All projects').then(function () {
        check('back on All projects the repo stays where it is', repoSaid() === before, before + ' → ' + repoSaid());
      });
    })
    .then(function () {
      // A skill moves the start to its repo, and what it took the place of comes back once it's
      // gone from the input, unless a project was picked meanwhile. The sample's `shape-up` is in
      // the first repo the panel asks about only.
      type(input(), '/shape');
      return until(function () { return sent('skills').length > 0 && all('.ca-quick-row.is-skill').length > 0; }, 5000, 'skill rows for /shape');
    })
    .then(function () {
      var path = last('skills').folders[0];
      skill = lanes.filter(function (l) { return l.path === path; })[0];
      type(input(), '');
      if (!skill) throw new Error('the first folder asked about for skills isn’t on the board: ' + path);
      var other = Object.keys(reposOf).filter(function (p) { return p !== skill.project; }).map(function (p) {
        return { project: p, repo: reposOf[p].filter(function (r) { return reposOf[skill.project].indexOf(r) < 0; })[0] };
      }).filter(function (o) { return !!o.repo; })[0];
      if (!other) throw new Error('no repo of another project than ' + skill.project + ' with a name of its own');
      left = other;
      return wait(200).then(function () { press('t'); return opened(); });
    })
    .then(function () {
      type(filter(), left.repo);
      return wait(100);
    })
    .then(function () {
      var row = named(left.repo) || named(left.project + ' · ' + left.repo);
      if (!row) throw new Error('no row for ' + left.repo + ': ' + titles().join(' | '));
      row.click();
      return closed();
    })
    .then(function () {
      type(input(), '/shape-up ');
      return until(function () { return repoSaid() === skill.repo; }, 3000, 'the skill moving the start to ' + skill.repo + ' (it says ' + repoSaid() + ')');
    })
    .then(function () { return pickProject(skill.project); })
    .then(function () {
      check('picking the skill’s project keeps its repo', repoSaid() === skill.repo, repoSaid());
      type(input(), '');
      return wait(300);
    })
    .then(function () {
      check('and with the skill gone, the repo of another project it replaced doesn’t come back', repoSaid() === skill.repo, repoSaid() + ' (it replaced ' + left.repo + ')');
    })
    .then(done, stopped);
})();
