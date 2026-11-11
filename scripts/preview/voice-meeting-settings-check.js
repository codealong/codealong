// Checks the meetings' settings (Settings › Experimental, ui/SettingsDialog.tsx `MeetingsFields`):
// with voice on, the offer to take a meeting's notes is off and shows nothing more; turning it
// on saves it and has macOS asked for the calendar, and what the calendar says shows under it
// (not asked yet, with a button to ask; then whose calendars are read and what's ahead), with
// what a meeting's session starts on, how long before a meeting it's offered, and every calendar
// under its account, to untick the ones whose meetings shouldn't be. Speakers or headphones
// isn't asked: it's heard (echo.rs).
//   PREVIEW_WAIT=60000 scripts/preview.sh sessions '' scripts/preview/voice-meeting-settings-check.js
(function () {
  // This runs before the app's script: config.toml reads back as it was saved, with voice on.
  var latest = null;
  var invoke = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    if (cmd === 'save_config') latest = args.config;
    var p = invoke(cmd, args, options);
    if (cmd !== 'get_config') return p;
    return p.then(function (view) {
      if (!latest) latest = Object.assign({}, view.config, { experimental: Object.assign({}, view.config.experimental, { voice: true }), voice: Object.assign({}, view.config.voice, { meetings: false }) });
      return Object.assign({}, view, { config: latest });
    });
  };
  // macOS hasn't asked yet, until the check says it has.
  window.__CALENDAR__ = { on: true, access: 'unasked', sources: [], meetings: 0, folder: '~/Library/Application Support/Codealong/meetings' };
  var field = function (label) {
    return all('.ca-settings-pane .ca-field').filter(function (f) { var s = f.querySelector(':scope > span:not(.ca-checkbox)'); return s && s.textContent === label; })[0];
  };
  var offer = function () { return field('Offer to take a meeting’s notes as it starts'); };
  var says = function () { var f = offer(); var all = f ? f.querySelectorAll('small') : []; return all.length > 1 ? all[all.length - 1].textContent : ''; };
  var asks = function () { return all('.ca-settings-pane button').filter(function (b) { return b.textContent.trim() === 'Ask macOS for my calendar'; })[0]; };
  var open = function () {
    var gear = q('.ca-side-config');
    var nav = all('.ca-settings .ca-nav, .ca-settings-nav button').filter(function (b) { return b.textContent.trim() === 'Experimental'; })[0];
    if (offer()) return true;
    if (nav) nav.click();
    else if (gear) gear.click();
    return false;
  };

  say('voice meeting settings check: opening Settings');
  until(open, 30000, 'Settings › Experimental, with voice on')
    .then(function () { return wait(400); })
    .then(function () {
      var box = offer().querySelector('input[type="checkbox"]');
      check('with voice on, the offer to take a meeting’s notes is there, and off', !!box && !box.checked);
      check('off, it says nothing of the calendar and asks nothing more', says() === '' && !field('A meeting’s session starts on') && !asks() && calls('calendar_access').length === 0, says());
      check('it says where meetings are kept', offer().textContent.indexOf('under ~/Library/Application Support/Codealong/meetings') > 0, offer().textContent.slice(0, 400));
      box.click();
      return until(function () { return calls('calendar_access').length > 0 && says() !== ''; }, 5000, 'macOS asked for the calendar');
    })
    .then(function () {
      var saved = sent('save_config').slice(-1)[0];
      check('turning it on saves it', !!saved && saved.config.voice.meetings === true, JSON.stringify(saved && saved.config.voice));
      check('and has macOS asked for the calendar', calls('calendar_access').length === 1);
      check('not asked yet, it says so and offers to ask', says() === 'macOS hasn’t asked yet whether Codealong may read your calendar.' && !!asks(), says());
      check('with what a meeting’s session starts on, and nothing to say of speakers', !!field('A meeting’s session starts on') && !field('I’m on speakers in my meetings'));
      // You said yes: it says whose calendars it reads and what's ahead.
      window.__CALENDAR__ = {
        on: true, access: 'allowed', sources: [{ title: 'sam@northwind.example', kind: 'caldav' }, { title: 'iCloud', kind: 'icloud' }], meetings: 2, nextAt: Date.now() + 40 * 60e3, folder: '~/Library/Application Support/Codealong/meetings',
        calendars: [{ id: 'C-family', title: 'Family', source: 'iCloud' }, { id: 'C-home', title: 'Home', source: 'iCloud' }, { id: 'C-work', title: 'Work', source: 'sam@northwind.example' }],
      };
      asks().click();
      return until(function () { return says().indexOf('Reading the calendars of') === 0; }, 5000, 'whose calendars are read');
    })
    .then(function () {
      check('allowed, it says whose calendars it reads and what’s ahead', /^Reading the calendars of sam@northwind\.example and iCloud\. 2 meetings in the day ahead, the next at \d{1,2}:\d\d/.test(says()) && !asks(), says());
      // Which calendars count: each listed under its account, ticked until you untick it.
      var accounts = all('.ca-settings-calaccount').map(function (a) { return a.querySelector('small').textContent + ': ' + all('.ca-settings-cal', a).map(function (c) { return c.textContent.trim(); }).join(', '); });
      check('every calendar is listed under its account', accounts.join(' | ') === 'iCloud: Family, Home | sam@northwind.example: Work', accounts.join(' | '));
      check('and counts until it’s unticked', all('.ca-settings-cal input[type="checkbox"]').every(function (b) { return b.checked; }));
      var home = all('.ca-settings-cal').filter(function (c) { return c.textContent.trim() === 'Home'; })[0];
      home.querySelector('input[type="checkbox"]').click();
      return until(function () { var s = sent('save_config').slice(-1)[0]; return s && (s.config.voice.meeting_calendars_off || []).length === 1; }, 3000, 'the calendar left out to be saved');
    })
    .then(function () {
      var off = sent('save_config').slice(-1)[0].config.voice.meeting_calendars_off;
      check('unticking one leaves it out, by its id', off.join(',') === 'C-home', off.join(','));
      // How long before a meeting it's offered.
      var lead = field('A meeting is offered').querySelector('select');
      check('a meeting is offered a minute before, to begin with', lead.value === '1', lead.value);
      Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype, 'value').set.call(lead, '5');
      lead.dispatchEvent(new Event('change', { bubbles: true }));
      return until(function () { var s = sent('save_config').slice(-1)[0]; return s && s.config.voice.meeting_lead === 5; }, 3000, 'how long before to be saved');
    })
    .then(function () {
      var voice = sent('save_config').slice(-1)[0].config.voice;
      check('and how long before is saved, with the calendar still left out', voice.meeting_lead === 5 && voice.meeting_calendars_off.join(',') === 'C-home', JSON.stringify(voice));
    })
    .then(done)
    .catch(stopped);
})();
