// Checks the floating bar's snooze menus (Bar.tsx, snooze.ts): a session pill's Snooze submenu
// snoozes it, a folded pill's snoozes all of its sessions, and the snoozed pill opens, wakes or
// wakes all. It needs sessions in the feed's `snoozed`, which the engine lists only for sessions
// you snoozed; a PREVIEW_BIN that moves a few there does for a preview.
//   PREVIEW_WAIT=60000 scripts/preview.sh bar '' scripts/preview/snooze-check.js
(function () {
  var texts = function (items) { return items.map(function (i) { return i.item === 'Separator' ? '-' : i.text; }); };
  var find = function (items, text) { return items.filter(function (i) { return i.text && i.text.indexOf(text) === 0; })[0]; };
  var snoozes = function () { return calls('snooze_sessions').map(function (c) { return c[1]; }); };

  say('snooze check: waiting for pills');
  until(function () { return document.querySelector('.ca-hud [data-key]') && document.querySelector('.ca-hud .is-snoozed'); }, 20000)
    .then(function () {
      var pill = document.querySelector('.ca-hud [data-key]');
      rightClick(pill);
      return wait(200).then(function () {
        var menu = lastMenu();
        var group = find(menu, 'Snooze');
        check('a session pill has a Snooze submenu', !!group && group.items, JSON.stringify(texts(menu)));
        check('with four times', group && texts(group.items).join('|').indexOf('For 15 minutes|For 1 hour|For 3 hours|Until ') === 0, group && texts(group.items).join('|'));
        var before = Date.now();
        pick(find(group.items, 'For 1 hour'));
        return wait(100).then(function () {
          var s = snoozes().pop();
          check('it snoozes that session for an hour', s && s.keys.join() === pill.dataset.key && Math.abs(s.until - before - 3600000) < 60000, JSON.stringify(s));
        });
      });
    })
    .then(function () {
      var fold = document.querySelector('.ca-hud .ca-hud-fold.is-unread') || document.querySelector('.ca-hud .ca-hud-fold.is-needs');
      if (!fold) return check('a folded pill (none on the bar to check)', true);
      fold.click();
      return wait(200).then(function () {
        var menu = lastMenu();
        var group = find(menu, 'Snooze these');
        check('a folded pill snoozes its sessions together', !!group, JSON.stringify(texts(menu)));
        var n = menu.filter(function (i) { return i.text && !i.items && i.text.indexOf('Mark ') !== 0; }).length;
        pick(group.items[0]);
        return wait(100).then(function () {
          var s = snoozes().pop();
          check('all of them, for 15 minutes', s && s.keys.length === n && s.until - Date.now() > 14 * 60000, JSON.stringify(s));
        });
      });
    })
    .then(function () {
      var pill = document.querySelector('.ca-hud .is-snoozed');
      check('the snoozed pill says how many and until when', /^3 sessions snoozed, back on the bar when their time comes:\n.+ · until .+/.test(pill.title), pill.title);
      pill.click();
      return wait(200).then(function () {
        var menu = lastMenu();
        var entries = menu.filter(function (i) { return i.items; });
        check('its menu lists each with when it is back', entries.length === 3 && entries.every(function (e) { return / · until .+/.test(e.text); }), JSON.stringify(texts(menu)));
        check('and wakes them all', texts(menu).slice(-2).join('|') === '-|Wake all', JSON.stringify(texts(menu)));
        var first = entries[0];
        check('each shows in Codealong first, then opens, wakes or snoozes again', /^Show in Codealong\|(Show|Attach|Resume|Open|Respond).*\|-\|Wake now\|Snooze instead$/.test(texts(first.items).join('|')), texts(first.items).join('|'));
        pick(find(first.items, 'Wake now'));
        return wait(100).then(function () {
          var s = snoozes().pop();
          check('Wake now wakes that one', s && s.keys.length === 1 && s.until === null, JSON.stringify(s));
          pick(first.items[1]);
          return wait(300);
        }).then(function () {
          var s = snoozes().pop();
          var opened = calls('open_from_bar').pop();
          check('opening wakes it, then opens it', s && s.until === null && s.keys.length === 1 && !!opened && window.__calls.indexOf(opened) > window.__calls.lastIndexOf(calls('snooze_sessions').pop()), JSON.stringify([s, opened && opened[1]]));
          pick(find(menu, 'Wake all'));
          return wait(100);
        }).then(function () {
          var s = snoozes().pop();
          check('Wake all wakes the three', s && s.keys.length === 3 && s.until === null, JSON.stringify(s));
        });
      });
    })
    .then(done)
    .catch(stopped);
})();
