// Checks the meeting on offer (src/meetings.ts, ui/Meeting.tsx; meetings.rs in the engine): as
// one in your calendar is about to start, the window says which and when and asks whether to
// take its notes. Take notes asks the engine to take it and says it's starting, in whichever
// window asked and in the others (the offer's `taking`); a take that failed says why, and the
// offer stays; Not this one passes on it; and the offer goes when the board has none. The same
// in the main window's bar and the floating bar (one line) and under the notch's pills (a card,
// which opens the notch without taking the keyboard). In the main window, the bar's Meetings
// button opens the ones that were kept: the list, the latest first, and the one picked with its
// notes and its transcript (ui/Meetings.tsx).
//   PREVIEW_WAIT=60000 scripts/preview.sh sessions '' scripts/preview/voice-meeting-check.js
//   PREVIEW_WAIT=60000 scripts/preview.sh bar '' scripts/preview/voice-meeting-check.js
//   PREVIEW_SIZE=1512,720 PREVIEW_WAIT=60000 scripts/preview.sh notch '' scripts/preview/voice-meeting-check.js
(function () {
  // This runs before the app's script: the board and the bar's feed it gets have the meeting.
  window.__PREVIEW_VOICE__ = true;
  var soon = function (over) {
    return Object.assign({ id: 'E1@1794330000000', title: 'Renewal call', start: Date.now() + 55e3, end: Date.now() + 31 * 60e3, calendar: 'Work', attendees: ['Dana Kestrel', 'Lee Atlas'] }, over);
  };
  window.__MEETING__ = soon({});
  var where = function () { return q('.ca-notch') ? 'notch' : q('.ca-hud') ? 'bar' : q('.ca-win-bar') ? 'window' : null; };
  var offer = function () {
    return where() === 'notch' ? q('.ca-notch-session .ca-meeting.is-card') : where() === 'bar' ? q('.ca-hud > .ca-meeting.is-line') : q('.ca-usagebar .ca-meeting.is-line');
  };
  var text = function (sel) { var el = offer() && offer().querySelector(sel); return el ? el.textContent.trim() : ''; };
  var button = function (word) { return all('button', offer()).filter(function (b) { return b.textContent.trim() === word || b.getAttribute('aria-label') === word; })[0]; };
  var takes = function () { return button('Take notes') || button('Starting…'); };
  // What the engine says now, to this window as it hears of it.
  var again = function (meeting) {
    window.__MEETING__ = meeting;
    var feed = where() === 'window' ? ['get_board', 'board'] : ['bar_feed', 'bar-feed'];
    return window.__TAURI_INTERNALS__.invoke(feed[0]).then(function (now) {
      if (!meeting) delete now.meeting;
      window.__emit(feed[1], now);
      return wait(250);
    });
  };
  var asked = function () { return calls('meeting').map(function (c) { return c[1]; }); };

  // Your meetings, from the main window's bar: the list, and the one picked with its notes and
  // its transcript.
  var library = function () {
    var dialog = function () { return q('.ca-dialog.ca-meetingsdlg'); };
    var opens = q('.ca-win-bar button[aria-label="Meetings"]');
    check('with voice on, the window’s bar opens your meetings', !!opens);
    if (!opens) return null;
    opens.click();
    return until(function () { return dialog() && all('.ca-meetings-row', dialog()).length === 2 && q('.ca-meetings-body .ca-md-h1', dialog()); }, 5000, 'your meetings')
      .then(function () {
        var rows = all('.ca-meetings-row', dialog());
        check('they’re listed, the latest first, and the latest is shown', rows[0].classList.contains('is-on') && q('.ca-meetings-title', rows[0]).textContent === 'Renewal call' && q('.ca-meetings-title', rows[1]).textContent === 'Standup', rows.map(function (r) { return r.textContent; }).join(' | '));
        check('each says who was in it, and when nothing was heard', q('.ca-meetings-who', rows[0]).textContent === 'with Dana Kestrel and Lee Atlas' && q('.ca-meetings-who', rows[1]).textContent === 'with Sam Northwind · nothing heard', q('.ca-meetings-who', rows[1]).textContent);
        check('with its notes, as they were kept', q('.ca-meetings-body .ca-md-h1', dialog()).textContent === 'Renewal call' && all('.ca-meetings-body li', dialog()).length === 2);
        check('read from its own folder', calls('meeting_files')[0][1].folder === '/Users/me/Library/Application Support/Codealong/meetings/2026-11-10 1400 Renewal call', JSON.stringify(calls('meeting_files')[0]));
        all('.ca-meetings-head button', dialog()).filter(function (b) { return b.textContent.trim() === 'Transcript'; })[0].click();
        return until(function () { return all('.ca-meetings-body .ca-heard-line', dialog()).length === 2; }, 3000, 'its transcript');
      })
      .then(function () {
        var said = all('.ca-meetings-body .ca-heard-line', dialog()).map(function (l) { return q('.ca-heard-who', l).textContent; });
        check('and its transcript, as the meeting’s lines', said.join(',') === 'Them,You' && all('.ca-meetings-body .ca-heard-aside', dialog()).length === 1, said.join(','));
        all('.ca-meetings-head button', dialog()).filter(function (b) { return b.textContent.trim() === 'Its session'; })[0].click();
        return wait(200);
      })
      .then(function () {
        check('a session the board no longer has is said to be gone, and the dialog stays', !!dialog() && q('.ca-meetings-failed', dialog()).textContent.indexOf('no longer on the board') > 0);
        all('button', dialog()).filter(function (b) { return b.textContent.trim() === 'Done'; })[0].click();
        return until(function () { return !dialog(); }, 2000, 'the dialog to close');
      })
      .then(function () { check('Done closes it', true); });
  };

  say('voice meeting check: waiting for the offer');
  until(function () { return where() && offer(); }, 30000, 'the meeting on offer')
    .then(function () { return wait(300); })
    .then(function () {
      check('a meeting about to start is on offer, by name', text('.ca-meeting-title') === 'Renewal call', text('.ca-meeting-title'));
      check('with when it starts', text('.ca-meeting-when') === 'in 1 min', text('.ca-meeting-when'));
      if (where() === 'notch') {
        check('and who else is in it', text('.ca-meeting-who') === 'with Dana Kestrel and Lee Atlas', text('.ca-meeting-who'));
        check('under the notch’s pills, without taking the keyboard', q('.ca-notch').classList.contains('is-end') && calls('notch_keys').filter(function (c) { return c[1].on; }).length === 0, q('.ca-notch').className);
        // All of it inside the shape the notch shows, as it told the engine: the shape is a clip
        // over a layout made for a bigger one, so a card can be there and cut off.
        var card = offer().getBoundingClientRect();
        var shape = last('notch_shape');
        check('all of it inside the notch’s shape, its answers too', card.left >= shape.x - 1 && card.right <= shape.x + shape.w + 1 && button('Not this one').getBoundingClientRect().bottom <= shape.y + shape.h + 1 && button('Take notes').getBoundingClientRect().left >= shape.x, JSON.stringify({ card: [Math.round(card.left), Math.round(card.right), Math.round(card.bottom)], shape: shape }));
      } else {
        check('and who else is in it, on hover', (offer().getAttribute('title') || '').indexOf('with Dana Kestrel and Lee Atlas') > 0, offer().getAttribute('title'));
      }
      check('it asks whether to take its notes, or not this one', !!button('Take notes') && !!button('Not this one') && !button('Take notes').disabled);
      // A take that fails says why, and the offer stays to try again.
      window.__MEETING_FAILS__ = 'There’s no account to start its session on.';
      button('Take notes').click();
      return until(function () { return offer() && offer().textContent.indexOf('no account to start') >= 0; }, 3000, 'why it couldn’t be taken');
    })
    .then(function () {
      var first = asked()[0];
      check('Take notes asks the engine to take it', !!first && first.id === 'E1@1794330000000' && first.take === true, JSON.stringify(first));
      check('a take that failed says why, and the offer stays', !!button('Take notes') && !button('Take notes').disabled);
      window.__MEETING_FAILS__ = null;
      // Taken, here or in another window: it says it's starting, and takes no second answer.
      return again(soon({ taking: true }));
    })
    .then(function () {
      check('one being taken says it’s starting, and can’t be answered twice', !!button('Starting…') && takes().disabled && button('Not this one').disabled, offer() ? offer().textContent : 'no offer');
      return again(soon({}));
    })
    .then(function () {
      var before = asked().length;
      button('Not this one').click();
      return until(function () { return asked().length > before; }, 2000, 'the engine asked to pass on it');
    })
    .then(function () {
      var last = asked()[asked().length - 1];
      check('Not this one passes on it', last.id === 'E1@1794330000000' && last.take === false, JSON.stringify(last));
      return again(null);
    })
    .then(function () { return until(function () { return !offer(); }, 3000, 'the offer to go'); })
    .then(function () { check('and it goes when the board has none on offer', !q('.ca-meeting')); })
    .then(function () { return where() === 'window' ? library() : null; })
    .then(done)
    .catch(stopped);
})();
