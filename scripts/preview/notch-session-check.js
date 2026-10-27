// Checks the session the bar docked in the notch shows under its pills (NotchSession.tsx,
// Notch.tsx): hovering a pill opens no peek; a click on it opens the notch on its session, with
// the keyboard and the cursor in its box, the window never resized; its pill is marked; ⌘] goes
// on to the next that waits on you and ⌥⇧↑ back, ⌥↓ steps along the bar, ⌘E marks seen and goes
// on, ⌃- and ⌃⇧- go back and forward through the ones looked at; Escape puts it away and gives the keyboard back; a click on its pill again does too; one
// that lost the keyboard stays while the pointer is on it and goes when it leaves, or at once
// with the pointer elsewhere; a double-click opens the session where it lives; and a count beside
// the camera shows its group's first, and a click on the notch itself the current one; the pin in its header keeps it on the desktop; and Rename…
// in a pill's menu shows it with its name ready to change. The strip over its header has the
// common actions with their keys, ⌃⌘W closes its terminal, and its bottom corner drags it to a
// size it keeps. ⌃⌘1 snoozes it.
//   PREVIEW_SIZE=1512,980 PREVIEW_WAIT=150000 scripts/preview.sh notch '' scripts/preview/notch-session-check.js
(function () {
  var calls = sent;
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
  // turn first (SessionReply.tsx).
  var escape = function () { window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', cancelable: true })); };
  var enter = function () { shape().dispatchEvent(new PointerEvent('pointerover', { bubbles: true })); };
  var leave = function () { shape().dispatchEvent(new PointerEvent('pointerout', { bubbles: true, relatedTarget: document.body })); };

  // Three of the board's sessions wait on you and one runs, whatever the board has now: the
  // feed's first four that are this Mac's own sessions on the board, said to be so. This runs
  // before the app's script, so both files are read here, at once.
  var json = function (url) {
    var xhr = new XMLHttpRequest();
    xhr.open('GET', url, false);
    xhr.send();
    return JSON.parse(xhr.responseText);
  };
  var onBoard = {};
  json('./board.json').lanes.forEach(function (l) {
    l.sessions.forEach(function (s) { if (!s.visitor && !s.archived && s.key.charAt(0) !== '@') onBoard[s.key] = true; });
  });
  var f = json('./bar-feed.json');
  var items = f.items.filter(function (i) { return onBoard[i.key]; }).slice(0, 4).map(function (i, n) {
    var group = n === 0 ? 'needs' : n < 3 ? 'unread' : 'working';
    // On the bar whatever its worktree: one the board keeps out of sight isn't. Only the one
    // that waits on a call to allow isn't a reply that asks you something.
    return Object.assign({}, i, { group: group, status: group, asks: false, snoozedUntil: null, since: 1000 + n, hidden: false });
  });
  // Only these on the bar, so the notch is as narrow as a session gets.
  var feed = Promise.resolve(Object.assign({}, f, { loading: false, items: items, recent: [], snoozed: [], reviews: [], todos: [] }));
  // The card under the pills reads its session from the board (session_at), not from its pill:
  // each says there what its pill says, and each runs in a terminal of its own here, which ⌃⌘W
  // closes. What the card does with a key goes by this, so it mustn't be whatever the sessions
  // happened to be doing when the board was read.
  var said = {};
  items.forEach(function (i) {
    said[i.key] = { title: i.title, status: i.status, asks: false, snoozedUntil: null, archived: false, live: true, surface: 'cli', hosted: false, pid: 4242, attachCommand: null };
  });
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd) {
    var p = real.apply(this, arguments);
    // A step can hold back the board's answer for the session a pill asks for (session_at).
    if (cmd === 'session_at' && window.__holdSessionAt) return window.__holdSessionAt.then(function () { return p; });
    if (cmd !== 'get_board') return p;
    return p.then(function (b) {
      b.lanes.forEach(function (l) {
        l.sessions = l.sessions.map(function (s) { return said[s.key] && !s.visitor ? Object.assign({}, s, said[s.key]) : s; });
      });
      return b;
    });
  };
  // The name each card shows, which Rename… changes for the first.
  var names = {};
  items.forEach(function (i) { names[i.key] = i.title; });
  // The card shows the session `key` itself, not only its pill marked: going from one to another,
  // the card keeps the one before until the board's answer for the next comes (NotchSession.tsx),
  // and its keys are that one's until then.
  var showing = function (key) { return is('session') && !!pill(key) && pill(key).classList.contains('is-peeked') && title() === names[key]; };

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
      check('the strip counts nothing of what’s left', !q('.ca-reply-queue .ca-chip'), (q('.ca-reply-queue') || {}).textContent);
      // At the narrowest a session gets, what it says and every button share one line, what it
      // says cut short there rather than put the buttons on a line of their own.
      var note = strip.querySelector('.ca-reply-queue-note').getBoundingClientRect();
      var keysBox = strip.querySelector('.ca-hotkeys').getBoundingClientRect();
      var lastKey = strip.querySelector('.ca-hotkey:last-child').getBoundingClientRect();
      check('the strip is one line', strip.getBoundingClientRect().height < 34 && Math.abs(note.top + note.height / 2 - (keysBox.top + keysBox.height / 2)) < 4 && lastKey.right <= strip.getBoundingClientRect().right, 'strip ' + Math.round(strip.getBoundingClientRect().width) + ' by ' + Math.round(strip.getBoundingClientRect().height) + ', saying ' + Math.round(note.width) + ' wide');
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
      var labels = Array.prototype.map.call(document.querySelectorAll('.ca-notch-session .ca-hotkeys .ca-hotkey:not(.is-more)'), function (b) { return b.textContent; });
      check('the strip has the common actions with their keys', labels.join('|') === 'Seen⌘E|Done⇧⌘E|Snooze⌃⌘1–6|' + labels[3] + '|Archive⌃⌘A|Back⌃-|Next⌘]|esc' && /⌃⌘W$/.test(labels[3]), labels.join('|'));
      // Nothing to mark seen on one that runs: ⌘E goes on as ⌘] does.
      key({ key: 'e', metaKey: true });
      return until(shows(items[0]), 3000, '⌘E from one running');
    })
    .then(function () {
      check('⌘E from one that runs goes on to the first that waits, and marks nothing seen', calls('mark_seen').length === 0, JSON.stringify(calls('mark_seen')));
      key({ key: 'ArrowDown', altKey: true });
      return until(shows(items[1]), 3000, 'back along the bar');
    })
    .then(function () {
      key({ key: 'ArrowDown', altKey: true });
      return until(shows(items[2]), 3000, 'back along the bar');
    })
    .then(function () {
      key({ key: 'ArrowDown', altKey: true });
      return until(shows(items[3]), 3000, 'back to the one running');
    })
    .then(function () {
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
      rightClick(pill(items[0].key));
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
      names[items[0].key] = 'Renamed in the notch';
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
      return wait(500);
    })
    .then(function () {
      // Its bottom corner drags it to a size, as much wider on the other side, in the window it has.
      shape().click();
      return until(function () { return is('session'); }, 3000, 'the session, to size');
    })
    .then(function () { return wait(900); })
    .then(function () {
      var before = lastShape();
      var fitsBefore = fits().length;
      var g = document.querySelector('.ca-notch-grip.is-right');
      var at = function (type, x, y, buttons) { g.dispatchEvent(new PointerEvent(type, { bubbles: true, cancelable: true, button: type === 'pointerdown' ? 0 : -1, buttons: buttons, screenX: x, screenY: y })); };
      at('pointerdown', 500, 500, 1);
      at('pointermove', 530, 520, 1);
      at('pointermove', 560, 550, 1);
      return wait(300).then(function () {
        var s = lastShape();
        check('a bottom corner drags it bigger, as much on the other side', s.w === before.w + 120 && s.h === before.h + 50 && s.x === before.x - 60, JSON.stringify(before) + ' to ' + JSON.stringify(s));
        check('with no easing to catch up', notch().classList.contains('is-sizing'));
        at('pointerup', 560, 550, 0);
        return wait(200);
      }).then(function () {
        var kept = JSON.parse(localStorage.getItem('codealong.notch.size') || 'null');
        check('and it keeps the size', !!kept && kept.w === before.w + 120 && !notch().classList.contains('is-sizing'), JSON.stringify(kept));
        check('in the window it has', fits().length === fitsBefore, fits().slice(fitsBefore).join(' '));
        at('pointerdown', 560, 550, 1);
        at('pointermove', 3000, 3000, 1);
        return wait(300);
      }).then(function () {
        var s = lastShape();
        var win = fits()[fits().length - 1].split('x').map(Number);
        check('no bigger than its window has room for', s.w <= win[0] - 40 && s.h <= win[1] - 32 && s.w > before.w + 120, JSON.stringify(s) + ' in ' + win.join('x'));
        at('pointerup', 3000, 3000, 0);
        g.dispatchEvent(new MouseEvent('dblclick', { bubbles: true }));
        return wait(700);
      }).then(function () {
        var s = lastShape();
        check('a double-click on the corner gives it its usual size back', s.w === before.w && s.h === before.h && localStorage.getItem('codealong.notch.size') === null, JSON.stringify(s));
      });
    })
    .then(function () { return wait(600); })
    .then(function () {
      // The strip's buttons: back to the one looked at before, and away.
      key({ key: 'ArrowDown', altKey: true });
      return until(function () { return is('session') && pill(items[2].key) && pill(items[2].key).classList.contains('is-peeked'); }, 3000, 'the next along, to go back from');
    })
    .then(function () { return wait(1200); })
    .then(function () {
      var back = Array.prototype.filter.call(document.querySelectorAll('.ca-notch-session .ca-hotkey'), function (b) { return /^Back/.test(b.textContent); })[0];
      check('the strip’s Back is there to press', !!back && !back.disabled);
      if (back) back.click();
      return until(function () { return showing(items[0].key); }, 3000, 'Back on the strip');
    })
    .then(function () {
      check('the strip’s Back goes to the one looked at before', true);
      var close = Array.prototype.filter.call(document.querySelectorAll('.ca-notch-session .ca-hotkey'), function (b) { return /^Close⌃⌘W/.test(b.textContent); })[0];
      check('the strip’s Close is there to press', !!close && !close.disabled, close ? 'disabled' : 'no button');
      var closes = calls('close_session').length;
      key({ key: 'w', metaKey: true, ctrlKey: true });
      return until(function () { return calls('close_session').length === closes + 1; }, 2000, '⌃⌘W').then(function () {
        var c = calls('close_session');
        check('⌃⌘W closes its terminal', c[c.length - 1].key === items[0].key, JSON.stringify(c[c.length - 1]));
        return until(function () { return is('session') && pill(items[2].key) && pill(items[2].key).classList.contains('is-peeked'); }, 3000, 'on to the next after closing').then(function () { check('and goes on to the next that waits', true); });
      });
    })
    .then(function () { return until(function () { return showing(items[2].key); }, 3000, 'the card for the one gone on to'); })
    .then(function () {
      // Going to another while the board hasn't answered for it yet, the card still shows the one
      // before: a key pressed then does nothing, where it would act on the session you just left.
      var release;
      window.__holdSessionAt = new Promise(function (r) { release = r; });
      var closes = calls('close_session').length;
      var seen = calls('mark_seen').length;
      // Another of the check's sessions still on the bar (the ones dealt with have left it).
      var other = items.filter(function (i) { return i.key !== items[2].key && pill(i.key); })[0];
      if (!other) throw new Error('no other of the check’s sessions left on the bar');
      pill(other.key).click();
      return until(function () { return pill(other.key).classList.contains('is-peeked'); }, 3000, 'the other pill marked')
        .then(function () {
          key({ key: 'w', metaKey: true, ctrlKey: true });
          key({ key: 'e', metaKey: true });
          return wait(600);
        })
        .then(function () {
          check('a key pressed before the next card comes does nothing to the one before', calls('close_session').length === closes && calls('mark_seen').length === seen && title() === names[items[2].key], JSON.stringify(calls('close_session').slice(closes)) + ' ' + title());
          window.__holdSessionAt = null;
          release();
          return until(function () { return showing(other.key); }, 3000, 'the other card, once the board answers');
        })
        .then(function () { check('and the next card comes once the board answers', true); });
    })
    .then(function () {
      var away = Array.prototype.filter.call(document.querySelectorAll('.ca-notch-session .ca-hotkey'), function (b) { return /esc$/.test(b.textContent); })[0];
      if (away) away.click();
      return until(function () { return !is('session'); }, 2000, 'the strip’s ×');
    })
    .then(function () {
      check('the strip’s × puts it away', keysCalls()[keysCalls().length - 1] === false);
      return wait(500);
    })
    .then(function () {
      // ⌃⌘1 snoozes the one shown for the first of its menu's times, and it leaves what waits.
      shape().click();
      return until(function () { var p = document.querySelector('.ca-hud-pill.is-peeked[data-key]'); return p && showing(p.dataset.key); }, 3000, 'a session, to snooze');
    })
    .then(function () { return wait(400); })
    .then(function () {
      var shown = (window.__snoozed = document.querySelector('.ca-hud-pill.is-peeked[data-key]').dataset.key);
      var before = calls('snooze_sessions').length;
      var at = Date.now();
      key({ key: '1', code: 'Digit1', metaKey: true, ctrlKey: true });
      return until(function () { return calls('snooze_sessions').length === before + 1; }, 2000, '⌃⌘1').then(function () {
        var c = calls('snooze_sessions')[before];
        var mins = (c.until - at) / 60000;
        check('⌃⌘1 snoozes it for 15 minutes', c.keys.join() === shown && mins > 14.5 && mins < 15.5, JSON.stringify(c));
        return wait(400);
      });
    })
    .then(function () {
      var now = document.querySelector('.ca-hud-pill.is-peeked[data-key]');
      check('and the notch goes on from it, or closes with none left', !is('session') || (!!now && now.dataset.key !== window.__snoozed), notch().className);
      escape();
      return wait(300);
    })
    .then(done)
    .catch(function (e) { stopped(e.message + ' (showing “' + title() + '”, ' + notch().className + ')'); });
})();
