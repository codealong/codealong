// Checks a session's box in the notch (NotchSession.tsx, SessionReply.tsx) offers what the peek's
// does: its account, model and effort as one chip beside Send (SessionAgent.tsx), for one that isn't
// running with where it goes on in its menu and at the foot's left, and for one that's running
// the model it's on, each inside the card's width.
//   PREVIEW_SIZE=1512,980 PREVIEW_WAIT=60000 scripts/preview.sh notch '' scripts/preview/notch-model-check.js
(function () {
  var json = function (url) {
    var xhr = new XMLHttpRequest();
    xhr.open('GET', url, false);
    xhr.send();
    return JSON.parse(xhr.responseText);
  };
  var board = json('./board.json');
  var accounts = json('./start-options.json').accounts;
  var account = function (s) { return accounts.filter(function (a) { return a.name === s.account && a.harness === s.harness; })[0]; };
  var f = json('./bar-feed.json');
  var onBar = {};
  f.items.forEach(function (i) { onBar[i.key] = i; });
  var resting = null;
  var running = null;
  board.lanes.forEach(function (l) {
    l.sessions.forEach(function (s) {
      var a = account(s);
      if (!onBar[s.key] || l.hidden || l.device || s.archived || s.visitor || !a) return;
      if (!resting && !s.live && s.surface === 'cli' && (a.resumes || []).length > 1) resting = s;
      if (!running && s.live && s.model && s.surface === 'cli' && !s.attachCommand && !s.hosted && s.status !== 'working' && a.switchesModel) running = s;
    });
  });
  if (!resting || !running) { stopped('the bar has no session that isn’t running and one that is, between turns'); return; }
  // Only these two on the bar, each a pill of its own.
  var items = [resting, running].map(function (s) { return Object.assign({}, onBar[s.key], { group: 'needs', status: 'needs', asks: true, snoozedUntil: null, hidden: false }); });
  var feed = Object.assign({}, f, { loading: false, items: items, recent: [], snoozed: [], reviews: [], todos: [] });

  var notch = function () { return q('.ca-notch'); };
  var is = function (state) { return notch() && notch().classList.contains('is-' + state); };
  var pill = function (key) { return q('.ca-notch-body .ca-hud-pill[data-key="' + key + '"]'); };
  var card = function () { return q('.ca-notch-session'); };
  var inside = function (el) { var a = el.getBoundingClientRect(); var b = card().getBoundingClientRect(); return a.left >= b.left - 1 && a.right <= b.right + 1; };

  say('notch model check: ' + resting.key + ', ' + running.key);
  until(function () { return notch() && calls('notch_shape').length > 0; }, 15000, 'the notch')
    .then(function () {
      window.__emit('bar-feed', feed);
      q('.ca-notch-shape').dispatchEvent(new PointerEvent('pointerover', { bubbles: true }));
      return until(function () { return is('pills') && pill(resting.key); }, 5000, 'its pills');
    })
    .then(function () {
      pill(resting.key).click();
      return until(function () { return !!q('.ca-notch-session .ca-composer-buttons .ca-modelmenu-chip') && !!q('.ca-notch-session .ca-boxplace'); }, 8000, 'no chip and place in the notch');
    })
    .then(function () {
      var place = q('.ca-notch-session .ca-boxplace');
      var chip = q('.ca-notch-session .ca-composer-buttons .ca-modelmenu-chip');
      check('one that isn’t running says where it goes on, at the foot’s left', place.textContent.trim().length > 0, place.textContent);
      check('and its account and what it goes on with beside Send', chip.textContent.indexOf(resting.account) === 0, chip.textContent);
      check('the place fits the card', inside(place), JSON.stringify(place.getBoundingClientRect()));
      check('so does the chip', inside(chip), JSON.stringify(chip.getBoundingClientRect()));
      chip.click();
      return until(function () { return all('.ca-notch-session .ca-picker-foot-label').some(function (l) { return l.textContent === 'Runs in'; }); }, 5000, 'no Runs in row in its menu');
    })
    .then(function () {
      var menu = q('.ca-notch-session .ca-modelmenu .ca-picker');
      check('its menu has where it goes on, and fits the card', inside(menu), JSON.stringify(menu.getBoundingClientRect()));
      q('.ca-notch-session .ca-composer-buttons .ca-modelmenu-chip').click();
      pill(running.key).click();
      // The one that is running has its chip beside Send too, with no place to pick in its menu.
      return until(function () { var c = q('.ca-notch-session .ca-composer-buttons .ca-modelmenu-chip'); return !!c && c.textContent.indexOf(running.account) === 0; }, 8000, 'no chip for the running one in the notch');
    })
    .then(function () {
      var chip = q('.ca-notch-session .ca-composer-buttons .ca-modelmenu-chip');
      check('one that’s running says its model beside Send, after its account', !!q('.ca-modelmenu-detail', chip), chip.textContent);
      check('the chip fits the card', inside(chip), JSON.stringify(chip.getBoundingClientRect()));
    })
    .then(done, stopped);
})();
