// Checks the PRs view while GitHub's PRs are on their way (boardLists.ts `prsRead`): with none of
// the project's PRs come yet, it says it's reading them, the mark at work and a tip under it, the
// board taking the panel's room, rather than that there are none; with GitHub's notice up it says
// why instead, the panel back; and once they come it lists them.
//   PREVIEW_WAIT=60000 scripts/preview.sh prs '' scripts/preview/prs-reading-check.js
(function () {
  var text = function (sel) { var el = document.querySelector(sel); return el ? el.textContent : ''; };
  var json = function (file) { return fetch(file).then(function (r) { return r.json(); }); };
  var rows = function () { return all('[role="listbox"] [role="option"]').length; };
  var emit = function (board, over) { window.__emit('board', Object.assign({}, board, over)); };

  say('prs reading check: waiting for the PRs view');
  json('./board.json')
    .then(function (real) {
      return until(function () { return !!document.querySelector('.ca-board') && !document.querySelector('.ca-board-loading'); }, 20000).then(function () {
        var project = text('.ca-projswitch-name');
        var repos = real.lanes.filter(function (l) { return l.project === project && l.repoSlug; });
        if (repos.length === 0) {
          check('the project shown has a repo on GitHub', false, project);
          return null;
        }
        var others = real.notices.filter(function (n) { return n.key !== 'github'; });
        // The board as it is between the first scan and GitHub's first poll ending: no PRs, and
        // each repo the poll asked about not read.
        var current = {};
        Object.keys(real.githubCurrent || {}).forEach(function (slug) { current[slug] = null; });
        var unread = { prs: [], githubPolled: false, githubCurrent: current, notices: others };
        emit(real, unread);
        return until(function () { return !!document.querySelector('.ca-board-loading'); }, 5000)
          .then(function () { return until(function () { return !!document.querySelector('.ca-board-loading .ca-tip'); }, 5000).catch(function () {}); })
          .then(function () {
            check('the view says it is reading PRs', text('.ca-board-loading .ca-reading-what') === 'Reading PRs from GitHub…', text('.ca-board-loading .ca-reading-what'));
            check('with the mark at work', !!document.querySelector('.ca-board-loading .ca-lmark[data-status="working"]'));
            check('and a tip under it', !!document.querySelector('.ca-board-loading .ca-tip[data-tip]'), text('.ca-board-loading .ca-tip-text'));
            check('not that there are none', !document.querySelector('.ca-board-empty'), text('.ca-board-empty'));
            check('and the panel gives the board its room', !document.querySelector('.ca-insp'));
            check('the header keeps the project', text('.ca-projswitch-name') === project, text('.ca-projswitch-name'));
            // GitHub can't be asked: waiting won't bring them, so it says why.
            emit(real, Object.assign({}, unread, { notices: others.concat([{ key: 'github', title: 'Signed out of GitHub', text: 'Signed out of GitHub: run gh auth login.' }]) }));
            return until(function () { return !document.querySelector('.ca-board-loading'); }, 5000);
          })
          .then(function () {
            check('with GitHub failing it says why', text('.ca-board-empty') === "PRs can't be read: Signed out of GitHub.", text('.ca-board-empty'));
            check('and the panel is back', !!document.querySelector('.ca-insp'));
            emit(real, {});
            var listed = real.prs.filter(function (p) { return p.project === project; }).length;
            return until(function () { return !document.querySelector('.ca-board-loading') && (listed === 0 || rows() > 0); }, 5000).then(function () {
              if (listed > 0) check('once they come the view lists them', rows() > 0, rows() + ' rows');
              else check('once they come, none, it says so', text('.ca-board-empty') === 'No PRs here right now.', text('.ca-board-empty'));
              check('and the panel is back with them', !!document.querySelector('.ca-insp'));
            });
          });
      });
    })
    .then(done, stopped);
})();
