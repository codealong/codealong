// Checks Background in the quick panel (Quick.tsx, start.ts `startHappens`): every account that
// starts in the terminal offers it; for an agent with no background service of its own (Codex,
// OpenCode, Antigravity) the start row says its terminal opens behind your windows, and for
// Claude Code that it runs in the background, a terminal attached later; either way ⌘⏎ asks for
// the background place, for that start alone (no `keep_start_place`), and picking a model then
// leaves Background picked. The places are the Runs in row under the agent menu's lines (⌘O opens
// that menu, AgentMenu.tsx `AgentFoot`), each saying what it is on hover, and the agent chip says
// the place; Make default there keeps the one picked as the account's start.
//   PREVIEW_WAIT=60000 scripts/preview.sh quick '' scripts/preview/start-background-check.js
// It needs a Claude Code account and one of another agent in config.toml; a missing one is
// noted and its steps are skipped.
(function () {
  var agentChip = function () { return q('.ca-quick-agent .ca-quick-chip'); };
  var why = function () { var el = q('.ca-quick-compose .ca-quick-row-why'); return el ? el.textContent : ''; };
  var placeRow = function () { return all('.ca-quick-agent .ca-picker-foot-row').filter(function (r) { var l = q('.ca-picker-foot-label', r); return l && l.textContent === 'Runs in'; })[0]; };
  // What the agent chip says of the place.
  var makeDefault = function () { return all('button', placeRow()).filter(function (x) { return x.textContent.trim() === 'Make default'; })[0]; };
  var placeNow = function () { var p = q('.ca-quick-agent .ca-quick-chip-place'); return p ? p.textContent.trim() : ''; };
  var titleOf = function (o) { return o.textContent.trim(); };
  // Opens the agent menu with ⌘O, for its place row's options.
  var placeOptions = function () {
    key(window, { key: 'o', metaKey: true });
    return until(function () { return placeRow(); }, 3000, 'no Runs in row on ⌘O').then(function () { return all('button', placeRow()); });
  };
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
      .then(placeOptions)
      .then(function (opts) {
        var b = opts.find(function (o) { return titleOf(o) === 'Background'; });
        check(account.name + ' offers Background', !!b, opts.map(titleOf).join(', '));
        check(account.name + ': each place says what it is', opts.every(function (o) { return !!o.title; }), opts.map(function (o) { return o.textContent + ': ' + o.title; }).join(' | '));
        if (!b) return null;
        b.click();
        return until(function () { return placeNow() === 'Background'; }, 2000, 'the chip doesn’t say Background').then(function () {
          // Make default is offered where the place picked isn't the account's own.
          var own = new RegExp(account.name + '’s default').test(placeRow().textContent);
          check(account.name + ': Make default is offered unless Background is its own place already', own !== !!makeDefault(), placeRow().textContent);
          key(q('.ca-quick-agent .ca-picker-filter'), { key: 'Escape' });
          return wait(200);
        });
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
        return wait(500);
      })
      .then(function () {
        check(account.name + ': the start leaves its own place as it was', calls('keep_start_place').length === 0, JSON.stringify(calls('keep_start_place')));
        // A start puts the panel away; the stub keeps the page, so open it afresh for the next.
        return wait(300);
      });
  };

  // Picks one of an account's models in the agent menu: a place that takes a model stays picked.
  var modelKeepsBackground = function (account) {
    key(window, { key: 'l', metaKey: true });
    return until(function () { return all('.ca-picker-section').length > 0; }, 3000).then(function () {
      var section = all('.ca-picker-section').find(function (s) { var n = q('.ca-quick-acct-name', s); return !!n && n.textContent === account.name; });
      var model = section && all('.ca-picker-opt', section)[1];
      if (!model) return note(account.name + ' lists no models: the model step skipped');
      model.click();
      return wait(300).then(function () {
        check(account.name + ': picking a model keeps Background', placeNow() === 'Background', placeNow());
        return placeOptions();
      }).then(function (opts) {
        // A place that isn't its own, for Make default: Background, else the first other.
        var picked = makeDefault() ? 'Background' : titleOf(opts.filter(function (o) { return o.getAttribute('aria-checked') !== 'true'; })[0]);
        var button = opts.filter(function (o) { return titleOf(o) === picked; })[0];
        button.click();
        return until(function () { return makeDefault(); }, 2000, 'no Make default for ' + picked).then(function () {
          makeDefault().click();
          return until(function () { return calls('keep_start_place').length > 0; }, 2000, 'Make default kept nothing');
        }).then(function () {
          var asked = calls('keep_start_place').pop()[1];
          var want = (account.places.find(function (p) { return p.label === picked; }) || {}).place;
          check(account.name + ': Make default keeps ' + picked + ' as its start in config.toml', asked.place === want && asked.accounts.join() === account.name, JSON.stringify(asked) + ' wanted ' + want);
          return until(function () { return !makeDefault() && /default/.test(placeRow().textContent); }, 2000, 'the row doesn’t say it’s the default now');
        });
      });
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
      return tryAccount(claude, 'in the background. Attach a terminal any time.').then(function () { return modelKeepsBackground(claude); });
    })
    .then(done)
    .catch(stopped);
})();
