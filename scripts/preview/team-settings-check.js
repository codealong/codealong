// Checks a repo's settings (repoSettings.ts, teamfile.rs), with a repo that has team settings
// (team-init.js). In the worktrees view: a worktree's menu has Automation for its repo, which
// opens Settings › Automation with that repo picked. With the team's repo picked, the team's
// settings show over the fields: where they're read from, approvals never submitted, and what
// they suggest that you don't have, with Use them (take_team_settings). Each field says where its
// value comes from: yours for the repo with Reset, the team's, or as everywhere; the team's words
// for reviews are there to read. A field changed saves your table for the repo, not [reviews];
// Reset takes it out. Everywhere has no source tags and has the usage limits. In the PRs view: the
// team's PR, with no review yet, has the team's suggestions in its Review tab in place of what an
// empty one says; Use them takes them, and the cross dismisses them for good (localStorage), the
// empty tab's words back.
//   PREVIEW_INIT=scripts/preview/team-init.js PREVIEW_WAIT=60000 scripts/preview.sh worktrees '' scripts/preview/team-settings-check.js
//   PREVIEW_INIT=scripts/preview/team-init.js PREVIEW_WAIT=60000 scripts/preview.sh prs '' scripts/preview/team-settings-check.js
(function () {
  localStorage.setItem('codealong.project', 'All projects');
  localStorage.removeItem('codealong.teamOffers.dismissed');
  var view = localStorage.getItem('codealong.view');
  var press = function (el) {
    el.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, cancelable: true, button: 0, buttons: 1 }));
    el.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, cancelable: true, button: 0 }));
    el.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true, detail: 1 }));
  };
  var button = function (label, within) { return all('button', within).filter(function (b) { return b.textContent.trim() === label; })[0]; };
  var nav = function (label) { return all('.ca-settings-nav button').filter(function (b) { return b.textContent.trim() === label; })[0]; };
  var field = function (words) { return all('.ca-settings-pane .ca-field').filter(function (f) { var s = f.querySelector(':scope > span'); return s && words.test(s.textContent); })[0]; };
  var from = function (words) { var f = field(words); var t = f && q('.ca-settings-from', f); return t ? t.textContent : null; };
  var picker = function () { return q('.ca-settings-scope select'); };
  var pick = function (select, value) {
    Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype, 'value').set.call(select, value);
    select.dispatchEvent(new Event('change', { bubbles: true }));
  };
  var lastSave = function () { var c = calls('save_config'); return c.length ? c[c.length - 1][1].config : null; };

  (async function () {
    try {
      await until(function () { return !!window.__TEAM__; }, 20000, 'the board never came');
      var team = window.__TEAM__;
      if (!team.repo) throw new Error('no repo on the board to give team settings');
      var key = 'github.com/' + team.repo;

      if (view === 'prs') {
        if (!team.pr) throw new Error('no open PR without a review that isn’t yours, to show the team’s offer on');
        var row = function () { return all('.ca-prrow').filter(function (r) { return (q('.ca-prrow-num', r) || {}).textContent === '#' + team.number || (q('.ca-prrow-num', r) || {}).textContent === String(team.number); })[0]; };
        await until(row, 10000, 'the team’s PR #' + team.number + ' isn’t listed');
        press(row());
        var tab = function (label) { return all('.ca-prinsp .ca-tab').filter(function (t) { return t.firstChild && t.firstChild.textContent === label; })[0]; };
        await until(function () { return !!tab('Review'); }, 5000, 'the PR panel has no Review tab');
        press(tab('Review'));
        await until(function () { return !!q('.ca-agentrev-offer'); }, 5000, 'the Review tab has no offer from the team');
        var offer = q('.ca-agentrev-offer');
        check('the Review tab says what the team suggests', /suggests: an agent reviews each pull request that waits on your review; an agent on another model reviews the pull requests your sessions open; the session that opened a pull request told what happens on it\./.test(offer.textContent), offer.textContent);
        check('in place of what an empty one says', !q('.ca-agentrev .ca-empty'), '');
        press(button('Use them', offer));
        await until(function () { return calls('take_team_settings').length > 0; }, 3000, 'Use them sent nothing');
        var took = calls('take_team_settings')[0][1];
        check('Use them takes the suggestions of the PR’s base branch', took.repo === team.repo && took.branch === team.base, JSON.stringify(took));
        press(q('.ca-agentrev-offer-x'));
        await until(function () { return !q('.ca-agentrev-offer'); }, 3000, 'the cross left the offer up');
        var kept = JSON.parse(localStorage.getItem('codealong.teamOffers.dismissed') || '[]');
        check('dismissed, it’s remembered', kept.length === 1 && kept[0].indexOf(team.repo + '@' + team.base + ':') === 0, JSON.stringify(kept));
        check('and the empty tab says what it did', !!q('.ca-agentrev .ca-empty'), '');
        return done();
      }

      // A worktree's menu opens Settings on its repo.
      await until(function () { return !!q('.ca-board .ca-lane'); }, 10000, 'no worktree on the board');
      var lane = all('.ca-board .ca-lane').filter(function (l) { rightClick(l); return menuLabels().some(function (x) { return /^Automation for /.test(x); }); })[0];
      check('a worktree’s menu has Automation for its repo', !!lane, menuLabels().join(' | '));
      if (lane) {
        rightClick(lane);
        var label = menuLabels().filter(function (x) { return /^Automation for /.test(x); })[0];
        var repo = label.replace(/^Automation for /, '').replace(/…$/, '');
        menuItem(label)();
        await until(function () { return !!picker(); }, 3000, 'Automation for… didn’t open Settings › Automation');
        check('which opens Settings › Automation with that repo picked', picker().value === repo, picker().value + ' / ' + repo);
        press(button('Done'));
        await until(function () { return !picker(); }, 3000, 'Settings didn’t close');
      }

      q('.ca-side-config').click();
      await until(function () { return !!nav('Automation'); }, 3000, 'no Automation in Settings');
      nav('Automation').click();
      await until(picker, 3000, 'no repo picker in Settings › Automation');
      check('it opens on Everywhere, with no source tags and the usage limits', picker().value === '' && !q('.ca-settings-from') && !!field(/When a session hits its usage limit/), picker().value);
      var names = all('option', picker()).map(function (o) { return o.textContent; });
      check('the picker lists the team’s repo as having team settings and yours', names.indexOf(team.repo + ' · team settings · yours') >= 0, names.join(' | '));

      pick(picker(), team.repo);
      await until(function () { return !!q('.ca-settings-team'); }, 3000, 'no team settings for the repo');
      var head = q('.ca-settings-team-head').textContent;
      check('the team’s settings say where they’re read from', head.indexOf('.codealong/config.toml on ' + team.base + ' at 1a2b3c4') >= 0, head);
      check('and that approvals are never submitted', /never has an approval submitted for anyone/.test(q('.ca-settings-team').textContent), q('.ca-settings-team').textContent);
      var offers = all('.ca-settings-team-offer li').map(function (l) { return l.textContent; });
      check('and what they suggest that you don’t have', offers.length === 3 && offers[0] === 'An agent reviews each pull request that waits on your review', offers.join(' | '));
      press(button('Use them', q('.ca-settings-team')));
      await until(function () { return calls('take_team_settings').length > 0; }, 3000, 'Use them sent nothing');
      check('Use them takes them', JSON.stringify(calls('take_team_settings')[0][1]) === JSON.stringify({ repo: team.repo, branch: team.base }), JSON.stringify(calls('take_team_settings')[0][1]));

      check('a value set for the repo says it’s yours here', from(/^Where each review goes/) === 'Yours here' && !!button('Reset', field(/^Where each review goes/)), String(from(/^Where each review goes/)));
      check('one the team sets says so', from(/^What a nudge tells it about/) === 'Team', String(from(/^What a nudge tells it about/)));
      var nudge = q('select', field(/^What a nudge tells it about/));
      check('and shows the team’s value', nudge.value === 'feedback-and-checks', nudge.value);
      check('the rest are as everywhere', from(/^The reviewer/) === 'Same as everywhere', String(from(/^The reviewer/)));
      check('the team’s words for reviews are there to read', /Check that every migration runs both ways/.test((q('.ca-settings-teamwords') || {}).textContent || ''), '');
      check('the usage limits aren’t per repo', !field(/When a session hits its usage limit/), '');

      var before = calls('save_config').length;
      pick(q('select', field(/^Where each review goes/)), 'comment');
      await until(function () { return calls('save_config').length > before; }, 3000, 'changing where reviews go saved nothing');
      var cfg = lastSave();
      check('a change saves your table for the repo', cfg.repos && cfg.repos[key] && cfg.repos[key].reviews.post === 'comment', JSON.stringify(cfg.repos && cfg.repos[key]));
      var original = (await fetch('./config.json').then(function (r) { return r.json(); })).config;
      check('not [reviews]', JSON.stringify(cfg.reviews || null) === JSON.stringify(original.reviews || null), JSON.stringify(cfg.reviews) + ' / ' + JSON.stringify(original.reviews));

      before = calls('save_config').length;
      press(button('Reset', field(/^Where each review goes/)));
      await until(function () { return calls('save_config').length > before; }, 3000, 'Reset saved nothing');
      cfg = lastSave();
      check('Reset takes it out, and the repo’s table with it once empty', !cfg.repos || !cfg.repos[key], JSON.stringify(cfg.repos));

      pick(picker(), '');
      await until(function () { return !q('.ca-settings-team'); }, 3000, 'Everywhere still shows the team');
      check('Everywhere again has no source tags', !q('.ca-settings-from'), '');
    } catch (e) {
      return stopped(e);
    }
    done();
  })();
})();
