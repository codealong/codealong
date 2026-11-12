// Checks the Analytics window's Your part and Sessions (ui/UsageAnalytics.tsx), the analytics
// experiment's views, on the report fake-tauri.js makes up (`sampleAnalytics`): with the
// experiment on the window has its three views; Your part leads with your prompts and what
// share of the turns you began, charts each day's prompts by what they did, splits the turns
// and their cost by who began them, and lists them by account, project and session; Sessions
// says how sessions began and how the ones you marked done stood, and lists the unfinished. The
// days ask for both reports again. Off, the window is what it was: no views, no analytics asked.
//   PREVIEW_ANALYTICS=1 scripts/preview.sh sessions '' scripts/preview/usage-analytics-check.js
//   scripts/preview.sh sessions '' scripts/preview/usage-analytics-check.js
(function () {
  var on = !!window.__PREVIEW_ANALYTICS__;
  var dialog = function () { return q('.ca-usage'); };
  var tiles = function () { return all('.ca-usage-stat').map(function (t) { return t.textContent; }); };
  var button = function (root, label) { return all(root + ' button').find(function (b) { return b.textContent.trim() === label; }); };
  var titles = function () { return all('.ca-usage-chart-title').map(function (t) { return t.textContent; }); };

  say('usage analytics check: opening');
  until(function () {
    window.__emit('usage');
    return dialog() && q('.ca-usage-stats');
  }, 20000, 'the Analytics window')
    .then(function () {
      if (!on) {
        check('off, the window has no views', !button('.ca-usage-filters', 'Your part') && !button('.ca-usage-filters', 'Sessions'));
        check('and asks for no analytics', calls('analytics_report').length === 0, calls('analytics_report').length);
        check('nor offers the experiment’s rows', !button('.ca-usage-by', 'Begun by'));
        return;
      }
      check('on, the window has its three views', !!button('.ca-usage-filters', 'Cost') && !!button('.ca-usage-filters', 'Your part') && !!button('.ca-usage-filters', 'Sessions'));
      check('it opens on Cost, as it always did', /At API prices/.test(tiles()[0]), tiles()[0]);
      button('.ca-usage-by', 'Turn begun by').click();
      return until(function () { return all('.ca-usage-table tbody tr').length >= 3 && /You/.test(q('.ca-usage-table tbody th').textContent); }, 5000, 'the cost by who began the turn')
        .then(function () {
          var rows = all('.ca-usage-table tbody th').map(function (t) { return t.textContent; });
          check('Cost lists what turns cost by who began them', rows.indexOf('You') >= 0 && rows.indexOf('Other agents') >= 0 && rows.indexOf('Codealong') >= 0, rows.join(', '));
          button('.ca-usage-filters', 'Your part').click();
          return until(function () { return /Your prompts/.test(tiles()[0] || ''); }, 5000, 'Your part');
        })
        .then(function () {
          var asked = last('analytics_report');
          check('it asked for 30 days of every account', asked && asked.days === 30 && asked.account === null, JSON.stringify(asked));
          check('it leads with your prompts', /Your prompts[\d,]+about \d+ a day/.test(tiles()[0]), tiles()[0]);
          check('and the share of turns you started', /Turns you started\d+%[\d,]+ of [\d,]+/.test(tiles()[1]), tiles()[1]);
          check('and what the turns you started cost', /Cost of your turns\d+%\$[\d,.]+ of \$[\d,.]+ at API prices/.test(tiles()[2]), tiles()[2]);
          check('and how often you agreed', /You agreed\d+%of [\d,]+ replies to the agent/.test(tiles()[3]), tiles()[3]);
          check('each of the 30 days has its prompts stacked', all('.ca-usage-bar').length === 30, all('.ca-usage-bar').length);
          var keys = all('.ca-usage-chart .ca-usage-legend .ca-usage-key').map(function (k) { return k.textContent; });
          check('the chart names what a prompt did', ['Set out new work', 'Agreed', 'Steered', 'Redirected'].every(function (k) { return keys.indexOf(k) >= 0; }), keys.slice(0, 8).join(', '));
          check('the turns and their cost are split by who began them', all('.ca-usage-split-bar').length === 2 && all('.ca-usage-split-bar')[0].children.length >= 3, all('.ca-usage-split-bar').length);
          check('more about your prompts is listed', all('.ca-usage-marks li').length >= 3, all('.ca-usage-marks li').length);
          check('a row’s prompts are cut by what they did', all('.ca-usage-table .ca-usage-mix').length >= 1 && q('.ca-usage-mix').children.length >= 2);
          check('the prompts that steered or redirected are listed', titles().indexOf('Where you steered or redirected') >= 0 && all('.ca-usage-list li').length >= 1, titles().join(' | '));
          check('and what waits on a read is said', /haven’t been read/.test(q('.ca-usage-notes').textContent), q('.ca-usage-notes').textContent.slice(0, 80));
          button('.ca-usage-by', 'Session').click();
          return until(function () { return !!q('.ca-usage-table tbody .ca-linkbtn'); }, 5000, 'session rows');
        })
        .then(function () {
          check('a session’s row names a session of the board', !!q('.ca-usage-table tbody .ca-linkbtn'));
          button('.ca-usage-filters', '7 days').click();
          return until(function () { return last('analytics_report').days === 7 && all('.ca-usage-bar').length === 7; }, 5000, 'seven days');
        })
        .then(function () {
          check('the days ask for both reports again', last('analytics_report').days === 7 && last('token_report').days === 7);
          button('.ca-usage-filters', 'Sessions').click();
          return until(function () { return /Sessions[\d,]+active in these days/.test(tiles()[0] || ''); }, 5000, 'Sessions');
        })
        .then(function () {
          check('Sessions says how many you started', /Sessions you started\d+%[\d,]+ of [\d,]+/.test(tiles()[1]), tiles()[1]);
          check('and how many you marked done, and left unfinished', /Marked done\d+\d+ of them finished/.test(tiles()[2]) && /Left unfinished\d+/.test(tiles()[3]), tiles()[2] + ' | ' + tiles()[3]);
          var rows = all('.ca-usage-table tbody th').map(function (t) { return t.textContent; });
          check('it lists how sessions began', rows.indexOf('You') >= 0 && rows.indexOf('An agent, as its helper') >= 0 && rows.indexOf('Codealong, to review a PR') >= 0, rows.join(', '));
          check('how sessions stood when you were done with them is one bar', all('.ca-usage-split-bar').length === 1 && all('.ca-usage-split-bar')[0].children.length >= 3);
          check('the unfinished ones are listed, with how each stood', titles().indexOf('Left unfinished') >= 0 && /asked you something|hadn’t landed|still open/.test(q('.ca-usage-list').textContent), q('.ca-usage-list') && q('.ca-usage-list').textContent.slice(0, 80));
          check('and the ones still open with work left undone', titles().indexOf('Still open, with work left undone') >= 0);
          var link = q('.ca-usage-list .ca-linkbtn');
          if (!link) {
            note('no listed session is on the board to show');
            return;
          }
          link.click();
          return until(function () { return !dialog(); }, 5000, 'the window to close').then(function () {
            check('a listed session closes the window to show it', !dialog());
          });
        });
    })
    .then(done, stopped);
})();
