// Checks the countdown to a meeting before its offer (meetings.rs `counting_down`, ui/Meeting.tsx
// `MeetingSoon`): from `[voice] meeting_countdown` minutes before it starts, the minutes to go
// show quietly, with nothing opening by itself, and a click on them offers it then. Docked, the
// minutes are beside the camera ahead of the counts, the closed shape as much wider on both
// hands, and the click opens its offer under the notch, which the pointer leaving puts away; in
// the floating bar it's a pill with the meeting's name, which a click turns into the offer's
// line; in the main window the Meetings button has the minutes, and its dialog opens on it. Take
// notes on an offer asked for early takes it, and the countdown goes when the board has none.
//   PREVIEW_SIZE=1512,720 PREVIEW_WAIT=60000 scripts/preview.sh notch '' scripts/preview/voice-meeting-soon-check.js
//   PREVIEW_WAIT=60000 scripts/preview.sh bar '' scripts/preview/voice-meeting-soon-check.js
//   PREVIEW_WAIT=60000 scripts/preview.sh sessions '' scripts/preview/voice-meeting-soon-check.js
(function () {
  // This runs before the app's script: the board and the bar's feed it gets have the meeting.
  window.__PREVIEW_VOICE__ = true;
  var ID = 'E2@1794330000000';
  window.__MEETING_SOON__ = { id: ID, title: 'Design review', start: Date.now() + 4 * 60e3 - 20e3, end: Date.now() + 34 * 60e3, calendar: 'Work', attendees: ['Dana Kestrel'] };
  var where = function () { return q('.ca-notch') ? 'notch' : q('.ca-hud') ? 'bar' : q('.ca-win-bar') ? 'window' : null; };
  var countdown = function () {
    if (where() === 'notch') return q('.ca-notch-side.is-right .ca-meeting-soon.is-notch');
    if (where() === 'bar') return q('.ca-hud > .ca-meeting-soon.is-pill');
    return q('.ca-win-bar button[aria-label="Meetings"]');
  };
  var offer = function () { return where() === 'notch' ? q('.ca-notch-session .ca-meeting.is-card') : q('.ca-hud > .ca-meeting.is-line'); };
  var button = function (word) { return all('button', offer()).filter(function (b) { return b.textContent.trim() === word || b.getAttribute('aria-label') === word; })[0]; };
  var asked = function () { return calls('meeting').map(function (c) { return c[1]; }); };
  // What the engine says now, to this window as it hears of it.
  var again = function (soon) {
    window.__MEETING_SOON__ = soon;
    var feed = where() === 'window' ? ['get_board', 'board'] : ['bar_feed', 'bar-feed'];
    return window.__TAURI_INTERNALS__.invoke(feed[0]).then(function (now) {
      if (!soon) delete now.meetingSoon;
      window.__emit(feed[1], now);
      return wait(250);
    });
  };

  var docked = function () {
    var shape = last('notch_shape');
    var notch = q('.ca-notch');
    var wide = shape.w;
    check('closed, the minutes to go are beside the camera, ahead of the counts', countdown().textContent.trim() === '4m' && countdown().parentElement.classList.contains('ca-notch-soon') && countdown().parentElement === q('.ca-notch-side.is-right .ca-notch-side-in').firstElementChild, countdown().textContent);
    check('with nothing open under the notch', !notch.classList.contains('is-open') && !q('.ca-notch-session .ca-meeting'), notch.className);
    // The closed shape is the notch and its sides, as much wider on both hands as the countdown
    // takes, so its minutes and the first count both show inside it.
    var r = countdown().getBoundingClientRect();
    check('the closed shape has room for it, and stays centered on the camera', r.left >= shape.x && r.right <= shape.x + shape.w && wide > 185 + 2 * 44, JSON.stringify({ countdown: [Math.round(r.left), Math.round(r.right)], shape: shape }));
    var count = q('.ca-notch-side.is-right .ca-notch-count');
    if (count) {
      var c = count.getBoundingClientRect();
      check('and the first count still shows inside it', c.right <= shape.x + shape.w + 1, JSON.stringify({ count: [Math.round(c.left), Math.round(c.right)], shape: shape }));
    }
    check('its tooltip says which meeting and when', (countdown().getAttribute('title') || '').indexOf('Design review, in 4 min, with Dana Kestrel') === 0, countdown().getAttribute('title'));
    // Closed, the notch keeps the pointer a press begins on it, to drag it to another screen,
    // which sends the click to the notch: never a press on the countdown, whose click it'd take.
    var kept = [];
    var keep = Element.prototype.setPointerCapture;
    Element.prototype.setPointerCapture = function (id) { kept.push(this); return keep.call(this, id); };
    var press = function (el) {
      el.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, button: 0, buttons: 1, pointerId: 1 }));
      el.dispatchEvent(new PointerEvent('pointerup', { bubbles: true, button: 0, pointerId: 1 }));
    };
    press(countdown());
    var onCountdown = kept.length;
    press(q('.ca-notch-cam'));
    Element.prototype.setPointerCapture = keep;
    check('closed, a press on it isn’t kept by the notch, as one elsewhere on it is', onCountdown === 0 && kept.length === 1, JSON.stringify({ onCountdown: onCountdown, all: kept.length }));
    window.__emit('notch-pointer', true);
    countdown().click();
    return until(function () { return offer() && q('.ca-notch').classList.contains('is-end'); }, 3000, 'its offer under the notch')
      .then(function () {
        check('a click on it offers the meeting under the notch, without taking the keyboard', q('.ca-meeting-title', offer()).textContent === 'Design review' && !!button('Take notes') && calls('notch_keys').filter(function (c) { return c[1].on; }).length === 0);
        // The pointer leaving puts an offer asked for early away, as a session without the
        // keyboard goes.
        window.__emit('notch-pointer', false);
        return until(function () { return !offer(); }, 3000, 'the early offer to go with the pointer');
      })
      .then(function () {
        check('the pointer leaving puts it away, the countdown still there', !offer() && !!countdown());
        window.__emit('notch-pointer', true);
        countdown().click();
        return until(function () { return offer(); }, 3000, 'its offer again');
      });
  };

  var floating = function () {
    check('the bar counts down to it as a pill with its name', q('.ca-meeting-title', countdown()).textContent === 'Design review' && q('.ca-meeting-when', countdown()).textContent === 'in 4 min', countdown().textContent);
    check('ahead of the other pills, and with no offer yet', countdown() === q('.ca-hud > .ca-meeting-soon, .ca-hud > .ca-hud-pill, .ca-hud > .ca-meeting') && !offer());
    countdown().click();
    return until(function () { return offer(); }, 3000, 'its offer in the bar').then(function () {
      check('a click on it turns it into the offer’s line', !countdown() && q('.ca-meeting-title', offer()).textContent === 'Design review' && !!button('Take notes') && !!button('Not this one'));
    });
  };

  var windowed = function () {
    var opens = countdown();
    check('the main window’s Meetings button has the minutes to go', !!opens && opens.textContent.trim() === '4m' && !opens.classList.contains('is-icon'), opens && opens.textContent);
    check('and says which meeting, and where to take it', (opens.getAttribute('title') || '').indexOf('Design review, in 4 min: take its notes from Meetings') === 0, opens.getAttribute('title'));
    check('nothing is offered beside the notices before its time', !q('.ca-usagebar .ca-meeting'));
    return again(null).then(function () {
      check('with none counted down to, it’s the calendar alone', countdown().textContent.trim() === '' && countdown().classList.contains('is-icon'), countdown().textContent);
    });
  };

  say('voice meeting soon check: waiting for the countdown');
  until(function () { return where() && countdown() && (where() !== 'notch' || last('notch_shape')); }, 30000, 'the countdown')
    .then(function () { return wait(400); })
    .then(function () {
      if (where() === 'window') return windowed().then(function () { return 'done'; });
      return (where() === 'notch' ? docked() : floating()).then(function () {
        var before = asked().length;
        button('Take notes').click();
        return until(function () { return asked().length > before; }, 3000, 'the engine asked to take it');
      });
    })
    .then(function (was) {
      if (was === 'done') return null;
      var last = asked()[asked().length - 1];
      check('Take notes on an offer asked for early takes that meeting', last.id === ID && last.take === true, JSON.stringify(last));
      // Taken, the engine counts down to it no more, and has it on offer as being taken.
      window.__MEETING__ = Object.assign({}, window.__MEETING_SOON__, { taking: true });
      return again(null).then(function () {
        check('once taken, the countdown is gone and its offer says it’s starting', !countdown() && !!offer() && !!button('Starting…'), offer() ? offer().textContent : 'no offer');
      });
    })
    .then(done)
    .catch(stopped);
})();
