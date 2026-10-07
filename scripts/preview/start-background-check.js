// Checks Background in the quick panel (Quick.tsx, start.ts `startHappens`): every account that
// starts in the terminal offers it; for an agent with no background service of its own (Codex,
// OpenCode, Antigravity) the start row says its terminal opens behind your windows, and for
// Claude Code that it runs in the background, a terminal attached later; either way ⌘⏎ asks for
// the background place.
//   PREVIEW_WAIT=60000 scripts/preview.sh quick '' scripts/preview/start-background-check.js
// It needs a Claude Code account and one of another agent in config.toml; a missing one is
// noted and its steps are skipped.
(function () {
  var agentChip = function () { return q('.ca-quick-agent .ca-quick-chip'); };
  var why = function () { var el = q('.ca-quick-compose .ca-quick-row-why'); return el ? el.textContent : ''; };
  var placeButton = function (label) { return all('.ca-quick-places button').find(function (b) { return b.textContent.trim() === label; }); };
  var box = function () { return q('.ca-quick-input textarea'); };
  var listed = null;

  // Picks an account on its own default model in the agent menu, by the section its name heads.
  var pickAccount = function (name) {
    key(window, { key: 'l', metaKey: true });
    return until(function () { return all('.ca-picker-section').length > 0; }, 3000).then(function () {
      var section = all('.ca-picker-section').find(function (s) { var n = q('.ca-quick-acct-name', s); return !!n && n.textContent === name; });
      var opt = section && q('.ca-picker-opt', section);
      if (!opt) throw new Error('no option for ' + name + ' in the agent menu');
      opt.click();
      return until(function () { return agentChip().textContent.indexOf(name) >= 0; }, 3000);
    });
  };

  // Picks an account, then Background, and checks what the start row says and what ⌘⏎ asks for.
  var tryAccount = function (account, words) {
    return pickAccount(account.name)
      .then(function () {
        var b = placeButton('Background');
        check(account.name + ' offers Background', !!b, all('.ca-quick-places button').map(function (x) { return x.textContent; }).join(', '));
        if (!b) return null;
        b.click();
        return until(function () { return b.classList.contains('is-on'); }, 2000);
      })
      .then(function () {
        check(account.name + ': the start row says ' + words, why().indexOf(words) >= 0, why());
        note(account.name + ': ' + why());
        Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value').set.call(box(), 'Say hello');
        box().dispatchEvent(new Event('input', { bubbles: true }));
        var before = calls('start_from_quick').length;
        return wait(200).then(function () {
          key(window, { key: 'Enter', metaKey: true });
          return until(function () { return calls('start_from_quick').length > before; }, 3000);
        });
      })
      .then(function () {
        var spec = calls('start_from_quick').pop()[1].spec;
        check(account.name + ': ⌘⏎ starts it in the background', spec.account === account.name && spec.place === 'background', JSON.stringify({ account: spec.account, place: spec.place }));
        // A start puts the panel away; the stub keeps the page, so open it afresh for the next.
        return wait(300);
      });
  };

  localStorage.setItem('codealong.quick.project', 'All projects');
  say('start background check: waiting for the panel');
  fetch('./start-options.json')
    .then(function (r) { return r.json(); })
    .then(function (o) {
      listed = o.accounts;
      return until(function () { return calls('quick_ready').length > 0 && !!agentChip() && !agentChip().disabled; }, 30000);
    })
    .then(function () {
      listed.forEach(function (a) {
        var terminal = a.places.some(function (p) { return p.place === 'terminal'; });
        var background = a.places.find(function (p) { return p.place === 'background'; });
        if (terminal) check(a.name + ' is offered Background, as it starts in the terminal', !!background, a.places.map(function (p) { return p.place; }).join(', '));
        if (background) check(a.name + '’s Background says whether it’s behind your windows', !!background.behind === (a.harness !== 'claude'), String(background.behind));
      });
      var other = listed.find(function (a) { return a.harness !== 'claude' && a.places.some(function (p) { return p.place === 'background'; }); });
      if (!other) return note('no account besides Claude Code starts in the terminal here: its steps skipped');
      return tryAccount(other, 'behind your windows');
    })
    .then(function () {
      var claude = listed.find(function (a) { return a.harness === 'claude' && a.places.some(function (p) { return p.place === 'background'; }); });
      if (!claude) return note('no Claude Code account here: its steps skipped');
      return tryAccount(claude, 'in the background. Attach a terminal any time.');
    })
    .then(done)
    .catch(stopped);
})();
