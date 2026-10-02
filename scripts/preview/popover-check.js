// Checks a canvas card's popover (card/CardPopover.tsx): it opens after the pointer rests on a
// card, stays while the pointer crosses to it and rests on it, lists where a session came from and
// what came of it, a click on one of those selects its card, and a right-click gives that
// session's menu with Pin to desktop. Run it on a canvas holding a session with an origin:
//   PREVIEW_CANVAS=<doc> PREVIEW_WAIT=60000 scripts/preview.sh canvas <project> scripts/preview/popover-check.js
(function () {
  var move = function (el, x, y) { el.dispatchEvent(new PointerEvent('pointermove', { bubbles: true, clientX: x, clientY: y })); };
  var pop = function () { return document.querySelector('.ca-cv-popover'); };
  var run = async function () {
    var card = null;
    for (var i = 0; i < 100 && !card; i++) {
      card = Array.prototype.find.call(document.querySelectorAll('.ca-cv-card.is-session'), function (el) {
        var id = el.getAttribute('data-id') || '';
        var key = id.slice('session:'.length);
        return window.__kin && window.__kin.has(key);
      });
      if (!card) await wait(100);
    }
    if (!card) return stopped('no session on the canvas that came of another or had one come of it');
    var r = card.getBoundingClientRect();
    move(card, r.left + 20, r.top + r.height / 2);
    await wait(700);
    check('opens after resting on the card', !!pop());
    if (!pop()) return done();
    var p = pop().getBoundingClientRect();
    // Across the gap: over empty canvas between the card and the popover, then on the popover.
    var gapX = p.left > r.right ? (r.right + p.left) / 2 : (p.right + r.left) / 2;
    move(document.querySelector('.ca-cv'), gapX, r.top + r.height / 2);
    await wait(120);
    move(pop(), p.left + 30, p.top + 20);
    await wait(700);
    check('stays while the pointer is on it', !!pop());
    var labels = Array.prototype.map.call(pop().querySelectorAll('.ca-cvp-label'), function (el) { return el.textContent; });
    check('lists its kin', labels.length > 0, labels.join(', '));
    var line = pop().querySelector('.ca-cvp-sessionbtn');
    check('its sessions take clicks', !!line);
    if (line) {
      rightClick(line, { clientX: 5, clientY: 5 });
      await wait(400);
      var items = (window.__menu() || []).map(function (x) { return x[0]; });
      check('a right-click gives its menu with Pin to desktop', items.indexOf('Pin to desktop') >= 0 || items.indexOf('Remove from desktop') >= 0, items.slice(0, 6).join(' / '));
      var title = line.getAttribute('title');
      line.click();
      await wait(400);
      var sel = document.querySelector('.ca-cv-card.is-selected');
      check('a click selects its card', !!sel && (sel.textContent || '').indexOf(title.slice(0, 20)) >= 0, sel ? sel.textContent.slice(0, 40) : 'none');
    }
    // Off it and off the card: it goes after a moment.
    move(document.querySelector('.ca-cv'), 5, 300);
    await wait(700);
    check('goes once the pointer leaves both', !pop());
    done();
  };
  // Sessions with kin, from the board the preview serves.
  var xhr = new XMLHttpRequest();
  xhr.open('GET', './board.json', false);
  xhr.send();
  var board = JSON.parse(xhr.responseText);
  window.__kin = new Set();
  board.lanes.forEach(function (l) { l.sessions.forEach(function (s) { if (s.origin) { window.__kin.add(s.key); window.__kin.add(s.origin.key); } }); });
  setTimeout(function () { run().catch(stopped); }, 1500);
})();
