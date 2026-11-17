// Checks the card on a machine's load (LoadTag.tsx): this machine strained, the usage bar's
// "Machine strained" chip opens a card when the pointer rests on it, saying what strained means,
// its CPUs and memory, and how busy your other machines are (the Mac mini calm, the Studio away),
// with the quick panel's key for a calm one, nothing covering it; it goes when the pointer leaves.
// A Tab to the chip opens it too. A linked machine's tag in Settings › Devices, a dialog drawn
// over the cards, has its numbers in a tooltip instead.
//   PREVIEW_WAIT=60000 PREVIEW_DEVICES=scripts/preview/devices-sample.json scripts/preview.sh sessions '' scripts/preview/load-card-check.js
//   PREVIEW_WAIT=60000 PREVIEW_PLATFORM=linux PREVIEW_DEVICES=scripts/preview/devices-sample.json scripts/preview.sh sessions '' scripts/preview/load-card-check.js
(function () {
  var strained = { level: 'strained', perCpu: 9, cpus: 10, memoryFree: 45, memoryShort: true };
  // This runs before the app's script: the board says this machine is strained.
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    var p = real(cmd, args, options);
    return cmd === 'get_board' ? p.then(function (b) { b.load = strained; return b; }) : p;
  };

  var linux = window.__CODEALONG_PLATFORM__ === 'linux' || /\bLinux\b/.test(navigator.userAgent);
  var chip = function () { return q('.ca-usagebar-end .ca-load.is-bar'); };
  var card = function () { return q('.ca-loadcard'); };
  var enter = function (el) { el.dispatchEvent(new MouseEvent('mouseover', { bubbles: true, relatedTarget: document.body })); };
  var leave = function (el) { el.dispatchEvent(new MouseEvent('mouseout', { bubbles: true, relatedTarget: document.body })); };
  var machines = function () { return all('.ca-loadcard-machine').map(function (r) { return r.textContent.trim(); }); };

  say('load card check: waiting for the usage bar');
  until(function () { return !!chip() && calls('devices').length > 0; }, 20000, 'no Machine strained chip in the usage bar')
    .then(function () { return wait(500); })
    .then(function () {
      check('the chip says the machine is strained', /Machine strained/.test(chip().textContent), chip().textContent);
      check('no tooltip takes the card’s place', !chip().title, chip().title);
      enter(chip());
      return until(function () { return !!card(); }, 3000).catch(function () {});
    })
    .then(function () {
      var c = card();
      check('resting on the chip opens its card', !!c, 'no card');
      if (!c) return;
      check('it says this machine is strained and what that means', /This machine is strained/.test(c.textContent) && /everything on it waits/.test(c.textContent), c.textContent);
      check('it says its CPUs and memory', /9 per CPU over five minutes, 10 CPUs/.test(c.textContent) && /45% free, getting short/.test(c.textContent), c.textContent);
      var r = c.getBoundingClientRect();
      var hit = document.elementFromPoint(r.left + r.width / 2, r.top + r.height / 2);
      check('nothing covers it', !!hit && c.contains(hit), hit ? hit.className : 'nothing there');
      var rows = machines();
      check('it says how busy your other machines are', rows.some(function (r) { return /Mac mini/.test(r) && /Calm/.test(r); }) && rows.some(function (r) { return /Studio/.test(r) && /Away/.test(r); }), rows.join(' | '));
      check('and the quick panel’s key for a calm one', /offers a calm one/.test(c.textContent) && c.textContent.indexOf(linux ? 'Ctrl+Shift+D' : '⇧⌘D') >= 0, c.textContent);
      leave(chip());
      return wait(600).then(function () { check('it goes when the pointer leaves', !card(), card() ? card().textContent : ''); });
    })
    .then(function () {
      // From the keyboard: a Tab to the chip opens it, and leaving the chip puts it away.
      chip().focus();
      return until(function () { return !!card(); }, 3000).catch(function () {}).then(function () {
        check('a Tab to the chip opens its card', !!card() && document.activeElement === chip(), document.activeElement ? document.activeElement.className : '');
        chip().blur();
        return wait(600).then(function () { check('and it goes as the focus leaves', !card(), ''); });
      });
    })
    .then(function () {
      // Settings › Devices: the Mac mini's tag has a card of its own.
      q('.ca-side-config').click();
      return wait(400).then(function () {
        var nav = all('.ca-settings-nav button').filter(function (b) { return b.textContent.trim() === 'Devices'; })[0];
        if (nav) nav.click();
        return until(function () { return all('.ca-settings .ca-ucard .ca-load').length > 0; }, 5000).catch(function () {});
      });
    })
    .then(function () {
      // Settings is a dialog, drawn over the cards: its tags keep a tooltip.
      var tag = all('.ca-settings .ca-ucard .ca-load')[0];
      check('a linked machine’s card in Settings has its load', !!tag && /Calm/.test(tag.textContent), tag ? tag.textContent : 'no tag');
      if (!tag) return;
      check('with its numbers in a tooltip', /Mac mini is calm: load 0.3 per CPU of 8/.test(tag.title), tag.title);
      enter(tag);
      return wait(800).then(function () { check('and no card under the dialog', !card(), card() ? card().textContent : ''); });
    })
    .then(done, stopped);
})();
