// Checks the training flags in the accounts bar (ui/UsageBar.tsx, training.ts): an account whose
// provider may train on it, or where nothing's chosen, has its chip in the attention colors with
// a shield, the others not; hovering it brings up its card, which says why, stays up while the
// pointer is on it, opens the provider's page, and turns training off, saying how that went in a
// toast; one that didn't take says why. Then Settings › Accounts tags each account it can tell
// about.
//   PREVIEW_TRAINING=scripts/preview/training-sample.json PREVIEW_WAIT=60000 scripts/preview.sh worktrees '' scripts/preview/training-check.js
(function () {
  var chipOf = function (account) {
    return all('.ca-usagebar-accts .ca-uchip').filter(function (c) { return (q('.ca-uchip-name', c) || {}).textContent === account; })[0];
  };
  var card = function () { return q('.ca-usage-popover.is-training'); };
  var button = function (text) { return all('.ca-usage-popover button').filter(function (b) { return b.textContent === text; })[0]; };
  var toast = function () { return (q('.ca-toast') || {}).textContent || ''; };
  // React hears the pointer coming onto an element, or off it, from mouseover and mouseout.
  var enter = function (el) { el.dispatchEvent(new MouseEvent('mouseover', { bubbles: true, relatedTarget: document.body })); };
  var leave = function (el) { el.dispatchEvent(new MouseEvent('mouseout', { bubbles: true, relatedTarget: document.body })); };

  until(function () { return q('.ca-usagebar-accts .ca-uchip.is-training'); }, 20000)
    .then(function () {
      var flagged = all('.ca-usagebar-accts .ca-uchip.is-training .ca-uchip-name').map(function (n) { return n.textContent; });
      check('only accounts that may be trained on are flagged', flagged.join(',') === 'claude,claude2', flagged.join(','));
      check('the chip itself says so, with no flag beside it', !q('.ca-uchip-flag') && /Training is on: Help improve Claude/.test(chipOf('claude').getAttribute('aria-label')), chipOf('claude').getAttribute('aria-label'));
      enter(chipOf('claude'));
      return until(card, 3000);
    })
    .then(function () {
      check('its hover card says why', q('.ca-training-title', card()).textContent === 'Training is on' && q('.ca-uchip-name', card()).textContent === 'claude' && /^Help improve Claude is on/.test(q('.ca-training-text', card()).textContent), q('.ca-training-title', card()).textContent);
      check('and when it was read', /^Read /.test((q('.ca-training-asof', card()) || {}).textContent || ''));
      // The pointer leaves the chip for the card: it stays up.
      leave(chipOf('claude'));
      enter(card());
      return wait(400);
    })
    .then(function () {
      check('the card stays while the pointer is on it', !!card());
      button('Open settings').click();
      check('Open settings opens the provider’s page', sent('open_url').some(function (a) { return a && a.url === 'https://claude.ai/settings/data-privacy-controls'; }), JSON.stringify(sent('open_url')));
      var press = button('Turn off training');
      press.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, cancelable: true, button: 0, buttons: 1 }));
      check('a press on its buttons leaves it up', !!card());
      press.click();
      return until(function () { return !card(); }, 3000);
    })
    .then(function () {
      check('Turn off training asks for it and puts the card away', sent('stop_training').some(function (a) { return a.name === 'claude'; }), JSON.stringify(sent('stop_training')));
      return until(function () { return toast().indexOf('Training is off for claude.') >= 0; }, 3000);
    })
    .then(function () {
      check('and a toast says it went', true);
      enter(chipOf('claude2'));
      return until(card, 3000);
    })
    .then(function () {
      button('Turn off training').click();
      return until(function () { return /still reads as on/.test(toast()); }, 3000);
    })
    .then(function () {
      check('one that didn’t take says why', /^Training for claude2 was asked to turn off/.test(toast()), toast());
      enter(chipOf('codex'));
      return wait(400);
    })
    .then(function () {
      check('an account not trained on has no card', !card());
      all('.ca-usagebar-end button').filter(function (b) { return /Accounts/.test(b.textContent); })[0].click();
      return until(function () { return q('#ca-acct-claude .ca-acct-tags'); }, 5000);
    })
    .then(function () {
      var tags = function (account) { return all('#ca-acct-' + account + ' .ca-acct-tags .ca-tag').map(function (t) { return t.textContent; }).slice(1).join(','); };
      check('Settings tags each account it can tell about', tags('claude') === 'Training on' && tags('codex') === 'Not trained on' && tags('opencode') === '', [tags('claude'), tags('codex'), tags('opencode')].join(' / '));
      var actions = all('#ca-acct-claude .ca-acct-actions button').map(function (b) { return b.textContent; });
      check('and offers to turn it off there', actions.indexOf('Turn off training') >= 0 && actions.indexOf('Open settings') >= 0, actions.join(' | '));
    })
    .then(done)
    .catch(function (e) { stopped(e.message); });
})();
