// Checks the quick panel's Mac and effort choices (Quick.tsx): ⌘D picks another Mac, whose
// accounts are asked of it; the agent menu's ← → set the effort, which the chip shows and Haiku
// doesn't take; and ⌘⏎ starts on that Mac's worktree at that effort. ⌘D is pressed the moment
// the machine chip is drawn, when the render that drew it may not have run its effects yet: the
// panel's keys go by what's on screen (quickKeys.ts `useQuickKeys`).
//   PREVIEW_DEVICES=scripts/preview/devices-sample.json PREVIEW_WAIT=60000 scripts/preview.sh quick '' scripts/preview/start-effort-check.js
(function () {
  localStorage.setItem('codealong.quick.project', 'All projects');
  var MINI = '9f8e7d6c5b';
  var chips = function () { return Array.prototype.slice.call(document.querySelectorAll('.ca-quick-bar .ca-quick-chip')); };
  var deviceChip = function () { return chips().find(function (b) { return b.title.indexOf('⌘D') >= 0; }); };
  var agentChip = function () { return document.querySelector('.ca-quick-agent .ca-quick-chip'); };
  var filter = function () { return document.querySelector('.ca-picker-filter'); };
  var effortOn = function () { var b = document.querySelector('.ca-picker-foot [aria-checked="true"]'); return b ? b.textContent : null; };
  var agentText = function () { return agentChip() ? agentChip().textContent : ''; };

  // ⌘D as the machine chip is first drawn, in the turn of the page's loop that drew it: the
  // effects of a render that data brought are often still to run then. The panel's listener read
  // the panel as the render that put it on had it, so a key pressed here went by the render
  // before, which knew of no other machine, and did nothing; a look every 10 ms lands here now
  // and then, more often on a slower machine.
  var pressed = false;
  var watch = new MutationObserver(function () {
    if (pressed || !deviceChip()) return;
    pressed = true;
    watch.disconnect();
    key(window, { key: 'd', metaKey: true });
  });
  watch.observe(document.documentElement, { childList: true, subtree: true, attributes: true });

  say('start effort check: waiting for the panel');
  until(function () { return pressed; }, 20000, 'the machine chip')
    .then(function () {
      return until(function () { return !!filter(); }, 5000, 'the machines’ menu, on ⌘D pressed as its chip was drawn');
    })
    .then(function () {
      var opts = Array.prototype.slice.call(document.querySelectorAll('.ca-picker-opt'));
      var studio = opts.find(function (o) { return o.textContent.indexOf('Studio') >= 0; });
      check('an away Mac can’t be picked', !!studio && studio.disabled);
      opts.find(function (o) { return o.textContent.indexOf('Mac mini') >= 0; }).click();
      return until(function () { return deviceChip().textContent.indexOf('Mac mini') >= 0 && !agentChip().disabled; }, 3000);
    })
    .then(function () {
      var asked = calls('device_start_options');
      check('its accounts are asked of it', asked.length > 0 && asked[asked.length - 1][1].device === MINI, JSON.stringify(asked.map(function (c) { return c[1]; })));
      key(window, { key: 'l', metaKey: true });
      return until(function () { return !!document.querySelector('.ca-picker-foot'); }, 2000);
    })
    .then(function () {
      check('the effort starts at the account’s own', effortOn() === 'Default', effortOn());
      for (var i = 0; i < 4; i++) key(filter(), { key: 'ArrowRight' });
      return wait(100);
    })
    .then(function () {
      check('→ steps up to Extra', effortOn() === 'Extra', effortOn());
      key(filter(), { key: 'ArrowRight' });
      key(filter(), { key: 'ArrowRight' });
      return wait(100);
    })
    .then(function () {
      check('and stops at Max', effortOn() === 'Max', effortOn());
      key(filter(), { key: 'ArrowLeft' });
      return wait(100);
    })
    .then(function () {
      key(filter(), { key: 'Enter' });
      return until(function () { return !filter(); }, 2000);
    })
    .then(function () {
      check('the chip shows it', /Extra effort/.test(agentText()), agentText());
      key(window, { key: 'l', metaKey: true });
      return until(function () { return !!filter(); }, 2000);
    })
    .then(function () {
      type(filter(), 'haiku');
      return wait(100);
    })
    .then(function () {
      key(filter(), { key: 'Enter' });
      return until(function () { return !filter(); }, 2000);
    })
    .then(function () {
      check('Haiku takes none', /Haiku/.test(agentText()) && !/effort/.test(agentText()), agentText());
      key(window, { key: 'l', metaKey: true });
      return until(function () { return !!filter(); }, 2000);
    })
    .then(function () {
      var note = document.querySelector('.ca-picker-foot-note');
      check('and the menu says so', !!note && note.textContent === 'Haiku takes none', note && note.textContent);
      type(filter(), 'auto');
      return wait(100);
    })
    .then(function () {
      key(filter(), { key: 'Enter' });
      return until(function () { return !filter(); }, 2000);
    })
    .then(function () {
      var input = document.querySelector('.ca-quick-input textarea');
      type(input, 'Fix the build');
      return wait(100).then(function () {
        key(window, { key: 'Enter', metaKey: true });
        return until(function () { return calls('start_from_quick').length > 0; }, 3000);
      });
    })
    .then(function () {
      var spec = calls('start_from_quick')[0][1].spec;
      check('it starts on that Mac', spec.laneId.indexOf('@' + MINI + ':') === 0, spec.laneId);
      check('at that effort', spec.effort === 'xhigh', spec.effort);
      check('where effort goes', ['terminal', 'background', 'codealong'].indexOf(spec.place) >= 0, spec.place);
    })
    .catch(function (e) { check('ran', false, String(e) + ' at ' + agentText()); })
    .then(done);
})();
