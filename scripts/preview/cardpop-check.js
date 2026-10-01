// Checks the popover of a card pinned to the desktop (DesktopPopover.tsx): opened for a session
// with kin, it says how tall it is, lists where the session came from and what came of it, a click
// on one asks for its peek by the card, and a right-click gives that session's menu with Pin to
// desktop.
//   PREVIEW_PIN=session:<key> PREVIEW_WAIT=60000 scripts/preview.sh cardpop '' scripts/preview/cardpop-check.js
// Every step should say ok.
(function () {
  var out = [];
  var say = function (t) { window.__t = t; };
  setInterval(function () { if (window.__t) document.title = window.__t; }, 100);
  var wait = function (ms) { return new Promise(function (r) { setTimeout(r, ms); }); };
  var check = function (name, ok, why) { out.push(name + (ok ? ' ok' : ' FAIL') + (why ? ' (' + why + ')' : '')); };
  var called = function (cmd) { return window.__calls.filter(function (c) { return c[0] === cmd; }); };
  var run = async function () {
    for (var i = 0; i < 100 && !document.querySelector('.ca-cv-popover'); i++) await wait(100);
    var pop = document.querySelector('.ca-cv-popover');
    check('draws the card’s popover', !!pop);
    if (!pop) return say('done: ' + out.join('; '));
    await wait(600);
    // Its height as it opens, and again when that changes (fonts loading): the last said is what it is.
    var ready = called('card_popover_ready');
    var fits = called('card_popover_fit');
    var said = fits.length ? fits[fits.length - 1][1].height : ready.length ? ready[0][1].height : null;
    var h = Math.ceil(document.querySelector('.ca-deskpop').getBoundingClientRect().height);
    check('says how tall it is', ready.length === 1 && ready[0][1].id === 1 && said === h, said + ' vs ' + h);
    var labels = Array.prototype.map.call(pop.querySelectorAll('.ca-cvp-label'), function (el) { return el.textContent; });
    check('lists its kin', labels.length > 0, labels.join(', '));
    var line = pop.querySelector('.ca-cvp-sessionbtn');
    if (!line) return say('done: ' + out.concat('no session line FAIL').join('; '));
    line.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true, clientX: 40, clientY: 40 }));
    await wait(400);
    var items = (window.__menu() || []).map(function (x) { return x[0]; });
    check('a right-click gives its menu with Pin to desktop', items.indexOf('Pin to desktop') >= 0, items.slice(0, 4).join(' / '));
    var pin = (window.__menu() || []).find(function (x) { return x[0] === 'Pin to desktop'; });
    if (pin) pin[1]();
    await wait(200);
    var pinned = called('pin_to_desktop');
    check('which pins that session', pinned.length === 1 && pinned[0][1].pin.ref === 'session', JSON.stringify(pinned.map(function (c) { return c[1]; })).slice(0, 80));
    line.click();
    await wait(200);
    var peeked = called('card_popover_session');
    check('a click opens its peek by the card', peeked.length === 1 && !!peeked[0][1].key && peeked[0][1].key === (pinned[0] || [0, { pin: {} }])[1].pin.key, JSON.stringify(peeked.map(function (c) { return c[1]; })).slice(0, 80));
    window.__emit('card-popover-hidden', null);
    await wait(200);
    check('goes when told', !document.querySelector('.ca-cv-popover'));
    say('done: ' + out.join('; '));
  };
  setTimeout(run, 800);
})();
