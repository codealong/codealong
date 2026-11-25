// Checks the Analytics window's Experiments tab (`ui/UsageExperiments.tsx`):
// off, no Experiments button appears and `experiment_report` isn't called;
// on (`PREVIEW_EXPERIMENTS=1`), the Experiments tab shows the leaderboard and run details.
//   PREVIEW_EXPERIMENTS=1 scripts/preview.sh sessions '' scripts/preview/usage-experiments-check.js
//   scripts/preview.sh sessions '' scripts/preview/usage-experiments-check.js
(function () {
  var on = !!window.__PREVIEW_EXPERIMENTS__;
  var dialog = function () { return q('.ca-usage'); };
  var button = function (root, label) { return all(root + ' button').find(function (b) { return b.textContent.trim() === label; }); };

  say('usage experiments check: opening');
  until(function () {
    window.__emit('usage');
    return dialog() && q('.ca-usage-stats');
  }, 20000, 'the Analytics window')
    .then(function () {
      if (!on) {
        check('off, no Experiments tab shows', !button('.ca-usage-filters', 'Experiments'));
        check('and experiment_report is not called', calls('experiment_report').length === 0, calls('experiment_report').length);
        return;
      }
      check('on, the window offers Cost and Experiments', !!button('.ca-usage-filters', 'Cost') && !!button('.ca-usage-filters', 'Experiments'));
      button('.ca-usage-filters', 'Experiments').click();
      return until(function () { return !!q('.ca-exp-table'); }, 5000, 'Experiments leaderboard')
        .then(function () {
          check('experiment_report was called', calls('experiment_report').length >= 1, calls('experiment_report').length);
          check('leaderboard lists both arms', all('.ca-exp-section .ca-exp-table tbody tr').length >= 2);
          check('run shows judge recommendation', /Recommended codex\/sol-6\.2\/high/.test(q('.ca-exp-rec')?.textContent || ''));
          check('pairwise comparison is rendered', all('.ca-exp-pair').length === 1);
        });
    })
    .then(function () { done(); })
    .catch(function (e) { check('fatal', false, String(e)); done(); });
})();
