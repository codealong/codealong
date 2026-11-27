// Checks the limit resets in the status bar (ui/StatusBar.tsx, resets.ts): an account holding
// some and close to a limit has a clock on its chip, the others not (including one holding resets
// whose limits are low, and one close to a limit with none); hovering it brings up its card over
// the chip, which says how many, what kind and when the first expires, stays up while the pointer
// is on it, and asks again before Use a reset spends one, the first to expire, saying how that
// went in a toast. Cancel, a double-click on the button that asked, and moving to another
// account's card spend nothing; one its provider only counts can't be used from here, and its card
// says so.
//   PREVIEW_USAGE=scripts/preview/resets-usage-sample.json PREVIEW_RESETS=scripts/preview/resets-sample.json PREVIEW_WAIT=60000 scripts/preview.sh sessions '' scripts/preview/resets-check.js
(function () {
  var chipOf = function (account) {
    return all('.ca-statusbar-accts .ca-uchip').filter(function (c) { return (q('.ca-uchip-name', c) || {}).textContent === account; })[0];
  };
  var card = function () { return q('.ca-usage-popover.has-buttons'); };
  var button = function (text) { return all('.ca-usage-popover button').filter(function (b) { return b.textContent === text; })[0]; };
  var toast = function () { return (q('.ca-toast') || {}).textContent || ''; };
  // React hears the pointer coming onto an element, or off it, from mouseover and mouseout.
  var enter = function (el) { el.dispatchEvent(new MouseEvent('mouseover', { bubbles: true, relatedTarget: document.body })); };
  var leave = function (el) { el.dispatchEvent(new MouseEvent('mouseout', { bubbles: true, relatedTarget: document.body })); };

  until(function () { return q('.ca-statusbar-accts .ca-uchip .ca-uchip-resets'); }, 20000)
    .then(function () {
      var marked = all('.ca-statusbar-accts .ca-uchip').filter(function (c) { return q('.ca-uchip-resets', c); }).map(function (c) { return q('.ca-uchip-name', c).textContent; });
      check('only an account holding resets and close to a limit is marked', marked.sort().join(',') === 'antigravity,codex,cursor', marked.join(','));
      check('the chip says so to a screen reader', /\. 3 limit resets$/.test(chipOf('codex').getAttribute('aria-label')), chipOf('codex').getAttribute('aria-label'));
      enter(chipOf('codex'));
      return until(card, 3000);
    })
    .then(function () {
      check('its card counts them', q('.ca-resets-title', card()).textContent === '3 limit resets', q('.ca-resets-title', card()).textContent);
      check('and says what they are and when the first runs out', /^Full resets · the first expires [A-Z][a-z]{2} \d+$/.test(q('.ca-resets-text', card()).textContent), q('.ca-resets-text', card()).textContent);
      var early = (q('.ca-resets-early', card()) || {}).textContent || '';
      var fullest = Math.max.apply(null, all('.ca-meter-value', chipOf('codex')).map(function (v) { return parseInt(v.textContent, 10); }).concat([0]));
      check('it warns a reset would free little while the limits are low, and only then', fullest < 50 ? /would free little/.test(early) : early === '', fullest + '%: ' + early);
      var c = card().getBoundingClientRect();
      var at = chipOf('codex').getBoundingClientRect();
      check('it opens over the chip, inside the window', c.bottom <= at.top && c.top >= 0, JSON.stringify({ card: [c.top, c.bottom], chip: at.top }));
      leave(chipOf('codex'));
      enter(card());
      return wait(400);
    })
    .then(function () {
      check('the card stays while the pointer is on it', !!card());
      button('Use a reset…').click();
      return until(function () { return button('Cancel'); }, 2000);
    })
    .then(function () {
      check('Use a reset… asks first, spending nothing', /^Reset codex’s limits now\? This uses one\.$/.test(q('.ca-resets-ask', card()).textContent) && calls('use_reset').length === 0, q('.ca-resets-ask', card()).textContent);
      var foot = all('.ca-resets-foot button', card()).map(function (b) { return b.textContent; });
      check('Cancel takes the place of the button that asked', foot.join(',') === 'Use a reset,Cancel', foot.join(','));
      // A double-click's second click lands at once: it spends nothing.
      button('Use a reset').click();
      check('a click straight after the one that asked spends nothing', calls('use_reset').length === 0 && !!button('Cancel'));
      // Straight on to another account's card, as a pointer moving along the bar does.
      leave(card());
      enter(chipOf('cursor'));
      return until(function () { return card() && q('.ca-uchip-name', card()).textContent === 'cursor'; }, 3000);
    })
    .then(function () {
      check('another account’s card doesn’t open asking', !!button('Use a reset…') && !button('Cancel'), all('.ca-usage-popover button').map(function (b) { return b.textContent; }).join(','));
      check('one without a title says what a reset does', q('.ca-resets-text', card()).textContent === 'Each puts the plan’s limits back to empty' && q('.ca-resets-title', card()).textContent === '1 limit reset', q('.ca-resets-text', card()).textContent);
      leave(card());
      enter(chipOf('codex'));
      return until(function () { return card() && q('.ca-uchip-name', card()).textContent === 'codex' && button('Use a reset…'); }, 3000);
    })
    .then(function () {
      button('Use a reset…').click();
      return until(function () { return button('Cancel'); }, 2000);
    })
    .then(function () {
      button('Cancel').click();
      return until(function () { return button('Use a reset…'); }, 2000);
    })
    .then(function () {
      check('Cancel goes back, still spending nothing', calls('use_reset').length === 0);
      button('Use a reset…').click();
      return until(function () { return button('Use a reset'); }, 2000);
    })
    .then(function () { return wait(600); })
    .then(function () {
      button('Use a reset').click();
      return until(function () { return !card(); }, 3000);
    })
    .then(function () {
      var used = sent('use_reset');
      check('Use a reset spends the first to expire and puts the card away', used.length === 1 && used[0].name === 'codex' && used[0].credit === 'RateLimitResetCredit_a', JSON.stringify(used));
      return until(function () { return toast().indexOf('codex’s limits are reset. 2 left.') >= 0; }, 3000);
    })
    .then(function () {
      check('and a toast says it went, with what’s left', true);
      enter(chipOf('claude'));
      return wait(400);
    })
    .then(function () {
      check('an account holding resets below its limits has no card for them', !q('.ca-resets'));
      leave(chipOf('claude'));
      enter(chipOf('claude2'));
      return wait(400);
    })
    .then(function () {
      check('an account close to a limit without resets has no card for them', !q('.ca-resets'));
      leave(chipOf('claude2'));
      enter(chipOf('antigravity'));
      return until(function () { return q('.ca-usage-popover .ca-resets'); }, 3000);
    })
    .then(function () {
      var shown = q('.ca-usage-popover');
      check('one its provider only counts shows the count and why it can’t be used from here, with no button', q('.ca-resets-title', shown).textContent === '2 limit resets' && /leaves using one to it/.test((q('.ca-resets-early', shown) || {}).textContent || '') && !q('.ca-resets button', shown), shown.textContent);
      check('and its chip says why to a screen reader', /2 limit resets: Its provider says how many/.test(chipOf('antigravity').getAttribute('aria-label')), chipOf('antigravity').getAttribute('aria-label'));
    })
    .then(done)
    .catch(function (e) { stopped(e.message); });
})();
