// Checks how PRs, Todo and Sessions are sorted and narrowed (PrsView.tsx, TodoView.tsx, Sidebar.tsx,
// boardLists.ts `Facets`): Sort by over PRs and Todo lists them under the days they were opened or
// added, each row's age saying so, remembers it, and leaves Todo's order alone; every PR row has
// its repo's chip, on one line and leading the second on two, whose double-click shows only that
// repo's PRs and picks it in the sidebar, and again shows them all; the sidebar's Repos, Labels,
// Authors (you first) and Agents narrow PRs, Todo and Sessions (archived ones too), each pick with
// its count, Clear undoes them, the folded rail names them, and a saved view keeps them.
//   PREVIEW_TODOS=scripts/preview/todos-sample.json PREVIEW_WAIT=60000 scripts/preview.sh prs '' scripts/preview/list-facets-check.js
(function () {
  localStorage.removeItem('codealong.prSort');
  localStorage.removeItem('codealong.todoSort');
  var fire = function (el, type) { el.dispatchEvent(new MouseEvent(type, { bubbles: true, cancelable: true, button: 0, detail: type === 'dblclick' ? 2 : 1 })); };
  var nav = function (label) {
    return all('.ca-side .ca-nav').filter(function (b) { var l = q('.ca-nav-label', b); return l && l.textContent === label; })[0];
  };
  var tool = function (label) { return q('.ca-listtools button[aria-label="' + label + '"]'); };
  var heads = function () { return all('.ca-board-list .ca-bay').map(function (h) { return q('span', h).textContent; }); };
  var DAYS = ['Pinned', 'Today', 'Yesterday', 'Last 7 days', 'Older', 'Merged & closed', 'Done'];
  var byDay = function () { var h = heads(); return h.length > 0 && h.every(function (x) { return DAYS.indexOf(x) >= 0; }); };
  // A facet's chips in the sidebar, under its heading.
  var chips = function (label) {
    var head = all('.ca-side-label--row').filter(function (h) { return h.firstChild.textContent === label; })[0];
    var box = head && head.nextElementSibling;
    return box ? all('.ca-side-chip', box) : [];
  };
  var clear = function (label) {
    var head = all('.ca-side-label--row').filter(function (h) { return h.firstChild.textContent === label; })[0];
    return head && q('.ca-side-clear', head);
  };
  var picked = function (label) { return chips(label).filter(function (c) { return c.classList.contains('is-on'); }); };
  var countOf = function (chip) { return Number((q('.ca-side-chip-count', chip) || {}).textContent); };
  // A repo chip's slug: its tooltip leads with it.
  var slugOf = function (chip) { return chip.title.split('. ')[0]; };
  var rowSlugs = function () { return all('.ca-prrow').map(function (r) { var c = q('.ca-repochip', r); return c ? slugOf(c) : ''; }); };

  (async function () {
    try {
      say('list facets check: waiting for the PRs');
      await until(function () { return all('.ca-prrow').length > 0; }, 20000, 'PR rows');

      // Sorting PRs.
      check('PRs: Sort by and Rows over the list', !!tool('Status') && !!tool('Opened') && !!tool('One line'));
      check('sorted by status to begin with', tool('Status').classList.contains('is-on'));
      tool('Opened').click();
      await until(byDay, 3000, 'PRs under the days they were opened');
      check('Opened lists them under the days they were opened, and is remembered', localStorage.getItem('codealong.prSort') === 'opened', heads().join(' | '));
      var ages = all('.ca-prrow .ca-seg-age').map(function (a) { return a.title; });
      check('each row’s age says when it was opened', ages.length > 0 && ages.every(function (t) { return /^Opened /.test(t); }), ages[0]);
      tool('Status').click();
      await until(function () { return localStorage.getItem('codealong.prSort') === 'status' && !byDay(); }, 3000, 'PRs by status again');

      // The repo chip.
      check('every PR row has its repo’s chip', all('.ca-prrow').every(function (r) { return !!q('.ca-repochip', r); }));
      tool('Two lines').click();
      await until(function () { return all('.ca-prrow').every(function (r) { return !r.classList.contains('is-slim'); }); }, 3000, 'two lines');
      check('on two lines it leads the second line', all('.ca-prrow').every(function (r) { var c = q('.ca-repochip', r); return !!c && !!c.closest('.ca-lane-line2') && c.parentElement.firstElementChild === c; }));
      tool('One line').click();
      await until(function () { return all('.ca-prrow').every(function (r) { return r.classList.contains('is-slim'); }); }, 3000, 'one line');

      var repos = chips('Repos');
      check('the sidebar lists the repos, each with its count', repos.length > 0 && repos.every(function (c) { return countOf(c) >= 0; }), repos.map(function (c) { return c.textContent; }).join(', '));
      var total = all('.ca-prrow').length;
      var first = q('.ca-prrow .ca-repochip');
      var slug = slugOf(first);
      fire(first, 'dblclick');
      await until(function () { return rowSlugs().every(function (s) { return s === slug; }); }, 3000, 'only its repo’s PRs');
      check('a double-click on a repo’s chip shows only its PRs, picked in the sidebar', picked('Repos').length === 1 && picked('Repos')[0].title === slug, picked('Repos').map(function (c) { return c.title; }).join(', '));
      fire(q('.ca-prrow .ca-repochip'), 'dblclick');
      await until(function () { return all('.ca-prrow').length === total; }, 3000, 'every PR again');
      check('and another shows them all again', picked('Repos').length === 0);
      if (repos.length > 1) {
        var other = chips('Repos')[1];
        other.click();
        await until(function () { return rowSlugs().every(function (s) { return s === other.title; }); }, 3000, 'the repo picked in the sidebar');
        check('a repo picked in the sidebar narrows the list to it', all('.ca-prrow').length > 0);
        check('its Clear unpicks it', !!clear('Repos'));
        clear('Repos').click();
        await until(function () { return all('.ca-prrow').length === total; }, 3000, 'every PR after Clear');
      } else {
        note('one repo only: the sidebar’s pick not tried');
      }
      var labels = chips('Labels');
      if (labels.length > 0) {
        var label = q('span', labels[0]).textContent;
        labels[0].click();
        await until(function () { return all('.ca-prrow').every(function (r) { return all('.ca-prlabel-name', r).some(function (n) { return n.textContent === label; }); }); }, 3000, 'the PRs with the label picked');
        check('a PR label picked narrows the list to the PRs that have it', all('.ca-prrow').length > 0, label);
        clear('Labels').click();
        await until(function () { return all('.ca-prrow').length === total; }, 3000, 'every PR after Clear');
      } else {
        note('no PR has a label: Labels not tried');
      }
      var authors = chips('Authors');
      if (authors.length > 0) {
        var board = await window.__TAURI_INTERNALS__.invoke('get_board');
        var authorOf = {};
        board.prs.forEach(function (p) { authorOf['row-pr:' + p.key] = p.author; });
        var names = authors.map(function (c) { return q('span', c).textContent; });
        check('the sidebar lists who opened them, you first where you did', names.indexOf('you') <= 0, names.join(', '));
        // Its login, from its tooltip: "PRs ana opened", or "PRs you opened, as ana".
        var login = authors[0].title.replace(/^PRs you opened, as /, '').replace(/^PRs (.+) opened$/, '$1');
        authors[0].click();
        await until(function () { return all('.ca-prrow').length > 0 && all('.ca-prrow').every(function (r) { return authorOf[r.id] === login; }); }, 3000, 'the PRs ' + login + ' opened');
        check('an author picked narrows the list to the PRs they opened', picked('Authors').length === 1, login);
        clear('Authors').click();
        await until(function () { return all('.ca-prrow').length === total; }, 3000, 'every PR after Clear');
      } else {
        note('no PR says who opened it: Authors not tried');
      }

      // Todo: sorted by when added, narrowed by labels.
      nav('Todo').click();
      await until(function () { return all('.ca-todorow').length > 0 && tool('Added'); }, 5000, 'the todos');
      check('Todo: sorted by status to begin with, in your order', tool('Status').classList.contains('is-on') && all('.ca-todorow[data-pin-index]').length > 0);
      tool('Added').click();
      await until(byDay, 3000, 'todos under the days they were added');
      check('Added lists them under the days they were added, and is remembered', localStorage.getItem('codealong.todoSort') === 'added', heads().join(' | '));
      check('nothing is dragged into an order of yours', all('.ca-todorow[data-pin-index]').length === 0);
      check('each row’s age says when it was added', all('.ca-todorow .ca-seg-age').every(function (a) { return /^Added /.test(a.title); }));
      tool('Status').click();
      await until(function () { return localStorage.getItem('codealong.todoSort') === 'status'; }, 3000);
      var todoLabels = chips('Labels');
      check('the sidebar lists the todos’ labels', todoLabels.length > 0, todoLabels.map(function (c) { return c.textContent; }).join(', '));
      var allTodos = all('.ca-todorow').length;
      var name = q('span', todoLabels[todoLabels.length - 1]).textContent;
      todoLabels[todoLabels.length - 1].click();
      var hasLabel = function (r) { return all('.ca-labelchip', r).some(function (c) { return c.textContent === name; }); };
      await until(function () { return all('.ca-todorow').length < allTodos && all('.ca-todorow').every(hasLabel); }, 3000, 'the todos with the label picked');
      check('a label picked narrows the list to the todos that have it', true, name);
      var two = chips('Labels')[0];
      if (q('span', two).textContent !== name) {
        var before = all('.ca-todorow').length;
        two.click();
        await until(function () { return all('.ca-todorow').length > before; }, 3000, 'the todos with either label');
        check('two picked show the todos with either', picked('Labels').length === 2);
      }
      clear('Labels').click();
      await until(function () { return all('.ca-todorow').length === allTodos; }, 3000, 'every todo after Clear');

      // Sessions: narrowed by agent, named on the rail, kept by a saved view.
      nav('Sessions').click();
      await until(function () { return all('.ca-sessionlist .ca-lane.is-agent').length > 0 && chips('Agents').length > 0; }, 5000, 'the sessions and their agents');
      var agents = chips('Agents');
      var account = q('span', agents[agents.length - 1]).textContent;
      var sessions = all('.ca-sessionlist .ca-lane.is-agent').length;
      agents[agents.length - 1].click();
      // On one line a session's status names its account.
      await until(function () { var r = all('.ca-sessionlist .ca-lane.is-agent'); return r.length > 0 && r.length <= sessions && r.every(function (x) { return / · /.test(q('.ca-seg-glyph', x).title) && q('.ca-seg-glyph', x).title.split(' · ').pop() === account; }); }, 3000, 'the sessions on the agent picked');
      check('an agent picked narrows Sessions to its sessions', true, account);
      q('.ca-sidetoggle').click();
      var rail = await until(function () { return q('.ca-rail-btn[aria-label^="Filtered"]'); }, 3000, 'the folded rail’s filter button');
      check('the folded rail names it', rail.getAttribute('aria-label').indexOf(account) >= 0 && q('.ca-rail-badge', rail).textContent === '1', rail.getAttribute('aria-label'));
      rail.click();
      await until(function () { return q('.ca-side-save'); }, 3000, 'the sidebar again');
      q('.ca-side-save').click();
      var field = await until(function () { return q('.ca-side-name'); }, 3000, 'the saved view’s name');
      type(field, 'Agent check');
      key(field, { key: 'Enter' });
      await until(function () { return (localStorage.getItem('codealong.savedViews') || '').indexOf('Agent check') >= 0; }, 3000, 'the saved view');
      var view = JSON.parse(localStorage.getItem('codealong.savedViews')).filter(function (v) { return v.name === 'Agent check'; })[0];
      check('a saved view keeps the agent picked', !!view && view.facets && view.facets.agents && view.facets.agents[0] === account, JSON.stringify(view && view.facets));
      clear('Agents').click();
      await until(function () { return all('.ca-sessionlist .ca-lane.is-agent').length === sessions; }, 3000, 'every session after Clear');
      check('Clear shows every session again', true);

      // Archived sessions are narrowed too: an agent of one of them picked, they're all its.
      var fold = q('.ca-archived .ca-linkbtn');
      if (fold) {
        if (fold.textContent === 'Show') fold.click();
        await until(function () { return all('.ca-archived .ca-lane.is-agent').length > 0; }, 3000, 'the archived sessions');
        var accountOf = function (x) { return q('.ca-seg-glyph', x).title.split(' · ').pop(); };
        var archivedAll = all('.ca-archived .ca-lane.is-agent').map(accountOf);
        // Every agent offered, past "more" too; one with only archived sessions isn't.
        var more = all('.ca-side-more').filter(function (b) { return / more$/.test(b.textContent); })[0];
        if (more) more.click();
        var offered = chips('Agents').filter(function (c) { return archivedAll.indexOf(q('span', c).textContent) >= 0; });
        if (offered.length > 0) {
          var one = q('span', offered[offered.length - 1]).textContent;
          offered[offered.length - 1].click();
          await until(function () { return all('.ca-archived .ca-lane.is-agent').every(function (x) { return accountOf(x) === one; }); }, 3000, 'the archived sessions on the agent picked');
          check('archived sessions are narrowed to the agent picked too', all('.ca-archived .ca-lane.is-agent').length > 0, one + ': ' + all('.ca-archived .ca-lane.is-agent').length + ' of ' + archivedAll.length);
          clear('Agents').click();
        } else {
          note('no agent offered has an archived session: Archived not tried');
        }
      } else {
        note('no archived session: Archived not tried');
      }
      done();
    } catch (e) {
      stopped(e);
    }
  })();
})();
