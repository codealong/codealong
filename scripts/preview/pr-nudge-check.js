// Checks the switch that has a PR's primary session told of what happens on it (PrLink.tsx,
// nudges.rs), on the card of a session's PR chip, and the setting that turns it on for every PR of
// yours (Settings › Automation, `[pull_requests] nudge_by_default`). The first session a PR lists has
// that PR made an open one with the nudge off, that session its primary, and another session listed
// on it too: hovering its chip in the session's header shows the card with the primary session
// first, tagged with why it's the one, and the switch off, naming it; a click turns it on for that PR alone (set_pr_nudge
// with its key) and the card says so at once; a board that says the same keeps it on, and one that
// says it's on by the setting says that; with its primary session stopped, it says what the board
// says becomes of the news (`prStopped`): resumed for what asks for work, not beside another of the
// PR's sessions that runs, or nothing; a second click turns it off. A merged PR's card
// has no switch. Then the setting, off by default, saves `[pull_requests]` as config.toml has it,
// and resuming a stopped primary, on by default, saves `[sessions]`.
//   PREVIEW_WAIT=60000 scripts/preview.sh sessions '' scripts/preview/pr-nudge-check.js
(function () {
  localStorage.setItem('codealong.project', 'All projects');
  var picked = null;
  var other = null;
  var board = null;
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    var p = real(cmd, args, options);
    // Settings as a config.toml without [pull_requests] has them, whatever this Mac's says.
    if (cmd === 'get_config') {
      return p.then(function (view) {
        var config = Object.assign({}, view.config);
        delete config.pull_requests;
        return Object.assign({}, view, { config: config });
      });
    }
    if (cmd !== 'get_board') return p;
    return p.then(function (b) {
      b.lanes.forEach(function (l) {
        l.sessions.forEach(function (s) {
          if (picked || s.visitor || s.archived || l.hidden || l.device) return;
          var pr = b.prs.find(function (x) { return x.sessions.some(function (y) { return y.key === s.key; }); });
          if (pr) picked = { key: s.key, lane: l.id, title: s.title, pr: pr };
        });
      });
      if (picked) {
        // Another session listed on it, the most recent that isn't the primary, so the primary
        // coming first is the card's doing.
        b.lanes.forEach(function (l) {
          l.sessions.forEach(function (s) {
            if (s.key === picked.key || s.visitor || s.archived || l.hidden || l.device) return;
            if (!other || s.lastActivity > other.lastActivity) other = { key: s.key, lane: l.id, lastActivity: s.lastActivity };
          });
        });
        b.prs.forEach(function (x) {
          if (x.key !== picked.pr.key) return;
          Object.assign(x, { state: 'open', mergedAt: null, closedAt: null, nudge: false, nudgeByDefault: false, primary: picked.key, primaryBy: 'picked' });
          if (other && !x.sessions.some(function (y) { return y.key === other.key; })) x.sessions.push({ key: other.key, lane: other.lane });
        });
        // The session's header shows its worktree's PR and the PRs that list it: this one either way.
        b.lanes.forEach(function (l) {
          if (l.id === picked.lane && l.pr && l.pr.number === picked.pr.number) Object.assign(l.pr, { state: 'open', mergedAt: null, closedAt: null });
        });
      }
      // What a nudge tells of as a config.toml without [pull_requests] has it, as Settings does:
      // the board's is this Mac's, which the card says.
      b.nudgeAbout = 'everything';
      Object.keys(b.repos || {}).forEach(function (k) { delete b.repos[k].pull_requests; });
      (b.teamFiles || []).forEach(function (t) { if (t.settings) t.settings.nudgeAbout = null; });
      // The check's own copy to change and push: the page keeps the parts of a push that equal
      // what it has, so changing the board it holds would look like no change at all.
      board = JSON.parse(JSON.stringify(b));
      return b;
    });
  };

  var row = function () { return picked && document.getElementById('row-' + picked.lane + '::' + picked.key); };
  var title = function () { var t = document.querySelector('.ca-sessinsp .ca-sessinsp-title'); return t ? t.textContent : null; };
  var chip = function () {
    return Array.prototype.filter.call(document.querySelectorAll('.ca-sessinsp .ca-prchipcard'), function (c) { return new RegExp('#' + picked.pr.number + '(\\D|$)').test(c.textContent); })[0];
  };
  var card = function () { return document.querySelector('.ca-prhover'); };
  var toggle = function () { return card() && card().querySelector('.ca-prhover-nudge'); };
  var on = function () { return toggle().getAttribute('aria-checked') === 'true'; };
  var says = function () { return toggle().parentNode.querySelector('.ca-hovercard-meta').textContent; };
  var hover = function (el) { el.dispatchEvent(new MouseEvent('mouseover', { bubbles: true, relatedTarget: document.body })); };
  var leave = function (el) { el.dispatchEvent(new MouseEvent('mouseout', { bubbles: true, relatedTarget: document.body })); };
  var push = function (change) {
    board.prs.forEach(function (x) { if (x.key === picked.pr.key) Object.assign(x, change); });
    board.generatedAt = Date.now();
    window.__emit('board', JSON.parse(JSON.stringify(board)));
  };
  var nav = function (label) { return all('.ca-settings-nav button').filter(function (b) { return b.textContent.trim() === label; })[0]; };
  var box = function (words) { return all('.ca-settings-check').filter(function (l) { return words.test(l.textContent); })[0]; };

  (async function () {
    try {
      say('pr nudge check: waiting for the list');
      await until(function () { return row(); }, 20000).catch(function () { throw new Error('no session on the board that a PR lists'); });
      row().dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
      row().click();
      await until(function () { return title() === picked.title && chip(); }, 10000, "the clicked session's panel, with its PR's chip");
      hover(chip());
      await until(toggle, 3000, "the card of the session's PR chip, with its switch");

      check('the card names the PR', card().querySelector('.ca-prhover-ref').textContent === picked.pr.repoSlug + '#' + picked.pr.number);
      var rows = all('.ca-psess', card());
      if (other) {
        check('its primary session comes first, tagged', rows.length >= 2 && rows[0].querySelector('.ca-psess-title').textContent === picked.title && !!rows[0].querySelector('.ca-psess-primary') && !rows[1].querySelector('.ca-psess-primary'), rows.map(function (r) { return r.textContent; }).join(' | '));
        var tag = rows.length > 0 && rows[0].querySelector('.ca-psess-primary');
        check('its Primary tag says why it is the one', !!tag && /^The PR’s primary session: .* It was picked as the primary one\. The PR’s menu picks another/.test(tag.title), tag ? tag.title : 'no tag');
        check('the switch says only the primary is nudged', /^Nudge its primary session on updates/.test(toggle().textContent), toggle().textContent);
      } else {
        note('no second session to list on the PR');
      }
      var named = '“' + picked.title + '”';
      check('its nudge is off, as every PR starts, and says who turning it on tells', !on() && says() === 'Off for this PR. Turned on, ' + named + ' is told of reviews, comments, an approval, a merge, failing checks and conflicts.', says());
      check('nothing is sent until it is clicked', calls('set_pr_nudge').length === 0);
      var r = card().getBoundingClientRect();
      check('the card is inside the window', r.left >= 0 && r.top >= 0 && r.right <= window.innerWidth && r.bottom <= window.innerHeight, [r.left, r.top, r.right, r.bottom].map(Math.round).join(','));
      toggle().click();
      await wait(100);

      var sent = calls('set_pr_nudge');
      var a = sent.length && sent[sent.length - 1][1];
      check('a click turns it on for that PR', sent.length === 1 && a.key === picked.pr.key && a.on === true, JSON.stringify(a));
      var onWords = new RegExp('^On for this PR\\. ' + named.replace(/[.*+?^${}()|[\]\\]/g, '\\$&') + ', its primary session, (is sent a message|isn’t running here)');
      check('and the card stays up and says so at once, naming the session told', !!toggle() && on() && onWords.test(says()), toggle() && says());
      push({ nudge: true });
      await wait(200);
      check('the board that says so keeps it on', !!toggle() && on());
      push({ state: 'approved' });
      await wait(200);
      check('approved, it still says so', onWords.test(says()), says());
      push({ nudgeByDefault: true });
      await wait(200);
      check('on by the setting, it says that', /^On for every PR of yours, by Settings\. /.test(says()), says());
      // Its primary stopped: what becomes of the news, as the engine works it out for the board
      // (nudges.rs `stopped_primaries`).
      var running = function (live) {
        board.lanes.forEach(function (l) { l.sessions.forEach(function (s) { if (s.key === picked.key) s.live = live; }); });
      };
      var stopped = function (what) {
        board.prStopped = {};
        if (what) board.prStopped[picked.pr.key] = what;
        push({});
      };
      running(false);
      stopped('resumes');
      await wait(200);
      check('its primary stopped, it says it’s resumed for what asks for work', /isn’t running here, so it’s resumed out of sight to be told when someone else asks for changes or comments and when its checks fail or its branch conflicts with its base, never for an approval or a merge\.$/.test(says()), says());
      stopped('beside');
      await wait(200);
      check('with another of its sessions running, it says it isn’t resumed beside it', /isn’t running here, and another of the PR’s sessions is, so it isn’t resumed beside that one: nobody is told until it runs\.$/.test(says()), says());
      stopped('never');
      await wait(200);
      check('when it can’t be, nobody is told until it runs', /isn’t running here, so nobody is told until it is\.$/.test(says()), says());
      running(true);
      stopped(null);
      await wait(200);
      toggle().click();
      await wait(100);

      sent = calls('set_pr_nudge');
      a = sent[sent.length - 1][1];
      check('a second click turns it off for that PR', sent.length === 2 && a.key === picked.pr.key && a.on === false && !on(), JSON.stringify(a));
      push({ nudge: false, nudgeByDefault: false, state: 'merged', mergedAt: Date.now(), closedAt: Date.now() });
      await wait(200);
      check('a merged PR has no switch', !card() || !toggle());
      if (card()) leave(card());

      q('.ca-side-config').click();
      await until(function () { return !!nav('Automation'); }, 3000, 'no Automation in Settings');
      nav('Automation').click();
      var setting = /Nudge the session that opened a pull request/;
      await until(function () { return !!box(setting); }, 5000, 'no nudge setting in Settings › Automation');
      var input = box(setting).querySelector('input');
      check('Settings › Automation has the setting, off by default', !input.checked, String(input.checked));
      input.click();
      await until(function () { return calls('save_config').length > 0; }, 3000, 'turning it on saved nothing');
      var cfg = calls('save_config')[0][1].config;
      check('turned on, it saves [pull_requests] as config.toml has it', cfg && cfg.pull_requests && cfg.pull_requests.nudge_by_default === true, JSON.stringify(cfg && cfg.pull_requests));
      // What a nudge tells of: everything by default, else only feedback, with checks or without.
      var aboutField = function () { return all('.ca-field').filter(function (f) { return ((q('span', f) || {}).textContent || '') === 'What a nudge tells it about'; })[0]; };
      await until(function () { return !!aboutField(); }, 3000, 'no choice of what a nudge tells of');
      var abouts = all('option', aboutField()).map(function (o) { return o.textContent; });
      check('a nudge tells of everything by default, else only comments and requests for changes, with failing checks and conflicts or without', abouts.join(' | ') === 'Everything that happens on it | Only comments and requests for changes | Comments, requests for changes, checks and conflicts' && q('select', aboutField()).value === 'everything', abouts.join(' | ') + ' / ' + q('select', aboutField()).value);
      var aboutSaves = calls('save_config').length;
      var aboutSelect = q('select', aboutField());
      aboutSelect.value = 'feedback';
      aboutSelect.dispatchEvent(new Event('change', { bubbles: true }));
      await until(function () { return calls('save_config').length > aboutSaves; }, 3000, 'picking what a nudge tells of saved nothing');
      var prsCfg = calls('save_config')[calls('save_config').length - 1][1].config.pull_requests;
      check('picked, it saves [pull_requests] nudge_about beside nudging by default, and says what it does', prsCfg.nudge_about === 'feedback' && prsCfg.nudge_by_default === true && /^Only when someone else comments on it, asks for changes/.test(q('small', aboutField()).textContent), JSON.stringify(prsCfg) + ' / ' + q('small', aboutField()).textContent);
      // Resuming a primary that stopped: on unless turned off, saved under [sessions].
      var resume = /Resume a stopped session for its pull request’s news/;
      await until(function () { return !!box(resume); }, 3000, 'no setting for resuming a stopped session for its pull request’s news');
      var resumeInput = box(resume).querySelector('input');
      check('Settings › Automation has resuming a stopped primary, on by default', resumeInput.checked, String(resumeInput.checked));
      var resumeSaves = calls('save_config').length;
      resumeInput.click();
      await until(function () { return calls('save_config').length > resumeSaves; }, 3000, 'turning it off saved nothing');
      var sessionsCfg = calls('save_config')[calls('save_config').length - 1][1].config.sessions;
      check('turned off, it saves [sessions] resume_for_pr_news', sessionsCfg && sessionsCfg.resume_for_pr_news === false, JSON.stringify(sessionsCfg));
    } catch (e) {
      return stopped(e);
    }
    done();
  })();
})();
