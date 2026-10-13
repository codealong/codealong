// Checks a PR's Reviews tab (PrInspector.tsx, PrReviewers.tsx, prReviews.ts): its tabs are
// Description, Changes and Reviews, Description holding the author's words alone; Reviews has
// your review under its own heading, then everyone on the PR once, whoever asked for changes
// first, each with their verdict and what they wrote, what's still open first. A thread on the
// code says its file and line with the lines around it from the diff (a removed line's, numbered
// as the file was, for one on the old side), and how many replies follow; a resolved one is
// folded to a line; a click on a comment's head opens it on GitHub. A bot's long report shows cut
// short, without its HTML, until Show all, and Codealong's hidden mark never shows. The tab
// counts what's still open from others. A PR of yours someone asked for changes to opens on
// Reviews, without a review-of-yours section; one nobody has looked at says so.
//   scripts/preview.sh prs '' scripts/preview/pr-reviews-check.js
//   PREVIEW_SCHEME=dark scripts/preview.sh prs '' scripts/preview/pr-reviews-check.js
(function () {
  ['codealong.prPanelMax', 'codealong.prDetailsOpen'].forEach(function (k) { localStorage.removeItem(k); });
  var now = Date.now();
  var PATCH = ['@@ -1,4 +1,5 @@', ' fn a() {', '-    old();', '+    new();', '+    more();', ' }'].join('\n');
  var c = function (id, author, minutesAgo, more) {
    return Object.assign({ id: id, author: author, body: id + ' text', at: now - minutesAgo * 60e3, url: 'https://github.com/o/r/pull/7#' + id, path: null, line: null, outdated: false, replies: 0, resolved: false, mine: author === 'me' }, more || {});
  };
  var BOT = '<details><summary>Preview deployment</summary>\n\n' + 'Deployed to https://example.vercel.app with every page built. '.repeat(14) + '\n</details>';
  var RICH = {
    bodyHtml: '<p>Adds teams.</p>',
    mergeable: null,
    commits: 2,
    reviews: [
      { author: 'ken', state: 'APPROVED', at: now - 50 * 60e3, mine: false },
      { author: 'ana', state: 'CHANGES_REQUESTED', at: now - 40 * 60e3, mine: false },
      { author: 'me', state: 'COMMENTED', at: now - 30 * 60e3, mine: true },
    ],
    requested: ['gwen'],
    comments: [
      c('bot', 'vercel', 5, { body: BOT }),
      c('reply', 'kai', 8, { body: 'Fixed in the last commit.' }),
      c('mine', 'me', 30, { body: 'Looks close.\n<!-- codealong-review o/r#7 abc -->', review: 'COMMENTED' }),
      c('ana-text', 'ana', 40, { body: 'Please add a test for the empty case.', review: 'CHANGES_REQUESTED' }),
      c('ana-done', 'ana', 42, { path: 'src/b.ts', line: 3, resolved: true, replies: 1, body: 'Rename this?' }),
      c('ana-open', 'ana', 45, { path: 'src/a.ts', line: 2, replies: 2, body: 'This **panics** on an empty list.' }),
      c('ana-old', 'ana', 46, { path: 'src/a.ts', line: 2, before: true, body: 'Why drop this?' }),
      c('ken-1', 'ken', 51), c('ken-2', 'ken', 52), c('ken-3', 'ken', 53), c('ken-4', 'ken', 54),
    ],
    files: [{ path: 'src/a.ts', status: 'modified', additions: 2, deletions: 1, patch: PATCH }],
    moreFiles: false,
  };
  var EMPTY = { bodyHtml: '', mergeable: null, commits: 1, reviews: [], requested: [], comments: [], files: [], moreFiles: false };
  var picked = [];
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    if (cmd === 'pr_detail') return Promise.resolve(picked[2] && args.number === picked[2].number ? EMPTY : RICH);
    if (cmd === 'pr_draft') return Promise.resolve({ draft: null, submitted: null });
    // Jev says ana's review text asks for a change.
    if (cmd === 'pr_comment_asks') return Promise.resolve({ 'ana-text': 0.9 });
    var p = real(cmd, args, options);
    if (cmd !== 'get_board') return p;
    return p.then(function (board) {
      board.prs = (board.prs || []).map(function (x, i) {
        var base = { snoozedUntil: null, mergedAt: null, closedAt: null, autoReview: undefined, updatedAt: now - i * 1000 };
        // Someone else's, waiting on you; yours, with changes asked for; yours, with nobody on it.
        var as = [{ state: 'open', mine: false, author: 'kai', reviewRequested: true }, { state: 'changes', mine: true, author: 'me', reviewRequested: false }, { state: 'open', mine: true, author: 'me', reviewRequested: false }][i];
        if (!as) return Object.assign({}, x, { reviewRequested: false });
        var pr = Object.assign({}, x, base, as);
        picked[i] = pr;
        return pr;
      });
      return board;
    });
  };
  var press = function (el) {
    el.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, cancelable: true, button: 0, buttons: 1 }));
    el.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, cancelable: true, button: 0 }));
    el.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true, detail: 1 }));
  };
  var rowOf = function (pr) { return all('.ca-prrow').filter(function (r) { return (q('.ca-prrow-num', r) || {}).textContent === '#' + pr.number; })[0]; };
  var tab = function (label) { return all('.ca-prinsp .ca-tab').filter(function (t) { return t.firstChild && t.firstChild.textContent === label; })[0]; };
  var shownTab = function () { var t = q('.ca-prinsp .ca-tab.is-on'); return t && t.firstChild.textContent; };
  var tabCount = function (label) { var n = q('.ca-tab-count', tab(label)); return n ? n.textContent : null; };
  var hidden = function (el) { return !el || !!el.closest('[hidden]'); };
  var show = function (pr) {
    press(rowOf(pr));
    return until(function () { return q('.ca-insp-call') && q('.ca-insp-call').textContent === '#' + pr.number && tab('Reviews'); }, 5000, '#' + pr.number + '’s panel');
  };
  var person = function (who) { return all('.ca-prrev').filter(function (p) { return q('.ca-prrev-who', p).textContent === who; })[0]; };

  say('pr reviews check: waiting for the PRs');
  until(function () { return picked.length === 3 && rowOf(picked[2]); }, 20000, 'three PRs in the PRs view')
    .then(function () { return show(picked[0]); })
    .then(function () {
      var labels = all('.ca-prinsp .ca-tab').map(function (t) { return t.firstChild.textContent; });
      check('the tabs are Description, Changes and Reviews', labels.join(' | ') === 'Description | Changes | Reviews', labels.join(' | '));
      check('someone else’s PR with no review of the agent’s opens on Description', shownTab() === 'Description', shownTab());
      return until(function () { return tabCount('Reviews') === '3'; }, 5000, 'Reviews never counted what’s open');
    })
    .then(function () {
      var desc = q('.ca-prinsp [aria-label="Description"]').closest('[role="tabpanel"]');
      check('Description holds the description alone', /Adds teams\./.test(desc.textContent) && !q('.ca-prsaid', desc) && !/Comments ·/.test(desc.textContent), desc.textContent.slice(0, 120));
      check('the Reviews tab says what it counts', /3 still open/.test(tab('Reviews').getAttribute('title') || ''), tab('Reviews').getAttribute('title'));
      press(tab('Reviews'));
      return until(function () { return shownTab() === 'Reviews' && all('.ca-prrev').length > 0; }, 3000, 'the Reviews tab never listed anyone');
    })
    .then(function () {
      var yours = q('.ca-prinsp [aria-label="Your review"]');
      check('your review comes first, under its own heading', !!yours && !hidden(yours) && q('.ca-label', yours).textContent === 'Your review' && /No draft review of yours/.test(yours.textContent), yours && yours.textContent.slice(0, 80));
      var order = all('.ca-prrev').map(function (p) { return q('.ca-prrev-who', p).textContent; });
      check('everyone once, whoever asked for changes first and whoever only wrote last', order.join(' ') === 'ana you ken gwen vercel kai', order.join(' '));
      var heading = q('.ca-prrevs .ca-label').textContent;
      check('the list says what’s still open', heading === 'Reviews and comments · 3 open', heading);

      var ana = person('ana');
      check('a verdict with its words, and how much of it is open', q('.ca-prrev-verdict', ana).textContent === 'Requested changes' && /3 open/.test(q('.ca-prrev-head', ana).textContent), q('.ca-prrev-head', ana).textContent);
      var said = all('.ca-prsaid', ana);
      // Three open: all of them show, the resolved one behind Show more.
      check('what’s open first: the text Jev says asks for a change, then the open threads', said.length === 3 && /Asks for a change/.test(said[0].textContent) && /a\.ts/.test(said[1].textContent) && /a\.ts/.test(said[2].textContent) && /Show 1 more/.test(ana.textContent), said.map(function (s) { return s.textContent.slice(0, 30); }).join(' | '));
      check('the open ones are marked', said.every(function (x) { return x.classList.contains('is-open'); }), said.map(function (s) { return s.className; }).join(' | '));
      var gone = q('.ca-agentrev-code .is-at', said[2]);
      check('a thread on a line the PR removes marks that line, numbered as the file was', /line 2 before/.test(q('.ca-prsaid-head', said[2]).textContent) && !!gone && gone.classList.contains('is-del') && /old\(\);/.test(gone.textContent), (q('.ca-agentrev-code', said[2]) || {}).textContent);
      var thread = said[1];
      check('a thread says its file, line and replies', q('.ca-agentrev-file', thread).textContent === 'a.ts' && q('.ca-agentrev-dir', thread).textContent === 'src' && /line 2/.test(thread.textContent) && /2 replies/.test(thread.textContent), q('.ca-prsaid-head', thread).textContent);
      var at = q('.ca-agentrev-code .is-at', thread);
      check('with the lines around it from the diff, its own marked', !!at && /new\(\);/.test(at.textContent), (q('.ca-agentrev-code', thread) || {}).textContent);
      check('its words drawn as Markdown', !!q('.ca-prsaid-text strong', thread), thread.innerHTML.slice(0, 200));
      press(q('.ca-prsaid-head', thread));
      var opened = last('open_url');
      check('a click on its head opens it on GitHub', !!opened && opened.url === 'https://github.com/o/r/pull/7#ana-open', JSON.stringify(opened));
      press(q('.ca-more-btn', ana));
      return until(function () { return all('.ca-prsaid', person('ana')).length === 4; }, 3000, 'ana’s Show more never showed the resolved thread');
    })
    .then(function () {
      var done = all('.ca-prsaid', person('ana'))[3];
      check('a resolved thread comes last, folded to a line with no lines around it', done.classList.contains('is-resolved') && !done.classList.contains('is-open') && !q('.ca-agentrev-code', done) && !!q('.ca-prsaid-short', done) && /Resolved/.test(done.textContent) && /1 reply/.test(done.textContent), done.textContent);

      var me = person('you');
      check('your own review is yours, its hidden mark not shown', !!me && q('.ca-prrev-verdict', me).textContent === 'Commented' && /Looks close\./.test(me.textContent) && !/codealong-review/.test(me.textContent), me && me.textContent);

      var ken = person('ken');
      check('someone’s comments show three at first', all('.ca-prsaid', ken).length === 3 && /Show 1 more/.test(ken.textContent), all('.ca-prsaid', ken).length + ' ' + ken.textContent.slice(-20));
      press(q('.ca-more-btn', ken));
      return until(function () { return all('.ca-prsaid', person('ken')).length === 4; }, 3000, 'Show more never showed the rest');
    })
    .then(function () {
      var gwen = person('gwen');
      check('someone asked who hasn’t reviewed says so', /Review requested/.test(gwen.textContent) && !q('.ca-prsaid', gwen), gwen.textContent);
      var bot = person('vercel');
      var short = q('.ca-prsaid-short', bot);
      check('a long report shows cut short, without its HTML', !!short && /^Preview deployment Deployed to/.test(short.textContent) && !/<details>|<summary>/.test(bot.textContent), short && short.textContent.slice(0, 80));
      press(q('.ca-prsaid-short .ca-linkbtn', bot));
      return until(function () { return !!q('.ca-prsaid-text', person('vercel')); }, 3000, 'Show all never showed the whole report');
    })
    .then(function () {
      check('Show all draws the whole of it', !/<details>/.test(q('.ca-prsaid-text', person('vercel')).textContent));
      check('the PR’s author is said to be', /author/.test(q('.ca-prrev-head', person('kai')).textContent), q('.ca-prrev-head', person('kai')).textContent);
      return show(picked[1]);
    })
    .then(function () {
      return until(function () { return shownTab() === 'Reviews' && all('.ca-prrev').length > 0; }, 5000, 'your PR with changes asked for never showed its reviews');
    })
    .then(function () {
      check('a PR of yours someone asked for changes to opens on Reviews', shownTab() === 'Reviews', shownTab());
      check('with no review of yours to show, only everyone’s', !q('.ca-prinsp [aria-label="Your review"]') && !!q('.ca-prrevs'), (q('.ca-prinsp [aria-label="Your review"]') || {}).textContent);
      return show(picked[2]);
    })
    .then(function () {
      press(tab('Reviews'));
      return until(function () { var y = q('.ca-prinsp [aria-label="Your review"]'); return shownTab() === 'Reviews' && y && /Nobody has reviewed it/.test(y.textContent); }, 5000, 'your PR nobody looked at never said so');
    })
    .then(function () {
      check('a PR of yours nobody has looked at says so, with no heading and no count', !q('.ca-prrevs') && !q('.ca-prinsp [aria-label="Your review"] .ca-label') && tabCount('Reviews') === null, tabCount('Reviews'));
    })
    .then(done)
    .catch(stopped);
})();
