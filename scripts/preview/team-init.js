// A repo with team settings, as Codealong reads them from a repo's .codealong/config.toml
// (teamfile.rs), since this Mac's repos rarely have one: the repo of the first open PR that isn't
// yours and has no review yet (else the first repo on the board), read from that PR's base branch,
// with words for every review, approvals never submitted, nudges kept to feedback and checks, and
// three suggestions you don't have yet. Your own table for the repo says each review is a draft.
// `window.__TEAM__` says which repo, branch and PR. In the worktrees view without a check, it
// opens Settings › Automation on that repo, for a screenshot:
//   PREVIEW_INIT=scripts/preview/team-init.js PREVIEW_NAME=team-settings scripts/preview.sh
(function () {
  var place = null;
  var placeOf = function (b) {
    if (place) return place;
    var open = function (p) { return ['merged', 'closed', 'draft'].indexOf(p.state) < 0; };
    var pr = (b.prs || []).filter(function (p) { return !p.mine && !p.autoReview && open(p); })[0];
    var lane = b.lanes.filter(function (l) { return !l.device && l.repoSlug; })[0];
    place = { repo: pr ? pr.repoSlug : lane && lane.repoSlug, base: (pr && pr.base) || 'main', pr: pr ? pr.key : null, number: pr ? pr.number : null };
    window.__TEAM__ = place;
    return place;
  };
  var file = function (p) {
    return {
      repo: p.repo,
      branch: p.base,
      default: true,
      commit: '1a2b3c4d5e6f7a8b9c0d1e2f3a4b5c6d7e8f9a0b',
      settings: {
        reviewInstructions: 'Check that every migration runs both ways.\nCall out a public API that changes without a note in CHANGELOG.md.',
        ownReviewInstructions: '',
        neverSubmit: ['approve'],
        nudgeAbout: 'feedback-and-checks',
        suggests: { reviews: { auto: true, post: 'comment', own: { auto: true } }, pull_requests: { nudge_by_default: true } },
      },
      skipped: [],
      offers: { reviews: { auto: true, own: { auto: true } }, pull_requests: { nudge_by_default: true } },
    };
  };
  var mine = function (p, repos) {
    var out = Object.assign({}, repos);
    out['github.com/' + p.repo] = { reviews: { post: 'draft' } };
    return out;
  };
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd) {
    var got = real.apply(this, arguments);
    if (cmd === 'get_board') {
      return got.then(function (b) {
        var p = placeOf(b);
        if (!p.repo) return b;
        b.teamFiles = [file(p)];
        b.repos = mine(p, b.repos);
        return b;
      });
    }
    if (cmd === 'get_config') {
      return got.then(function (c) {
        if (place && place.repo) c.config.repos = mine(place, c.config.repos);
        return c;
      });
    }
    return got;
  };

  // For a screenshot: Settings › Automation, the repo picked as a worktree's menu picks it.
  (async function () {
    await until(function () { return !!q('.ca-side-config') && !!place; }, 20000, 'the board never came');
    if (document.querySelector('script[src="./check.js"]') || localStorage.getItem('codealong.view') !== 'worktrees' || !place.repo) return;
    q('.ca-side-config').click();
    var nav = function () { return all('.ca-settings-nav button').filter(function (b) { return b.textContent.trim() === 'Automation'; })[0]; };
    await until(function () { return !!nav(); }, 3000, 'no Automation in Settings');
    nav().click();
    await until(function () { return !!q('.ca-settings-scope select'); }, 3000, 'no repo picker');
    var select = q('.ca-settings-scope select');
    Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype, 'value').set.call(select, place.repo);
    select.dispatchEvent(new Event('change', { bubbles: true }));
    var teamWords = q('.ca-settings-teamwords');
    if (teamWords) teamWords.open = true;
  })().catch(function () {});
})();
