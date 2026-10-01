// Checks the session the bar docked in the notch shows under its pills (NotchSession.tsx,
// Notch.tsx): hovering a pill opens no peek; a click on it opens the notch on its session, with
// the keyboard and the cursor in its box, the window never resized; its pill is marked; ⌘] goes
// on to the next that waits on you and ⌥⇧↑ back, ⌥↓ steps along the bar, ⌘E marks seen and goes
// on, ⌃- and ⌃⇧- go back and forward through the ones looked at; Escape puts it away and gives the keyboard back; a click on its pill again does too; one
// that lost the keyboard stays while the pointer is on it and goes when it leaves, or at once
// with the pointer elsewhere; a double-click opens the session where it lives; and a count beside
// the camera shows its group's first, and a click on the notch itself the current one; the pin in its header keeps it on the desktop; and Rename…
// in a pill's menu shows it with its name ready to change.
//   PREVIEW_SIZE=1512,720 PREVIEW_WAIT=60000 scripts/preview.sh notch '' scripts/preview/notch-session-check.js
(function () {
  var out = [];
  var say = function (t) { window.__t = t; };
  setInterval(function () { if (window.__t) document.title = window.__t; }, 100);
  var wait = function (ms) { return new Promise(function (r) { setTimeout(r, ms); }); };
  var until = function (f, ms, what) {
    var s = Date.now();
    return new Promise(function (res, rej) {
      (function p() { if (f()) return res(); if (Date.now() - s > ms) return rej(new Error('timeout: ' + what)); setTimeout(p, 10); })();
    });
  };
  var check = function (name, ok, why) { out.push(name + (ok ? ' ok' : ' FAIL') + (why ? ' (' + why + ')' : '')); };
  var calls = function (cmd) { return window.__calls.filter(function (c) { return c[0] === cmd; }).map(function (c) { return c[1]; }); };
  var notch = function () { return document.querySelector('.ca-notch'); };
  var shape = function () { return document.querySelector('.ca-notch-shape'); };
  var is = function (state) { return notch() && notch().classList.contains('is-' + state); };
  var pill = function (key) { return document.querySelector('.ca-notch-body .ca-hud-pill[data-key="' + key + '"]'); };
  var title = function () { var t = document.querySelector('.ca-notch-session .ca-sessinsp-title, .ca-notch-session .ca-sessinsp-head h2'); return t ? t.textContent.trim() : ''; };
  var shows = function (item) { return function () { return is('session') && title().indexOf(item.title.slice(0, 20)) === 0; }; };
  var keysCalls = function () { return calls('notch_keys').map(function (a) { return a.on; }); };
  var lastShape = function () { var s = calls('notch_shape'); return s[s.length - 1]; };
  var fits = function () { return calls('fit_bar').map(function (a) { return a.width + 'x' + a.height; }); };
  var key = function (init) { (document.activeElement || document.body).dispatchEvent(new KeyboardEvent('keydown', Object.assign({ bubbles: true, cancelable: true }, init))); };
  // Escape as the window hears it: in the box of a session whose turn is running, it stops the
  // turn first (SessionReply.tsx), and the board's sessions are whatever they are today.
  var escape = function () { window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', cancelable: true })); };
  var enter = function () { shape().dispatchEvent(new PointerEvent('pointerover', { bubbles: true })); };
  var leave = function () { shape().dispatchEvent(new PointerEvent('pointerout', { bubbles: true, relatedTarget: document.body })); };

  // Three of the board's sessions wait on you and one runs, whatever the board has now: the
  // feed's own first four, said to be so.
  var items;
  var feed = fetch('./bar-feed.json').then(function (r) { return r.json(); }).then(function (f) {
    items = f.items.slice(0, 4).map(function (i, n) {
      var group = n === 0 ? 'needs' : n < 3 ? 'unread' : 'working';
      return Object.assign({}, i, { group: group, status: group, snoozedUntil: null, since: 1000 + n });
    });
    return Object.assign({}, f, { loading: false, items: items, recent: [], snoozed: [] });
  });

  say('notch session check: waiting for it');
  var windowAtStart;
  var fitsAtStart;
  until(function () { return notch() && calls('notch_shape').length > 0; }, 15000, 'the notch')
    .then(function () { return feed; })
    .then(function (f) {
      check('has four sessions to try', items.length === 4, String(items.length));
      window.__emit('bar-feed', f);
      enter();
      return until(function () { return is('pills') && pill(items[0].key); }, 3000, 'its pills');
    })
    .then(function () { return wait(500); })
    .then(function () {
      windowAtStart = fits()[fits().length - 1];
      fitsAtStart = fits().length;
      var p = pill(items[1].key);
      p.dispatchEvent(new PointerEvent('pointerover', { bubbles: true, screenX: 10, screenY: 10 }));
      p.dispatchEvent(new PointerEvent('pointermove', { bubbles: true, screenX: 12, screenY: 10 }));
      return wait(600);
    })
    .then(function () {
      check('hovering a pill opens no peek', calls('peek_open').length === 0 && !is('session'));
      check('a pill says the rest in a tooltip', !!pill(items[1].key).title);
      pill(items[0].key).click();
      return until(shows(items[0]), 3000, 'the session');
    })
    .then(function () { return wait(700); })
    .then(function () {
      check('a click opens the notch on the session', is('session') && is('open'), title());
      check('takes the keyboard', keysCalls().join() === 'true', keysCalls().join());
      check('the cursor is in its box', document.activeElement && document.activeElement.tagName === 'TEXTAREA');
      check('its pill is marked', pill(items[0].key).classList.contains('is-peeked') && pill(items[0].key).getAttribute('aria-pressed') === 'true');
      check('never resizes its window', fits().slice(fitsAtStart).every(function (f) { return f === windowAtStart; }), windowAtStart + ' then ' + fits().slice(fitsAtStart).join(' '));
      var s = lastShape();
      var card = document.querySelector('.ca-notch-session').getBoundingClientRect();
      check('says the shape with the session in it', Math.abs(s.h - card.bottom) < 1 && s.w >= 560, JSON.stringify(s) + ' card to ' + card.bottom);
      check('the window has room for it and its shadow', Number(windowAtStart.split('x')[1]) >= s.h + 30, windowAtStart);
      var strip = document.querySelector('.ca-notch-session .ca-reply-queue');
      check('says how many more wait', !!strip && /2 more waiting on you/.test(strip.textContent), strip ? strip.textContent : 'no strip');
      key({ key: ']', code: 'BracketRight', metaKey: true });
      return until(shows(items[1]), 3000, '⌘] to the next');
    })
    .then(function () {
      check('⌘] goes on to the next that waits', true);
      check('still has the keyboard', keysCalls().every(function (on) { return on; }), keysCalls().join());
      return wait(300);
    })
    .then(function () {
      check('the next one’s box has the cursor', document.activeElement && document.activeElement.tagName === 'TEXTAREA');
      key({ key: 'ArrowUp', altKey: true, shiftKey: true });
      return until(shows(items[0]), 3000, '⌥⇧↑ back');
    })
    .then(function () {
      check('⌥⇧↑ goes back', true);
      key({ key: 'ArrowDown', altKey: true });
      return until(shows(items[1]), 3000, '⌥↓ along the bar');
    })
    .then(function () {
      key({ key: 'ArrowDown', altKey: true });
      return until(shows(items[2]), 3000, '⌥↓ again');
    })
    .then(function () {
      key({ key: 'ArrowDown', altKey: true });
      return until(shows(items[3]), 3000, '⌥↓ to the one running');
    })
    .then(function () {
      check('⌥↓ steps along the bar, to the ones running too', true);
      check('no strip on one that doesn’t wait', !document.querySelector('.ca-notch-session .ca-reply-queue'));
      key({ key: ']', code: 'BracketRight', metaKey: true });
      return until(shows(items[0]), 3000, '⌘] from one running');
    })
    .then(function () {
      check('⌘] from one that runs goes to the first that waits', true);
      key({ key: 'ArrowDown', altKey: true });
      return until(shows(items[1]), 3000, 'the second');
    })
    .then(function () {
      key({ key: 'e', metaKey: true });
      return until(shows(items[2]), 3000, '⌘E on to the next');
    })
    .then(function () {
      var seen = calls('mark_seen');
      check('⌘E marks it seen and goes on', seen.length === 1 && seen[0].keys.join() === items[1].key, JSON.stringify(seen));
      var strip = document.querySelector('.ca-notch-session .ca-reply-queue');
      check('the strip says how it went', !!strip && /marked seen/.test(strip.textContent), strip ? strip.textContent : 'no strip');
      leave();
      return wait(900);
    })
    .then(function () {
      check('with the keyboard it stays with the pointer gone', is('session'));
      escape();
      return until(function () { return !is('session'); }, 2000, 'Escape');
    })
    .then(function () { return wait(100); })
    .then(function () {
      check('Escape puts it away and gives the keyboard back', keysCalls()[keysCalls().length - 1] === false, keysCalls().join());
      check('with the pointer elsewhere it closes in one move', is('closed'), notch().className);
      return wait(600);
    })
    .then(function () {
      check('its card goes once it has closed', !document.querySelector('.ca-notch-session .ca-notch-card'));
      check('says its closed shape', lastShape().h === parseFloat(getComputedStyle(notch()).getPropertyValue('--notch-h')), JSON.stringify(lastShape()));
      enter();
      return until(function () { return is('pills'); }, 2000, 'the pills again');
    })
    .then(function () {
      pill(items[2].key).click();
      return until(shows(items[2]), 3000, 'the third');
    })
    .then(function () {
      pill(items[2].key).click();
      return until(function () { return !is('session'); }, 2000, 'a click on its pill');
    })
    .then(function () {
      check('a click on its pill again puts it away, to the pills under the pointer', is('pills'), notch().className);
      pill(items[0].key).click();
      return until(shows(items[0]), 3000, 'the first again');
    })
    .then(function () {
      // Another app took the keyboard with the pointer still on the notch.
      window.__emit('notch-keys', false);
      return wait(700);
    })
    .then(function () {
      check('without the keyboard it stays while the pointer is on it', is('session'));
      var n = keysCalls().length;
      document.querySelector('.ca-notch-card').dispatchEvent(new PointerEvent('pointerdown', { bubbles: true }));
      return wait(100).then(function () { check('a click in it takes the keyboard back', keysCalls().length === n + 1 && keysCalls()[n] === true, keysCalls().join()); });
    })
    .then(function () {
      window.__emit('notch-keys', false);
      return wait(100);
    })
    .then(function () {
      leave();
      return until(function () { return is('closed'); }, 2000, 'leaving without the keyboard');
    })
    .then(function () {
      check('and goes a moment after the pointer leaves', true);
      return wait(500);
    })
    .then(function () {
      // A click on the notch itself, closed, with the pointer gone again before it's done.
      shape().click();
      return until(shows(items[0]), 3000, 'a click on the notch');
    })
    .then(function () {
      check('a click on the notch opens it on the one shown last, which still waits', keysCalls()[keysCalls().length - 1] === true);
      window.__emit('notch-keys', false);
      return until(function () { return is('closed'); }, 1000, 'the keyboard gone with the pointer elsewhere');
    })
    .then(function () {
      check('the keyboard gone with the pointer elsewhere, it closes at once', true);
      return wait(500);
    })
    .then(function () {
      enter();
      return until(function () { return is('pills'); }, 2000, 'the pills, for a double-click');
    })
    .then(function () {
      var p = pill(items[0].key);
      p.dispatchEvent(new MouseEvent('click', { bubbles: true, detail: 1 }));
      p.dispatchEvent(new MouseEvent('click', { bubbles: true, detail: 2 }));
      return wait(600);
    })
    .then(function () {
      var opened = calls('open_from_bar').concat(calls('open_target'));
      check('a double-click opens it where it lives, and the notch lets go', opened.length === 1 && !is('session') && keysCalls()[keysCalls().length - 1] === false, JSON.stringify(opened) + ' ' + notch().className);
      var count = document.querySelector('.ca-notch-side.is-right button.ca-notch-count.is-unread');
      check('a group’s count is a button', !!count);
      if (count) count.click();
      // The second was marked seen, so the third is the first whose turn ended.
      return until(shows(items[2]), 3000, 'the count’s first');
    })
    .then(function () {
      check('a count shows its group’s first', true);
      // Looked at for a moment, a session is a stop on the way back.
      return wait(1200);
    })
    .then(function () {
      key({ key: 'ArrowDown', altKey: true });
      return until(shows(items[3]), 3000, 'the fourth, to go back from');
    })
    .then(function () { return wait(1200); })
    .then(function () {
      key({ key: '-', code: 'Minus', ctrlKey: true });
      return until(shows(items[2]), 3000, '⌃- back');
    })
    .then(function () {
      key({ key: '_', code: 'Minus', ctrlKey: true, shiftKey: true });
      return until(shows(items[3]), 3000, '⌃⇧- forward');
    })
    .then(function () {
      check('⌃- goes back to the one looked at before, ⌃⇧- forward again', true);
      // The pin in its header keeps it on the desktop, where its card is, and the notch closes.
      var card = document.querySelector('.ca-notch-card').getBoundingClientRect();
      document.querySelector('.ca-notch-session button[aria-label="Keep on the desktop"]').click();
      return until(function () { return !is('session'); }, 2000, 'keeping it').then(function () {
        var kept = calls('keep_notch_session');
        check('its pin keeps it on the desktop where its card is', kept.length === 1 && kept[0].key === items[3].key && kept[0].w === card.width && kept[0].y === card.top, JSON.stringify(kept));
      });
    })
    .then(function () { return wait(500); })
    .then(function () {
      // Rename… in a pill's menu opens the notch on the session with its name ready to change.
      enter();
      return until(function () { return is('pills') && pill(items[0].key); }, 2000, 'the pills, to rename');
    })
    .then(function () {
      pill(items[0].key).dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true }));
      return until(function () { return window.__menu().length > 0; }, 3000, 'a pill’s menu');
    })
    .then(function () {
      var menu = window.__menu();
      var labels = menu.map(function (m) { return m[0]; });
      check('a pill’s menu has Reply here and Rename…', labels.indexOf('Reply here') >= 0 && labels.indexOf('Rename…') >= 0, labels.join('|'));
      menu.filter(function (m) { return m[0] === 'Rename…'; })[0][1]();
      return until(function () { return is('session') && document.querySelector('.ca-notch-session .ca-sessinsp-name'); }, 3000, 'the name field');
    })
    .then(function () { return wait(200); })
    .then(function () {
      var input = document.querySelector('.ca-notch-session .ca-sessinsp-name');
      check('Rename… shows the session in the notch, on its name', document.activeElement === input, document.activeElement && document.activeElement.tagName);
      Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set.call(input, 'Renamed in the notch');
      input.dispatchEvent(new Event('input', { bubbles: true }));
      input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true }));
      return until(function () { return !is('session'); }, 2000, 'renaming it');
    })
    .then(function () {
      var renamed = calls('rename_session');
      check('⏎ renames it and the notch closes', renamed.length === 1 && renamed[0].name === 'Renamed in the notch', JSON.stringify(renamed));
      return wait(200);
    })
    .then(function () {
      // A click on the notch itself, off its pills and buttons, opens it on the current session,
      // the first that waits since the one shown last runs, and a second puts it away.
      shape().click();
      // By its pill: its name is the new one now.
      return until(function () { return is('session') && pill(items[0].key) && pill(items[0].key).classList.contains('is-peeked'); }, 3000, 'a click on the notch, the pills showing');
    })
    .then(function () {
      shape().click();
      return until(function () { return !is('session'); }, 2000, 'a second click on the notch');
    })
    .then(function () {
      check('a click on the notch opens the current session, and a second puts it away', keysCalls()[keysCalls().length - 1] === false, keysCalls().join());
      return wait(200);
    })
    .then(function () { say('done: ' + out.join(' · ')); })
    .catch(function (e) { say('done: ' + out.concat(['error ' + e.message + ' (showing “' + title() + '”, ' + notch().className + ')']).join(' · ')); });
})();
