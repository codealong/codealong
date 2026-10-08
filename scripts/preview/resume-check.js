// Checks going on with a session that isn't running from its box (ResumeBar.tsx, start.ts
// `resumeChoice`): a chip beside Send offers the places its account's sessions go on in, each
// saying what it is, and the chip after it the account's models, where a running session's box has
// its model; the placeholder says what sending does; a model leaves only the places that take one;
// a send asks the engine to resume it there, on that model, and the place is kept for the
// account's next one; and its menu has Resume in, with the same places.
//   scripts/preview.sh peek '' scripts/preview/resume-check.js
(function () {
  // This runs before the app's script: pick a session on this Mac that isn't running, in a
  // terminal, whose account offers more than one place to go on in.
  var read = function (url) {
    var xhr = new XMLHttpRequest();
    xhr.open('GET', url, false);
    xhr.send();
    return JSON.parse(xhr.responseText);
  };
  var board = read('./board.json');
  var accounts = read('./start-options.json').accounts;
  var account = function (s) { return accounts.filter(function (a) { return a.name === s.account && a.harness === s.harness; })[0]; };
  var picked = null;
  var pickedLane = null;
  board.lanes.forEach(function (l) {
    l.sessions.forEach(function (s) {
      var a = account(s);
      var waits = s.status === 'needs' && !s.asks;
      if (!picked && !s.live && !s.hosted && !s.archived && !waits && s.surface === 'cli' && !l.hidden && !l.device && a && (a.resumes || []).length > 1 && (a.models || []).length > 1) {
        picked = s;
        pickedLane = l;
      }
    });
  });
  if (!picked) { stopped('no session that isn’t running, on an account with places and models to resume with'); return; }
  var acct = account(picked);
  var offered = acct.places.filter(function (p) { return acct.resumes.indexOf(p.place) >= 0; });
  var labelOf = function (place) { return (offered.filter(function (p) { return p.place === place; })[0] || {}).label; };
  localStorage.setItem('codealong.preview.peek', picked.key);
  localStorage.removeItem('codealong.draft.' + picked.key);
  localStorage.removeItem('codealong.resumed');

  var placeChip = function () { return q('.ca-peek .ca-composer-buttons .ca-placemenu-chip'); };
  var box = function () { return q('.ca-peek .ca-composer textarea'); };
  var options = function () { return all('.ca-peek .ca-placemenu .ca-picker-opt'); };
  var titleOf = function (o) { return q('.ca-picker-title', o).textContent.trim(); };
  var places = function () { return options().map(titleOf); };
  var on = function () { return placeChip() ? placeChip().textContent.trim() : ''; };
  // Opens the menu of places; the chip closes it again.
  var openPlaces = function () {
    placeChip().click();
    return until(function () { return options().length > 0; }, 3000, 'no menu of places');
  };
  var closePlaces = function () {
    placeChip().click();
    return until(function () { return options().length === 0; }, 3000, 'the menu of places stays open');
  };
  var pickPlace = function (label) {
    return openPlaces().then(function () {
      var o = options().filter(function (x) { return titleOf(x) === label; })[0];
      if (!o) throw new Error('no ' + label + ' among the places: ' + places().join(', '));
      o.click();
      return wait(50);
    });
  };
  var chip = function () { return q('.ca-peek .ca-composer-buttons .ca-modelmenu-chip'); };
  var enter = function (el) { el.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true })); };
  var other = acct.models.filter(function (m) { return m.id !== acct.defaultModel; })[0];

  say('resume check: ' + picked.key + ' on ' + acct.name);
  until(function () { return !!placeChip(); }, 20000, 'no place chip in the box')
    .then(function () {
      check('it goes on in a terminal unless picked otherwise', on() === labelOf('terminal'), on());
      check('the placeholder says it resumes in the terminal', box().getAttribute('aria-placeholder').indexOf('resumes it in ' + board.apps.terminal) >= 0, box().getAttribute('aria-placeholder'));
      check('the model chip says the account’s own model', !!chip() && chip().textContent.length > 0, chip() ? chip().textContent : 'none');
      check('the chips sit beside Send, the place first', !!chip() && !!q('.ca-composer-send', chip().closest('.ca-composer-buttons')) && chip().getBoundingClientRect().left >= placeChip().getBoundingClientRect().right, chip() ? chip().getBoundingClientRect().left + ' / ' + placeChip().getBoundingClientRect().right : 'none');
      return openPlaces();
    })
    .then(function () {
      check('its menu offers the places its account’s sessions go on in', places().join(', ') === offered.map(function (p) { return p.label; }).join(', '), places().join(', '));
      check('each says what it is', options().every(function (o) { return !!q('.ca-picker-detail', o); }), options().map(function (o) { return o.textContent; }).join(' | '));
      var menu = q('.ca-peek .ca-placemenu .ca-picker');
      check('the menu opens upward, over the conversation', menu.getBoundingClientRect().bottom <= placeChip().getBoundingClientRect().top + 1, menu.getBoundingClientRect().bottom + ' / ' + placeChip().getBoundingClientRect().top);
      return closePlaces();
    })
    .then(function () { return pickPlace(labelOf('background')); })
    .then(function () {
      var behind = (offered.filter(function (p) { return p.place === 'background'; })[0] || {}).behind;
      check('the menu closes on a pick, and the chip says it', options().length === 0 && on() === labelOf('background'), on());
      check('Background says where it goes', box().getAttribute('aria-placeholder').indexOf(behind ? 'behind your windows' : 'resumes it in the background') >= 0, box().getAttribute('aria-placeholder'));
      if (labelOf('claude-app')) {
        return pickPlace(labelOf('claude-app')).then(function () {
          check('the Claude app picks its own model, so the chip goes', !chip(), chip() ? chip().textContent : 'gone');
          check('the Claude app takes it on the clipboard', box().getAttribute('aria-placeholder').indexOf('Claude app opens it') >= 0, box().getAttribute('aria-placeholder'));
          return pickPlace(labelOf('background'));
        });
      }
    })
    .then(function () {
      chip().click();
      return until(function () { return !!q('.ca-peek .ca-modelmenu .ca-picker'); }, 3000, 'no model menu');
    })
    .then(function () {
      var menu = q('.ca-peek .ca-modelmenu .ca-picker');
      var rect = menu.getBoundingClientRect();
      check('the model menu opens upward, over the conversation', rect.bottom <= chip().getBoundingClientRect().top + 1, rect.bottom + ' / ' + chip().getBoundingClientRect().top);
      var opt = all('.ca-picker-opt', menu).filter(function (b) { return b.querySelector('.ca-picker-title').textContent.trim() === other.label.split(':')[0].replace(/ \(1M context\)$/, ' 1M'); })[0];
      check('the menu lists the account’s models', !!opt, all('.ca-picker-title', menu).map(function (t) { return t.textContent; }).join(', '));
      opt.click();
      return wait(50);
    })
    .then(function () {
      check('the menu closes on a pick', !q('.ca-peek .ca-modelmenu .ca-picker'), 'open');
      return openPlaces();
    })
    .then(function () {
      check('a model leaves only the places that take one', places().indexOf('Claude app') < 0 && on() === labelOf('background'), places().join(', ') + ' · ' + on());
      return closePlaces();
    })
    .then(function () {
      type(box(), 'Carry on where you left off');
      enter(box());
      return until(function () { return calls('resume_session').length === 1; }, 5000, 'no resume asked for');
    })
    .then(function () {
      var asked = sent('resume_session')[0];
      check('the send resumes it there, on that model', asked.key === picked.key && asked.resume.place === 'background' && asked.resume.model === other.id && asked.prompt === 'Carry on where you left off', JSON.stringify(asked));
      check('nothing went as a plain send', calls('send_to_session').length === 0, String(calls('send_to_session').length));
      check('the place is kept for the account’s next one', JSON.parse(localStorage.getItem('codealong.resumed') || '{}')[acct.name] === 'background', localStorage.getItem('codealong.resumed'));
      return menuOf(q('.ca-peek .ca-reply'));
    })
    .then(function (labels) {
      var resumes = labels.filter(function (l) { return l.indexOf('Resume in › ') === 0; }).map(function (l) { return l.slice('Resume in › '.length); });
      check('its menu has Resume in, with the same places', resumes.join(', ') === offered.map(function (p) { return p.label; }).join(', '), labels.join(' | '));
      (menuItem('Resume in › ' + labelOf('codealong')) || menuItem('Resume in › ' + labelOf('terminal')))();
      return until(function () { return calls('resume_session').length === 2; }, 5000, 'Resume in asked nothing');
    })
    .then(function () {
      var asked = sent('resume_session')[1];
      check('Resume in goes on with no prompt, on the account’s own model', asked.prompt === '' && asked.resume.model === null, JSON.stringify(asked));
    })
    .then(done, stopped);
})();
