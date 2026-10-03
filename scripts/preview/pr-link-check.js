// Checks PR links in a session's conversation (PrLink.tsx). The stub's reply for a session a PR
// lists links that PR bare and, as a Markdown link, an older one the board doesn't list. The
// bare one reads `name#N` with the PR's icon; hovering it shows a card with the PR's title and
// the session; the older one's card asks GitHub (pr_card) once and shows its answer, and its
// link then has an icon too; a right-click has the PR's menu; Show in Codealong opens the PR in
// the PRs view. The PR is made a merged one, whatever it is today: the PRs view lists those under
// Merged & closed, which is folded until Show in Codealong opens it.
//   PREVIEW_WAIT=60000 scripts/preview.sh sessions '' scripts/preview/pr-link-check.js
(function () {
  var texts = function (items) { return items.map(function (i) { return i.item === 'Separator' ? '-' : i.text; }); };

  localStorage.setItem('codealong.project', 'All projects');
  // The first session, in its own worktree, that a PR lists. Its reply is its excerpt with the
  // stub's two links after it, so not one whose excerpt has a PR link of its own.
  var picked = null;
  var linksPr = function (text) { return /github\.com\/[\w.-]+\/[\w.-]+\/pull\/\d+/i.test(text || ''); };
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    var p = real(cmd, args, options);
    if (cmd !== 'get_board') return p;
    return p.then(function (board) {
      board.lanes.forEach(function (l) {
        l.sessions.forEach(function (s) {
          if (picked || s.visitor || s.archived || l.hidden || linksPr(s.excerpt)) return;
          var pr = board.prs.find(function (x) { return x.sessions.some(function (y) { return y.key === s.key; }); });
          if (pr) picked = { key: s.key, lane: l.id, title: s.title, pr: pr };
        });
      });
      if (picked) {
        board.prs.forEach(function (x) {
          if (x.key === picked.pr.key) Object.assign(x, { state: 'merged', mergedAt: x.mergedAt || x.updatedAt, closedAt: x.closedAt || x.updatedAt });
        });
        // Not pinned, or it would lead the view whatever its state.
        board.pinnedPrs = (board.pinnedPrs || []).filter(function (k) { return k !== picked.pr.key; });
      }
      return board;
    });
  };

  var row = function () { return picked && document.getElementById('row-' + picked.lane + '::' + picked.key); };
  var title = function () { var t = document.querySelector('.ca-sessinsp .ca-sessinsp-title'); return t ? t.textContent : null; };
  var links = function () { return Array.prototype.slice.call(document.querySelectorAll('.ca-sessinsp .ca-thread-agent a.ca-prlink')); };
  var card = function () { return document.querySelector('.ca-prhover'); };
  var hover = function (el) { el.dispatchEvent(new MouseEvent('mouseover', { bubbles: true, relatedTarget: document.body })); };
  var leave = function (el) { el.dispatchEvent(new MouseEvent('mouseout', { bubbles: true, relatedTarget: document.body })); };

  say('pr link check: waiting for the list');
  until(function () { return row(); }, 20000)
    .catch(function () { throw new Error('no session on the board that a PR lists'); })
    .then(function () {
      row().dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
      row().click();
      // With nothing clicked the panel shows the list's top row, which can be another session a
      // PR lists, with links of its own: the links read are the clicked session's, once its
      // panel shows (a panel is the session's own, so its conversation starts empty).
      return until(function () { return title() === picked.title && links().length === 2; }, 10000, 'the clicked session’s panel, with its two links');
    })
    .then(function () {
      var bare = links()[0];
      var name = picked.pr.repoSlug.split('/')[1];
      check('a bare PR link reads name#N', bare.textContent === name + '#' + picked.pr.number, bare.textContent);
      check('it has the PR state icon', !!bare.querySelector('.ca-prlink-icon.ca-pr--' + picked.pr.state));
      check('a written PR link keeps its words, with no icon yet', links()[1].textContent === 'the earlier fix' && !links()[1].querySelector('.ca-prlink-icon'));
      hover(bare);
      check('no card at once', !card());
      return until(card, 3000);
    })
    .then(function () {
      var c = card();
      check('the card names the PR', c.querySelector('.ca-prhover-ref').textContent === picked.pr.repoSlug + '#' + picked.pr.number);
      check('with its title', c.querySelector('.ca-prhover-title').textContent === picked.pr.title);
      var titles = Array.prototype.map.call(c.querySelectorAll('.ca-psess-title'), function (t) { return t.textContent; });
      check('and the session that worked on it', titles.indexOf(picked.title) >= 0, titles.join(' | '));
      check('the board has it, so GitHub is not asked', calls('pr_card').length === 0);
      var r = c.getBoundingClientRect();
      check('inside the window', r.left >= 0 && r.top >= 0 && r.right <= window.innerWidth && r.bottom <= window.innerHeight, [r.left, r.top, r.right, r.bottom].map(Math.round).join(','));
      // Moving into the card keeps it up. React takes both the leave and the enter from the
      // link's mouseout.
      links()[0].dispatchEvent(new MouseEvent('mouseout', { bubbles: true, relatedTarget: c }));
      return wait(400).then(function () {
        check('the pointer in the card keeps it up', !!card());
        leave(card());
        return until(function () { return !card(); }, 2000);
      });
    })
    .then(function () {
      check('leaving it closes it', true);
      hover(links()[1]);
      return until(function () { return card() && card().querySelector('.ca-prhover-title'); }, 3000);
    })
    .then(function () {
      var asked = calls('pr_card');
      var num = Number(links()[1].getAttribute('href').split('/').pop());
      check('an unlisted PR is asked of GitHub once', asked.length === 1 && asked[0][1].number === num, JSON.stringify(asked.map(function (a) { return a[1]; })));
      check('and its card shows the answer', card().querySelector('.ca-prhover-title').textContent === 'An older PR, as GitHub has it');
      check('with no Show in Codealong', !Array.prototype.some.call(card().querySelectorAll('button'), function (b) { return /Show in Codealong/.test(b.textContent); }));
      check('its link now has an icon', !!links()[1].querySelector('.ca-prlink-icon.ca-pr--merged'));
      leave(links()[1]);
      return until(function () { return !card(); }, 2000);
    })
    .then(function () {
      hover(links()[1]);
      return until(card, 3000).then(function () {
        check('asked only once', calls('pr_card').length === 1);
        document.body.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
        return until(function () { return !card(); }, 1000);
      });
    })
    .then(function () {
      check('a key closes it', true);
      rightClick(links()[0]);
      return until(function () { return lastMenu().length > 0; }, 2000);
    })
    .then(function () {
      var t = texts(lastMenu());
      check("a right-click has the PR's menu", t.indexOf('Open on GitHub') >= 0 && t.indexOf('Copy link') >= 0, t.join(' | '));
      hover(links()[0]);
      return until(card, 3000);
    })
    .then(function () {
      var show = Array.prototype.filter.call(card().querySelectorAll('button'), function (b) { return /Show in Codealong/.test(b.textContent); })[0];
      check('a listed PR has Show in Codealong', !!show);
      show.click();
      return until(function () { return document.querySelector('.ca-prinsp'); }, 3000);
    })
    .then(function () {
      var shown = document.querySelector('.ca-prinsp .ca-insp-call').textContent;
      check('which shows it in the PRs view', shown === '#' + picked.pr.number, shown + ', not #' + picked.pr.number);
      check('and closes the card', !card());
    })
    .then(done)
    .catch(stopped);
})();
