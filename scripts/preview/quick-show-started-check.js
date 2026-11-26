// Checks the quick panel's switch for showing what it starts as it starts (AgentMenu.tsx
// `AgentFoot`, newSession.ts `showsStart`, `[sessions] show_started`): under Runs in in the agent
// menu (⌘O), offered only where nothing of the session's own opens (Background, Codealong), off
// until turned on. Off, ⌘⏎ on a prompt starts it from the panel (`start_from_quick`) and ⌥⌘⏎
// hands it to the main window already sent (`show_main` with a new session whose `sent` is the
// prompt), which shows it as it starts; on, the two swap. Flipping it keeps it in config.toml
// (`keep_show_started`).
//   PREVIEW_WAIT=60000 scripts/preview.sh quick '' scripts/preview/quick-show-started-check.js
// It needs an account that starts in the background and somewhere else; without one it's noted
// and the steps are skipped.
(function () {
  var agentChip = function () { return q('.ca-quick-agent .ca-quick-chip'); };
  var placeRow = function () { return all('.ca-quick-agent .ca-picker-foot-row').filter(function (r) { var l = q('.ca-picker-foot-label', r); return l && l.textContent === 'Runs in'; })[0]; };
  var switchRow = function () { return q('.ca-quick-agent .ca-picker-foot-check'); };
  var placeNow = function () { var p = q('.ca-quick-agent .ca-quick-chip-place'); return p ? p.textContent.trim() : ''; };
  var box = function () { return q('.ca-quick-input textarea'); };
  var filter = function () { return q('.ca-quick-agent .ca-picker-filter'); };

  // Opens the agent menu with ⌘O on the place row, and picks `label` there.
  var pickPlace = function (label) {
    if (filter()) key(filter(), { key: 'Escape' });
    return wait(150).then(function () {
      key(window, { key: 'o', metaKey: true });
      return until(function () { return placeRow(); }, 3000, 'no Runs in row on ⌘O');
    }).then(function () {
      var b = all('button', placeRow()).filter(function (o) { return o.textContent.trim() === label; })[0];
      if (!b) throw new Error('no ' + label + ' under Runs in');
      b.click();
      return until(function () { return placeNow() === label; }, 2000, 'the chip doesn’t say ' + label);
    });
  };
  var closeMenu = function () {
    if (filter()) key(filter(), { key: 'Escape' });
    return wait(150);
  };
  var type = function (words) {
    Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value').set.call(box(), words);
    box().dispatchEvent(new Event('input', { bubbles: true }));
    return wait(200);
  };

  // Picks Background, types `words` and presses ⌘⏎ (⌥⌘⏎ with `alt`): what was asked for.
  var startWith = function (words, alt) {
    var starts = calls('start_from_quick').length;
    var shows = calls('show_main').length;
    return pickPlace(labelOf('background'))
      .then(closeMenu)
      .then(function () { return type(words); })
      .then(function () {
        key(box(), { key: 'Enter', metaKey: true, altKey: !!alt });
        return until(function () { return calls('start_from_quick').length > starts || calls('show_main').length > shows; }, 3000, 'neither started nor shown');
      })
      .then(function () {
        return wait(200).then(function () {
          var started = calls('start_from_quick').length > starts;
          var shown = calls('show_main').length > shows;
          var reveal = shown ? calls('show_main').pop()[1].reveal || {} : null;
          return { started: started, shown: shown, reveal: reveal };
        });
      });
  };
  var shownSent = function (did, words) {
    var r = did.reveal || {};
    var draft = r.draft || {};
    return did.shown && !did.started && r.kind === 'new' && draft.sent === words && (draft.spec || {}).prompt === words && (draft.spec || {}).place === 'background';
  };

  localStorage.setItem('codealong.quick.project', 'All projects');
  say('quick show-started check: waiting for the panel');
  var account = null;
  var labelOf = function (place) { return account.places.find(function (p) { return p.place === place; }).label; };
  fetch('./start-options.json')
    .then(function (r) { return r.json(); })
    .then(function (o) {
      account = o.accounts.find(function (a) { return a.places.some(function (p) { return p.place === 'background'; }) && a.places.some(function (p) { return p.place === 'terminal'; }); });
      return until(function () { return calls('quick_ready').length > 0 && !!agentChip() && !agentChip().disabled; }, 30000);
    })
    .then(function () {
      if (!account) return note('no account here starts both in the background and in a terminal: steps skipped');
      key(window, { key: 'l', metaKey: true });
      return until(function () { return all('.ca-picker-section').length > 0; }, 3000)
        .then(function () {
          var section = all('.ca-picker-section').find(function (s) { var n = q('.ca-quick-acct-name', s); return !!n && n.textContent === account.name; });
          q('.ca-picker-opt', section).click();
          return until(function () { return agentChip().textContent.indexOf(account.name) >= 0; }, 3000);
        })
        .then(function () { return pickPlace(labelOf('terminal')); })
        .then(function () {
          check('a terminal comes forward by itself: no switch under Runs in', !switchRow(), switchRow() ? switchRow().textContent : '');
          return pickPlace(labelOf('background'));
        })
        .then(function () {
          var row = switchRow();
          check('in the background the switch is under Runs in', !!row && /Show it as it starts/.test(row.textContent), row ? row.textContent : 'none');
          check('and off until turned on', !!row && !q('input', row).checked, '');
          check('it says what it does on hover, with the key for one start', !!row && /⌥⌘⏎/.test(row.title), row ? row.title : '');
          return closeMenu();
        })
        .then(function () { return startWith('Tidy the importer'); })
        .then(function (did) {
          check('off, ⌘⏎ starts it from the panel and shows nothing', did.started && !did.shown, JSON.stringify(did).slice(0, 300));
          return startWith('Tidy the exporter', true);
        })
        .then(function (did) {
          check('off, ⌥⌘⏎ shows it in the main window, already sent', shownSent(did, 'Tidy the exporter'), JSON.stringify(did).slice(0, 400));
          return pickPlace(labelOf('background'));
        })
        .then(function () {
          switchRow().click();
          return until(function () { return calls('keep_show_started').length > 0; }, 2000, 'the switch kept nothing');
        })
        .then(function () {
          check('turning it on keeps it in config.toml', calls('keep_show_started').pop()[1].on === true, JSON.stringify(calls('keep_show_started')));
          check('and it shows on at once', q('input', switchRow()).checked, '');
          check('the menu stays open for it', !!placeRow(), '');
          return closeMenu();
        })
        .then(function () { return startWith('Tidy the parser'); })
        .then(function (did) {
          check('on, ⌘⏎ shows it in the main window, already sent', shownSent(did, 'Tidy the parser'), JSON.stringify(did).slice(0, 400));
          return startWith('Tidy the lexer', true);
        })
        .then(function (did) {
          check('on, ⌥⌘⏎ starts it from the panel and shows nothing', did.started && !did.shown, JSON.stringify(did).slice(0, 300));
        });
    })
    .then(done)
    .catch(stopped);
})();
