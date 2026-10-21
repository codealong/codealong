// Checks the Usage window (ui/UsageDialog.tsx) on the report fake-tauri.js makes up from the
// board: View › Usage… opens it on every account for 30 days; its tiles say what the work would
// have cost, what was paid and the leverage; the days and the account ask the ledger again; a
// chip on the usage bar opens it on that account; a session's row shows the session; and Plan
// prices… goes to Settings › Accounts.
//   scripts/preview.sh sessions '' scripts/preview/usage-check.js
(function () {
  var dialog = function () { return q('.ca-usage'); };
  var tiles = function () { return all('.ca-usage-stat').map(function (t) { return t.textContent; }); };
  var seg = function (root, label) { return all(root + ' button').find(function (b) { return b.textContent.trim() === label; }); };
  var open = function () {
    return until(function () {
      window.__emit('usage');
      return dialog() && q('.ca-usage-stats');
    }, 20000, 'the Usage window');
  };

  say('usage check: opening');
  open()
    .then(function () {
      var asked = last('token_report');
      check('it asks for 30 days of every account', asked && asked.days === 30 && asked.account === null, JSON.stringify(asked));
      check('it leads with what the work would have cost', /At API prices\$[\d,.]+/.test(tiles()[0]), tiles()[0]);
      check('and says what was paid, by kind', /Paid\$[\d,.]+plans \$/.test(tiles()[1]), tiles()[1]);
      check('and the leverage', /Leverage\d+(\.\d)?×/.test(tiles()[2]), tiles()[2]);
      check('and the tokens, with the cache’s share', /Tokens[\d.]+[KMB]?\d+% read from cache/.test(tiles()[3]), tiles()[3]);
      var bars = all('.ca-usage-bar').length;
      check('the daily chart has a bar for each of the 30 days', bars === 30, bars);
      check('the accounts are named beside the chart', all('.ca-usage-legend .ca-usage-swatch').length >= 1);
      seg('.ca-usage-filters .ca-segctl', '7 days').click();
      return until(function () { return last('token_report').days === 7 && all('.ca-usage-bar').length === 7; }, 5000, 'seven days');
    })
    .then(function () {
      check('7 days asks for 7', last('token_report').days === 7);
      var select = q('.ca-usage-account select');
      var account = select.options[1].value;
      select.value = account;
      select.dispatchEvent(new Event('change', { bubbles: true }));
      return until(function () { return last('token_report').account === account; }, 5000, 'one account').then(function () { return account; });
    })
    .then(function (account) {
      check('picking an account asks for it alone', last('token_report').account === account, JSON.stringify(last('token_report')));
      seg('.ca-usage-by .ca-segctl', 'Session').click();
      return until(function () { return all('.ca-usage-table tbody tr').length > 0; }, 5000, 'session rows');
    })
    .then(function () {
      var link = q('.ca-usage-table tbody .ca-linkbtn');
      if (!link) {
        note('no session of the board in the report to show');
        return;
      }
      link.click();
      return until(function () { return !dialog(); }, 5000, 'the window to close').then(function () {
        check('a session’s row closes the window to show it', !dialog());
      });
    })
    .then(function () {
      var chip = q('.ca-usagebar-accts .ca-uchip:not(.is-attn)');
      if (!chip) throw new Error('no account chip on the usage bar');
      var name = chip.querySelector('.ca-uchip-name').textContent;
      chip.click();
      return until(function () { return dialog() && last('token_report').account === name; }, 5000, 'the window on that account').then(function () {
        check('a chip on the usage bar opens it on its account', q('.ca-usage-account select').value === name, q('.ca-usage-account select').value);
      });
    })
    .then(function () {
      seg('.ca-dialog-foot', 'Plan prices…').click();
      return until(function () { return !dialog() && q('.ca-settings'); }, 5000, 'Settings');
    })
    .then(function () {
      check('Plan prices… opens Settings at Accounts', /Accounts/.test((q('.ca-settings-intro h3') || {}).textContent || ''), (q('.ca-settings-intro h3') || {}).textContent);
    })
    .then(done)
    .catch(stopped);
})();
