// Checks Settings › Jev's usage (JevUsage.tsx) against fake-tauri.js's made-up report, with a key
// saved: the last 7 days at first, their cost large with the requests and tokens beside it, and a
// row for each feature that asked, the most tokens first, named as its switch is. Today sums only
// today; 30 days reaches back past when counting started, and says since when. The Test button
// reads the usage again.
//   scripts/preview.sh sessions '' scripts/preview/jev-usage-check.js
(function () {
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    if (cmd === 'jev_key') return Promise.resolve(true);
    return real(cmd, args, options);
  };
  var nav = function (label) { return all('.ca-settings-nav button').filter(function (b) { return b.textContent.trim() === label; })[0]; };
  var button = function (label) { return all('.ca-settings-pane button').filter(function (b) { return b.textContent.trim() === label; })[0]; };
  var field = function () { return all('.ca-settings-pane .ca-field').filter(function (f) { var s = f.querySelector(':scope > span'); return s && s.textContent === 'Usage'; })[0]; };
  var names = { quick_panel: 'Quick panel suggestions', sessions: 'Session status', todos: 'Todos', pull_requests: 'Pull request reviews', emblems: 'Session emblems', test: 'Test button' };
  // What the page should say for the last `days` days of the report.
  var expect = function (report, days) {
    var by = {};
    report.days.slice(-days).forEach(function (d) {
      Object.keys(d.features).forEach(function (f) {
        var u = d.features[f];
        by[f] = by[f] || { requests: 0, tokens: 0 };
        by[f].requests += u.requests;
        by[f].tokens += u.tokens;
      });
    });
    var order = Object.keys(by).sort(function (a, b) { return by[b].tokens - by[a].tokens || a.localeCompare(b); });
    var requests = order.reduce(function (s, f) { return s + by[f].requests; }, 0);
    return { order: order.map(function (f) { return names[f]; }), requests: requests };
  };
  var shown = function () {
    var f = field();
    return {
      cost: (q('.ca-jev-usage-cost', f) || {}).textContent,
      counts: (q('.ca-jev-usage-counts', f) || {}).textContent || '',
      rows: all('tbody th', f).map(function (th) { return th.textContent; }),
      note: (q('small', f) || {}).textContent || '',
      on: (q('.ca-segctl .is-on', f) || {}).textContent,
    };
  };
  var segment = function (label) { return all('.ca-segctl button', field()).filter(function (b) { return b.textContent === label; })[0]; };

  (async function () {
    try {
      var gear = await until(function () { return q('.ca-side-config'); }, 20000, 'no settings button');
      gear.click();
      (await until(function () { return nav('Jev'); }, 10000, 'no Jev in Settings')).click();
      await until(function () { return field() && q('.ca-jev-usage-cost', field()); }, 10000, 'the usage never showed');
      var report = await real('jev_usage', { days: 30 });

      var week = expect(report, 7);
      var s = shown();
      check('the last 7 days show first', s.on === '7 days', s.on);
      check('their requests are summed', s.counts.indexOf(week.requests.toLocaleString('en-US') + ' requests') === 0, s.counts);
      check('a row for each feature, the most tokens first', s.rows.join(' | ') === week.order.join(' | '), s.rows.join(' | ') + ' / ' + week.order.join(' | '));
      check('a cost to the cent or finer', /^\$\d/.test(s.cost), s.cost);
      check('no since when inside the days counted', s.note.indexOf('Counted since') < 0, s.note);

      segment('Today').click();
      var today = expect(report, 1);
      await until(function () { return shown().on === 'Today'; }, 5000, 'Today never came on');
      s = shown();
      check('Today sums today only', s.counts.indexOf(today.requests.toLocaleString('en-US') + ' request') === 0 && s.rows.join(' | ') === today.order.join(' | '), s.counts + ' / ' + s.rows.join(' | '));

      segment('30 days').click();
      await until(function () { return shown().on === '30 days'; }, 5000, '30 days never came on');
      s = shown();
      check('30 days says since when it was counted', s.note.indexOf('Counted since') === 0, s.note);

      var before = calls('jev_usage').length;
      button('Test').click();
      await until(function () { return calls('jev_usage').length > before; }, 5000, 'the usage was never read again');
      check('Test reads the usage again', true);
    } catch (e) {
      stopped(e);
      return;
    }
    done();
  })();
})();
