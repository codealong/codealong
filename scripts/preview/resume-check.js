// Checks going on with a session that isn't running from its box (SessionAgent.tsx, start.ts
// `resumeChoice`): the chip beside Send, its account, model and effort, has the account's models
// in its menu and under them a Runs in row of the places its account's sessions go on in, each
// saying what it is on hover; the foot's left says where it goes on, and the placeholder what
// sending does; a model leaves only the places that take one, and a place whose app picks the model
// leaves no models to pick; a send asks the engine to resume it there, on that model, and the
// place is kept for the account's next one; and its menu has Resume in, with the same places.
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
  // Its account has as many levels as Codex's, the most any harness has, for the narrow box below.
  var codexLevels = ['low', 'medium', 'high', 'xhigh', 'max', 'ultra'].map(function (id) { return { id: id, label: id }; });
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd) {
    var got = real.apply(this, arguments);
    if (cmd !== 'start_options') return got;
    return got.then(function (o) {
      o.accounts.forEach(function (a) { if (a.name === picked.account && a.harness === picked.harness) a.efforts = codexLevels; });
      return o;
    });
  };

  var chip = function () { return q('.ca-peek .ca-composer-buttons .ca-modelmenu-chip'); };
  var menu = function () { return q('.ca-peek .ca-composer-buttons .ca-modelmenu .ca-picker'); };
  var box = function () { return q('.ca-peek .ca-composer textarea'); };
  // The Runs in row under the menu's lines, and its places.
  var placeRow = function () { return menu() && all('.ca-picker-foot-row', menu()).filter(function (r) { var l = q('.ca-picker-foot-label', r); return l && l.textContent === 'Runs in'; })[0]; };
  var options = function () { return placeRow() ? all('button', placeRow()) : []; };
  var titleOf = function (o) { return o.textContent.trim(); };
  var places = function () { return options().map(titleOf); };
  // Where the foot's left says it goes on.
  var on = function () { var p = q('.ca-peek .ca-boxplace'); return p ? p.textContent.trim() : ''; };
  // The models under its account in the menu.
  var models = function () { var own = menu() && all('.ca-picker-section', menu()).filter(function (x) { var n = q('.ca-quick-acct-name', x); return n && n.textContent === picked.account; })[0]; return own ? all('.ca-picker-opt', own) : []; };
  // Opens the chip's menu; the chip closes it again.
  var openMenu = function () {
    if (menu()) return Promise.resolve();
    chip().click();
    // Its rows come once the account's places are read.
    return until(function () { return !!placeRow(); }, 5000, 'no Runs in row in the chip’s menu');
  };
  var closeMenu = function () {
    if (!menu()) return Promise.resolve();
    chip().click();
    return until(function () { return !menu(); }, 3000, 'the menu stays open');
  };
  var pickPlace = function (label) {
    return openMenu().then(function () {
      var o = options().filter(function (x) { return titleOf(x) === label; })[0];
      if (!o) throw new Error('no ' + label + ' among the places: ' + places().join(', '));
      o.click();
      return wait(50);
    });
  };
  var enter = function (el) { el.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true })); };
  var other = acct.models.filter(function (m) { return m.id !== acct.defaultModel; })[0];

  say('resume check: ' + picked.key + ' on ' + acct.name);
  until(function () { return !!chip() && !!on(); }, 20000, 'no chip and place in the box')
    .then(function () {
      check('it goes on in a terminal unless picked otherwise, as the foot’s left says', on() === labelOf('terminal'), on());
      check('the placeholder says it resumes in the terminal', box().getAttribute('aria-placeholder').indexOf('resumes it in ' + board.apps.terminal) >= 0, box().getAttribute('aria-placeholder'));
      check('the chip says its account, then the model it goes on with', chip().textContent.indexOf(picked.account) === 0 && !!q('.ca-modelmenu-detail', chip()), chip().textContent);
      check('the chip sits beside Send', !!q('.ca-composer-send', chip().closest('.ca-composer-buttons')), 'apart');
      return openMenu();
    })
    .then(function () {
      check('its menu’s Runs in row offers the places its account’s sessions go on in', places().join(', ') === offered.map(function (p) { return p.label; }).join(', '), places().join(', '));
      check('each says what it is on hover', options().every(function (o) { return !!o.title; }), options().map(function (o) { return o.textContent + ': ' + o.title; }).join(' | '));
      check('the menu opens upward, over the conversation', menu().getBoundingClientRect().bottom <= chip().getBoundingClientRect().top + 1, menu().getBoundingClientRect().bottom + ' / ' + chip().getBoundingClientRect().top);
      return closeMenu();
    })
    .then(function () {
      // In a box as narrow as a card gets, every effort (Codex's many) and place is still in the
      // menu: the rows wrap.
      q('.ca-peek .ca-composer').style.width = '360px';
      return openMenu();
    })
    .then(function () {
      var edge = menu().getBoundingClientRect();
      var out = all('.ca-picker-foot button', menu()).filter(function (b) { var r = b.getBoundingClientRect(); return r.left < edge.left - 1 || r.right > edge.right + 1; });
      check('in a narrow box, every effort and place fits the menu', out.length === 0, out.map(function (b) { return b.textContent + ' to ' + Math.round(b.getBoundingClientRect().right) + ' of ' + Math.round(edge.right); }).join(', '));
      q('.ca-peek .ca-composer').style.width = '';
      return closeMenu();
    })
    .then(function () { return pickPlace(labelOf('background')); })
    .then(function () {
      var behind = (offered.filter(function (p) { return p.place === 'background'; })[0] || {}).behind;
      check('the menu stays up for a place, and the foot says it', !!menu() && on() === labelOf('background'), on());
      check('Background says where it goes', box().getAttribute('aria-placeholder').indexOf(behind ? 'behind your windows' : 'resumes it in the background') >= 0, box().getAttribute('aria-placeholder'));
      if (labelOf('claude-app')) {
        return pickPlace(labelOf('claude-app')).then(function () {
          check('the Claude app picks its own model, so there are none to pick', models().length === 0, models().map(titleOf).join(', '));
          check('the Claude app takes it on the clipboard', box().getAttribute('aria-placeholder').indexOf('Claude app opens it') >= 0, box().getAttribute('aria-placeholder'));
          return pickPlace(labelOf('background'));
        });
      }
    })
    .then(openMenu)
    .then(function () {
      var opt = models().filter(function (b) { return b.querySelector('.ca-picker-title').textContent.trim() === other.label.split(':')[0].replace(/ \(1M context\)$/, ' 1M'); })[0];
      check('the menu lists the account’s models', !!opt, models().map(function (b) { return b.textContent; }).join(', '));
      opt.click();
      return wait(50);
    })
    .then(function () {
      check('the menu closes on a model', !menu(), 'open');
      return openMenu();
    })
    .then(function () {
      check('a model leaves only the places that take one', places().indexOf('Claude app') < 0 && on() === labelOf('background'), places().join(', ') + ' · ' + on());
      return closeMenu();
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
